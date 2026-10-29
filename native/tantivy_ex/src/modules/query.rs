use rustler::{NifResult, ResourceArc};
use serde_json;
use std::ops::Bound;
use tantivy::query::{
    AllQuery, BooleanQuery, EmptyQuery, ExistsQuery, FuzzyTermQuery, MoreLikeThisQuery, Occur,
    PhrasePrefixQuery, PhraseQuery, Query, QueryParser, RangeQuery, RegexQuery, TermQuery,
};
use tantivy::schema::{IndexRecordOption, OwnedValue};
use tantivy::Term as TantivyTerm;

use crate::modules::resources::{
    error, IndexResource, QueryParserResource, QueryResource, SchemaResource,
};
use crate::modules::terms::{text_terms, text_tokenizer, value_terms};

/// Query system functions

#[rustler::nif]
pub fn query_parser_new(
    index_res: ResourceArc<IndexResource>,
    default_fields: Vec<String>,
) -> NifResult<ResourceArc<QueryParserResource>> {
    // Need at least one default field
    if default_fields.is_empty() {
        return Err(rustler::Error::Term(Box::new(
            "At least one default field is required for query parser",
        )));
    }

    // Convert field names to Field objects
    let mut fields = Vec::new();
    for field_name in default_fields {
        if let Ok(field) = index_res.index.schema().get_field(&field_name) {
            fields.push(field);
        } else {
            return Err(rustler::Error::Term(Box::new(format!(
                "Field '{}' not found in schema",
                field_name
            ))));
        }
    }

    // If we couldn't find any of the fields, return error
    if fields.is_empty() {
        return Err(rustler::Error::Term(Box::new(
            "None of the specified fields were found in schema",
        )));
    }

    // Create the parser using fields we found
    let parser = QueryParser::for_index(&index_res.index, fields);
    Ok(ResourceArc::new(QueryParserResource { parser }))
}

#[rustler::nif]
pub fn query_parser_parse(
    parser_res: ResourceArc<QueryParserResource>,
    query_str: String,
) -> NifResult<ResourceArc<QueryResource>> {
    // Check if query string is empty
    if query_str.trim().is_empty() {
        return Err(rustler::Error::Term(Box::new(
            "Query string cannot be empty",
        )));
    }

    match parser_res.parser.parse_query(&query_str) {
        Ok(query) => Ok(ResourceArc::new(QueryResource { query })),
        Err(e) => Err(rustler::Error::Term(Box::new(format!(
            "Failed to parse query: {}",
            e
        )))),
    }
}

#[rustler::nif]
pub fn query_term(
    schema_res: ResourceArc<SchemaResource>,
    field_name: String,
    term_value: String,
) -> NifResult<ResourceArc<QueryResource>> {
    // Every term of the value must be in the field; a value without terms
    // matches nothing
    let (_field, terms) =
        value_terms(&schema_res.schema, &field_name, &term_value).map_err(error)?;
    let mut clauses: Vec<(Occur, Box<dyn Query>)> = terms
        .into_iter()
        .map(|term| {
            let query: Box<dyn Query> = Box::new(TermQuery::new(term, IndexRecordOption::Basic));
            (Occur::Must, query)
        })
        .collect();

    let query: Box<dyn Query> = match clauses.len() {
        0 => Box::new(EmptyQuery),
        1 => clauses.pop().unwrap().1,
        _ => Box::new(BooleanQuery::new(clauses)),
    };
    Ok(ResourceArc::new(QueryResource { query }))
}

/// A phrase query: the words, each analyzed with the field's tokenizer, in
/// this order.
#[rustler::nif]
pub fn query_phrase(
    schema_res: ResourceArc<SchemaResource>,
    field_name: String,
    phrase_terms: Vec<String>,
) -> NifResult<ResourceArc<QueryResource>> {
    let schema = &schema_res.schema;
    let field = schema
        .get_field(&field_name)
        .map_err(|_| error(format!("Field '{}' not found", field_name)))?;
    let tokenizer = text_tokenizer(schema, field, &field_name).map_err(error)?;

    let mut terms = Vec::new();
    for word in &phrase_terms {
        terms.extend(text_terms(field, &field_name, &tokenizer, word).map_err(error)?);
    }

    let query: Box<dyn Query> = match terms.len() {
        0 => Box::new(EmptyQuery),
        1 => Box::new(TermQuery::new(terms.remove(0), IndexRecordOption::Basic)),
        _ => Box::new(PhraseQuery::new(terms)),
    };
    Ok(ResourceArc::new(QueryResource { query }))
}

#[rustler::nif]
pub fn query_range_u64(
    schema_res: ResourceArc<SchemaResource>,
    field_name: String,
    start: Option<u64>,
    end: Option<u64>,
) -> NifResult<ResourceArc<QueryResource>> {
    let field = match schema_res.schema.get_field(&field_name) {
        Ok(field) => field,
        Err(_) => {
            return Err(rustler::Error::Term(Box::new(format!(
                "Field '{}' not found",
                field_name
            ))))
        }
    };

    let lower_bound = start.map_or(Bound::Unbounded, |s| {
        Bound::Included(TantivyTerm::from_field_u64(field, s))
    });
    let upper_bound = end.map_or(Bound::Unbounded, |e| {
        Bound::Included(TantivyTerm::from_field_u64(field, e))
    });
    let query = RangeQuery::new(lower_bound, upper_bound);
    Ok(ResourceArc::new(QueryResource {
        query: Box::new(query),
    }))
}

#[rustler::nif]
pub fn query_range_i64(
    schema_res: ResourceArc<SchemaResource>,
    field_name: String,
    start: Option<i64>,
    end: Option<i64>,
) -> NifResult<ResourceArc<QueryResource>> {
    let field = match schema_res.schema.get_field(&field_name) {
        Ok(field) => field,
        Err(_) => {
            return Err(rustler::Error::Term(Box::new(format!(
                "Field '{}' not found",
                field_name
            ))))
        }
    };

    let lower_bound = start.map_or(Bound::Unbounded, |s| {
        Bound::Included(TantivyTerm::from_field_i64(field, s))
    });
    let upper_bound = end.map_or(Bound::Unbounded, |e| {
        Bound::Included(TantivyTerm::from_field_i64(field, e))
    });
    let query = RangeQuery::new(lower_bound, upper_bound);
    Ok(ResourceArc::new(QueryResource {
        query: Box::new(query),
    }))
}

#[rustler::nif]
pub fn query_range_f64(
    schema_res: ResourceArc<SchemaResource>,
    field_name: String,
    start: Option<f64>,
    end: Option<f64>,
) -> NifResult<ResourceArc<QueryResource>> {
    let field = match schema_res.schema.get_field(&field_name) {
        Ok(field) => field,
        Err(_) => {
            return Err(rustler::Error::Term(Box::new(format!(
                "Field '{}' not found",
                field_name
            ))))
        }
    };

    let lower_bound = start.map_or(Bound::Unbounded, |s| {
        Bound::Included(TantivyTerm::from_field_f64(field, s))
    });
    let upper_bound = end.map_or(Bound::Unbounded, |e| {
        Bound::Included(TantivyTerm::from_field_f64(field, e))
    });
    let query = RangeQuery::new(lower_bound, upper_bound);
    Ok(ResourceArc::new(QueryResource {
        query: Box::new(query),
    }))
}

#[rustler::nif]
pub fn query_boolean(
    must_queries: Vec<ResourceArc<QueryResource>>,
    should_queries: Vec<ResourceArc<QueryResource>>,
    must_not_queries: Vec<ResourceArc<QueryResource>>,
) -> NifResult<ResourceArc<QueryResource>> {
    let mut clauses = Vec::new();

    // Add MUST clauses (AND)
    for query_res in must_queries {
        clauses.push((Occur::Must, query_res.query.box_clone()));
    }

    // Add SHOULD clauses (OR)
    for query_res in should_queries {
        clauses.push((Occur::Should, query_res.query.box_clone()));
    }

    // Add MUST NOT clauses (NOT)
    for query_res in must_not_queries {
        clauses.push((Occur::MustNot, query_res.query.box_clone()));
    }

    let boolean_query = BooleanQuery::new(clauses);

    Ok(ResourceArc::new(QueryResource {
        query: Box::new(boolean_query),
    }))
}

/// A fuzzy query: the word, analyzed with the field's tokenizer (lowercased by
/// most), within `distance` edits.
#[rustler::nif]
pub fn query_fuzzy(
    schema_res: ResourceArc<SchemaResource>,
    field_name: String,
    term_value: String,
    distance: u8,
    prefix: bool,
) -> NifResult<ResourceArc<QueryResource>> {
    let schema = &schema_res.schema;
    let field = schema
        .get_field(&field_name)
        .map_err(|_| error(format!("Field '{}' not found", field_name)))?;
    let tokenizer = text_tokenizer(schema, field, &field_name).map_err(error)?;

    let mut terms = text_terms(field, &field_name, &tokenizer, &term_value).map_err(error)?;
    if terms.len() != 1 {
        return Err(error(format!(
            "A fuzzy query takes one word, {:?} gives {} terms",
            term_value,
            terms.len()
        )));
    }

    let query = FuzzyTermQuery::new(terms.remove(0), distance, prefix);
    Ok(ResourceArc::new(QueryResource {
        query: Box::new(query),
    }))
}

#[rustler::nif]
pub fn query_regex(
    schema_res: ResourceArc<SchemaResource>,
    field_name: String,
    pattern: String,
) -> NifResult<ResourceArc<QueryResource>> {
    let field = match schema_res.schema.get_field(&field_name) {
        Ok(field) => field,
        Err(_) => {
            return Err(rustler::Error::Term(Box::new(format!(
                "Field '{}' not found",
                field_name
            ))))
        }
    };

    match RegexQuery::from_pattern(&pattern, field) {
        Ok(query) => Ok(ResourceArc::new(QueryResource {
            query: Box::new(query),
        })),
        Err(e) => Err(rustler::Error::Term(Box::new(format!(
            "Failed to create regex query: {}",
            e
        )))),
    }
}

/// A wildcard query: `*` matches any characters, `?` one character, and the
/// other characters themselves. The pattern is matched against the indexed
/// terms, without analysis.
#[rustler::nif]
pub fn query_wildcard(
    schema_res: ResourceArc<SchemaResource>,
    field_name: String,
    pattern: String,
) -> NifResult<ResourceArc<QueryResource>> {
    let field = schema_res
        .schema
        .get_field(&field_name)
        .map_err(|_| error(format!("Field '{}' not found", field_name)))?;

    let mut regex_pattern = String::new();
    for c in pattern.chars() {
        match c {
            '*' => regex_pattern.push_str(".*"),
            '?' => regex_pattern.push('.'),
            c if "\\.+()|[]{}^$#&-~".contains(c) => {
                regex_pattern.push('\\');
                regex_pattern.push(c);
            }
            c => regex_pattern.push(c),
        }
    }

    let query = RegexQuery::from_pattern(&regex_pattern, field)
        .map_err(|e| error(format!("Failed to create wildcard query: {}", e)))?;
    Ok(ResourceArc::new(QueryResource {
        query: Box::new(query),
    }))
}

#[rustler::nif]
pub fn query_phrase_prefix(
    schema_res: ResourceArc<SchemaResource>,
    field_name: String,
    phrase_terms: Vec<String>,
    max_expansions: u32,
) -> NifResult<ResourceArc<QueryResource>> {
    let field = match schema_res.schema.get_field(&field_name) {
        Ok(field) => field,
        Err(_) => {
            return Err(rustler::Error::Term(Box::new(format!(
                "Field '{}' not found",
                field_name
            ))))
        }
    };

    let terms: Vec<TantivyTerm> = phrase_terms
        .iter()
        .map(|term_str| TantivyTerm::from_field_text(field, term_str))
        .collect();

    let mut query = PhrasePrefixQuery::new(terms);
    query.set_max_expansions(max_expansions);
    Ok(ResourceArc::new(QueryResource {
        query: Box::new(query),
    }))
}

#[rustler::nif]
pub fn query_exists(
    _schema_res: ResourceArc<SchemaResource>,
    field_name: String,
) -> NifResult<ResourceArc<QueryResource>> {
    // In tantivy 0.24.1, ExistsQuery::new takes field name and json_subpaths boolean
    let query = ExistsQuery::new(field_name, false);
    Ok(ResourceArc::new(QueryResource {
        query: Box::new(query),
    }))
}

#[rustler::nif]
pub fn query_all() -> ResourceArc<QueryResource> {
    let query = AllQuery;
    ResourceArc::new(QueryResource {
        query: Box::new(query),
    })
}

#[rustler::nif]
pub fn query_empty() -> ResourceArc<QueryResource> {
    let query = EmptyQuery;
    ResourceArc::new(QueryResource {
        query: Box::new(query),
    })
}

// One argument per option of TantivyEx.Query.more_like_this/3
#[allow(clippy::too_many_arguments)]
#[rustler::nif]
pub fn query_more_like_this(
    schema_res: ResourceArc<SchemaResource>,
    doc_json: String,
    min_doc_frequency: Option<u64>,
    max_doc_frequency: Option<u64>,
    min_term_frequency: Option<usize>,
    max_query_terms: Option<usize>,
    min_word_length: Option<usize>,
    max_word_length: Option<usize>,
    boost_factor: Option<f32>,
) -> NifResult<ResourceArc<QueryResource>> {
    // Parse the document JSON to extract field values
    let doc_data: serde_json::Value = match serde_json::from_str(&doc_json) {
        Ok(data) => data,
        Err(e) => {
            return Err(rustler::Error::Term(Box::new(format!(
                "JSON parse error: {}",
                e
            ))))
        }
    };

    // Extract field values from the JSON document
    let mut doc_fields = Vec::new();
    if let serde_json::Value::Object(obj) = doc_data {
        for (field_name, value) in obj {
            // Get the Field from schema
            if let Ok(field) = schema_res.schema.get_field(&field_name) {
                let mut field_values = Vec::new();

                // Convert JSON value to tantivy OwnedValue
                match value {
                    serde_json::Value::String(s) => {
                        field_values.push(OwnedValue::Str(s));
                    }
                    serde_json::Value::Number(n) => {
                        if let Some(i) = n.as_u64() {
                            field_values.push(OwnedValue::U64(i));
                        } else if let Some(i) = n.as_i64() {
                            field_values.push(OwnedValue::I64(i));
                        } else if let Some(f) = n.as_f64() {
                            field_values.push(OwnedValue::F64(f));
                        }
                    }
                    serde_json::Value::Bool(b) => {
                        field_values.push(OwnedValue::Bool(b));
                    }
                    serde_json::Value::Array(arr) => {
                        // Handle arrays by adding each element
                        // Non-string array elements are skipped for now
                        for item in arr {
                            if let serde_json::Value::String(s) = item {
                                field_values.push(OwnedValue::Str(s));
                            }
                        }
                    }
                    _ => {} // Skip other types for now
                }

                if !field_values.is_empty() {
                    doc_fields.push((field, field_values));
                }
            }
        }
    }

    if doc_fields.is_empty() {
        return Err(rustler::Error::Term(Box::new(
            "No valid field values found in document",
        )));
    }

    // Build the More Like This query
    let mut builder = MoreLikeThisQuery::builder();

    if let Some(min_doc_freq) = min_doc_frequency {
        builder = builder.with_min_doc_frequency(min_doc_freq);
    }

    if let Some(max_doc_freq) = max_doc_frequency {
        builder = builder.with_max_doc_frequency(max_doc_freq);
    }

    if let Some(min_term_freq) = min_term_frequency {
        builder = builder.with_min_term_frequency(min_term_freq);
    }

    if let Some(max_query_terms) = max_query_terms {
        builder = builder.with_max_query_terms(max_query_terms);
    }

    if let Some(min_word_len) = min_word_length {
        builder = builder.with_min_word_length(min_word_len);
    }

    if let Some(max_word_len) = max_word_length {
        builder = builder.with_max_word_length(max_word_len);
    }

    if let Some(boost) = boost_factor {
        builder = builder.with_boost_factor(boost);
    }

    // Create the query with document fields
    let query = builder.with_document_fields(doc_fields);

    Ok(ResourceArc::new(QueryResource {
        query: Box::new(query),
    }))
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn query_extract_terms(
    query_res: ResourceArc<QueryResource>,
    _schema_res: ResourceArc<SchemaResource>,
) -> Vec<String> {
    // Extract terms using Tantivy's term extraction capability
    let mut term_set = std::collections::BTreeSet::new();

    // Extract terms from the query
    query_res.query.query_terms(&mut |term, _need_position| {
        // Get the term's value and try different decoding methods
        let value = term.value();

        // Try as_str() for text fields
        if let Some(text) = value.as_str() {
            if !text.is_empty() {
                term_set.insert(text.to_string());
            }
            return; // Found text, no need to try other methods
        }

        // Try as_u64() for numeric fields
        if let Some(num) = value.as_u64() {
            term_set.insert(num.to_string());
            return;
        }

        // Try as_i64() for signed numeric fields
        if let Some(num) = value.as_i64() {
            term_set.insert(num.to_string());
            return;
        }

        // Try as_f64() for float fields
        if let Some(num) = value.as_f64() {
            term_set.insert(num.to_string());
            return;
        }

        // Try as_bytes() as a fallback
        if let Some(bytes) = value.as_bytes() {
            if let Ok(text) = std::str::from_utf8(bytes) {
                if !text.is_empty() {
                    term_set.insert(text.to_string());
                }
            }
        }
    });

    term_set.into_iter().collect()
}

#[rustler::nif]
pub fn facet_term_query(
    schema_res: ResourceArc<SchemaResource>,
    field_name: String,
    facet_path: String,
) -> NifResult<ResourceArc<QueryResource>> {
    let field = crate::modules::facet::facet_field(&schema_res.schema, &field_name)?;

    // Parse the facet from the path string
    let facet = match tantivy::schema::Facet::from_text(&facet_path) {
        Ok(f) => f,
        Err(e) => {
            return Err(rustler::Error::Term(Box::new(format!(
                "Invalid facet path '{}': {}",
                facet_path, e
            ))))
        }
    };

    // Create the term query for the facet
    let term = TantivyTerm::from_facet(field, &facet);
    let query = TermQuery::new(term, tantivy::schema::IndexRecordOption::Basic);
    let boxed_query: Box<dyn tantivy::query::Query> = Box::new(query);

    Ok(ResourceArc::new(QueryResource { query: boxed_query }))
}
