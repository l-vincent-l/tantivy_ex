use rustler::{NifResult, ResourceArc};
use serde_json;
use std::sync::{Mutex, PoisonError};
use tantivy::collector::FacetCollector;
use tantivy::query::{AllQuery, BooleanQuery, Occur, Query, TermQuery};
use tantivy::schema::{Facet, Field, FieldType, IndexRecordOption, Schema};
use tantivy::Term as TantivyTerm;

use crate::modules::resources::{json_to_term, QueryResource, SchemaResource, SearcherResource};

/// The field and facets of a FacetCollector, built for each search: the
/// resource is not locked while searching.
pub struct FacetCollectorResource {
    pub field_name: String,
    pub facets: Mutex<Vec<Facet>>,
}

#[rustler::resource_impl]
impl rustler::Resource for FacetCollectorResource {}

/// Resource for managing Facet state
pub struct FacetResource {
    pub facet: Facet,
}

#[rustler::resource_impl]
impl rustler::Resource for FacetResource {}

/// Creates a new facet collector for the specified field
#[rustler::nif]
pub fn facet_collector_for_field(
    field_name: String,
) -> NifResult<ResourceArc<FacetCollectorResource>> {
    Ok(ResourceArc::new(FacetCollectorResource {
        field_name,
        facets: Mutex::new(Vec::new()),
    }))
}

/// Adds a facet path to the collector for counting
#[rustler::nif]
pub fn facet_collector_add_facet(
    collector_res: ResourceArc<FacetCollectorResource>,
    facet_path: String,
) -> NifResult<rustler::Atom> {
    let facet = match Facet::from_text(&facet_path) {
        Ok(f) => f,
        Err(e) => {
            return Err(rustler::Error::Term(Box::new(format!(
                "Invalid facet path '{}': {}",
                facet_path, e
            ))))
        }
    };

    // A panic cannot leave the list half-updated: it stays usable after one
    collector_res
        .facets
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(facet);

    Ok(rustler::types::atom::ok())
}

/// Performs a search with facet collection
#[rustler::nif(schedule = "DirtyCpu")]
pub fn facet_search<'a>(
    env: rustler::Env<'a>,
    searcher_res: ResourceArc<SearcherResource>,
    query_res: ResourceArc<QueryResource>,
    collector_res: ResourceArc<FacetCollectorResource>,
) -> NifResult<rustler::Term<'a>> {
    let mut collector = FacetCollector::for_field(&collector_res.field_name);
    let facets = collector_res
        .facets
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    for facet in facets {
        collector.add_facet(facet);
    }

    match searcher_res.searcher.search(&*query_res.query, &collector) {
        Ok(facet_counts) => {
            // Convert FacetCounts to a nested JSON structure
            let mut result = serde_json::Map::new();

            // Get all facets and their counts
            let all_facets: Vec<(&Facet, u64)> = facet_counts.get("/").collect();

            // Build hierarchical structure
            for (facet, count) in all_facets {
                let facet_path = facet.to_string();
                insert_facet_hierarchically(&mut result, &facet_path, count);
            }

            Ok(json_to_term(env, &serde_json::Value::Object(result)))
        }
        Err(e) => Err(rustler::Error::Term(Box::new(format!(
            "Faceted search failed: {}",
            e
        )))),
    }
}

/// Helper function to insert facet counts hierarchically
fn insert_facet_hierarchically(
    result: &mut serde_json::Map<String, serde_json::Value>,
    facet_path: &str,
    count: u64,
) {
    let segments: Vec<&str> = facet_path.split('/').filter(|s| !s.is_empty()).collect();

    if segments.is_empty() {
        return;
    }

    // Build the full path
    let full_path = if facet_path.starts_with('/') {
        facet_path.to_string()
    } else {
        format!("/{}", facet_path)
    };

    // Simply insert the count at the full path
    result.insert(
        full_path,
        serde_json::Value::Number(serde_json::Number::from(count)),
    );
}

/// Creates a multi-facet boolean query
/// The field named `field_name`, which must be a facet field: a facet term on
/// another field would match its own terms (the text token "public" for the
/// facet "/public").
pub fn facet_field(schema: &Schema, field_name: &str) -> NifResult<Field> {
    let field = schema.get_field(field_name).map_err(|_| {
        rustler::Error::Term(Box::new(format!(
            "Field '{}' not found in schema",
            field_name
        )))
    })?;

    match schema.get_field_entry(field).field_type() {
        FieldType::Facet(_) => Ok(field),
        _ => Err(rustler::Error::Term(Box::new(format!(
            "Field '{}' is not a facet field",
            field_name
        )))),
    }
}

/// A query matching the documents with the facets (or their descendants),
/// combined with `occur`: any of them ("should"), all of them ("must"), or
/// none of them ("must_not").
#[rustler::nif]
pub fn facet_multi_query(
    schema_res: ResourceArc<SchemaResource>,
    field_name: String,
    facet_paths: Vec<String>,
    occur_str: String,
) -> NifResult<ResourceArc<QueryResource>> {
    let field = facet_field(&schema_res.schema, &field_name)?;

    let occur = match occur_str.as_str() {
        "should" => Occur::Should,
        "must" => Occur::Must,
        "must_not" => Occur::MustNot,
        _ => {
            return Err(rustler::Error::Term(Box::new(format!(
                "Invalid occur '{}': expected should, must or must_not",
                occur_str
            ))))
        }
    };

    let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();
    for facet_path in facet_paths {
        let facet = Facet::from_text(&facet_path).map_err(|e| {
            rustler::Error::Term(Box::new(format!(
                "Invalid facet path '{}': {}",
                facet_path, e
            )))
        })?;
        let term = TantivyTerm::from_facet(field, &facet);
        clauses.push((
            occur,
            Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
        ));
    }

    // Excluded clauses alone match nothing: they exclude from all documents
    if occur == Occur::MustNot {
        clauses.push((Occur::Must, Box::new(AllQuery)));
    }

    Ok(ResourceArc::new(QueryResource {
        query: Box::new(BooleanQuery::new(clauses)),
    }))
}

/// Creates a facet from text
#[rustler::nif]
pub fn facet_from_text(facet_path: String) -> NifResult<ResourceArc<FacetResource>> {
    match Facet::from_text(&facet_path) {
        Ok(facet) => Ok(ResourceArc::new(FacetResource { facet })),
        Err(e) => Err(rustler::Error::Term(Box::new(format!(
            "Invalid facet path '{}': {}",
            facet_path, e
        )))),
    }
}

/// Converts a facet to string
#[rustler::nif]
pub fn facet_to_string(facet_res: ResourceArc<FacetResource>) -> NifResult<String> {
    Ok(facet_res.facet.to_string())
}
