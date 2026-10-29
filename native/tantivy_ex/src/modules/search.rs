use base64::{engine::general_purpose, Engine as _};
use rustler::{Encoder, Env, NifResult, ResourceArc, Term};
use tantivy::collector::TopDocs;
use tantivy::query::{Query, QueryParser};
use tantivy::schema::{Facet, FieldType, Value};
use tantivy::snippet::SnippetGenerator;
use tantivy::{DocAddress, Score, Searcher, TantivyDocument};

use crate::modules::resources::{QueryResource, SearcherResource};

// Search and retrieval functions

/// Searches with a query string, parsed with the index's tokenizers. The
/// default fields are the fields searched by terms without a field; when
/// empty, they are all the indexed text fields.
#[rustler::nif(schedule = "DirtyCpu")]
pub fn searcher_search<'a>(
    env: Env<'a>,
    searcher_res: ResourceArc<SearcherResource>,
    query_str: String,
    default_fields: Vec<String>,
    limit: usize,
    include_docs: bool,
) -> NifResult<Term<'a>> {
    let searcher = &searcher_res.searcher;
    let schema = searcher.schema();

    let fields = if default_fields.is_empty() {
        schema
            .fields()
            .filter(|(_, entry)| {
                matches!(entry.field_type(), FieldType::Str(_)) && entry.is_indexed()
            })
            .map(|(field, _)| field)
            .collect()
    } else {
        default_fields
            .iter()
            .map(|name| {
                schema.get_field(name).map_err(|_| {
                    rustler::Error::Term(Box::new(format!("Field not found: {}", name)))
                })
            })
            .collect::<NifResult<Vec<_>>>()?
    };

    let query = QueryParser::for_index(searcher.index(), fields)
        .parse_query(&query_str)
        .map_err(|e| rustler::Error::Term(Box::new(format!("Failed to parse query: {}", e))))?;

    search_results(env, searcher, &*query, limit, include_docs)
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn searcher_search_with_query<'a>(
    env: Env<'a>,
    searcher_res: ResourceArc<SearcherResource>,
    query_res: ResourceArc<QueryResource>,
    limit: usize,
    include_docs: bool,
) -> NifResult<Term<'a>> {
    search_results(
        env,
        &searcher_res.searcher,
        &*query_res.query,
        limit,
        include_docs,
    )
}

/// The `limit` best matches of `query`, as maps with their `"score"` and,
/// with `include_docs`, their stored fields. A field with several values
/// gives a list, a field with one value gives that value.
fn search_results<'a>(
    env: Env<'a>,
    searcher: &Searcher,
    query: &dyn Query,
    limit: usize,
    include_docs: bool,
) -> NifResult<Term<'a>> {
    let top_docs = TopDocs::with_limit(limit).order_by_score();
    let docs = searcher
        .search(query, &top_docs)
        .map_err(|e| rustler::Error::Term(Box::new(format!("Search failed: {}", e))))?;

    encode_hits(env, searcher, docs, include_docs)
}

/// The hits as maps with their `"score"` and, with `include_docs`, their
/// stored fields.
pub fn encode_hits<'a>(
    env: Env<'a>,
    searcher: &Searcher,
    docs: Vec<(Score, DocAddress)>,
    include_docs: bool,
) -> NifResult<Term<'a>> {
    let mut results = Vec::new();

    for (score, doc_address) in docs {
        let mut result = rustler::types::map::map_new(env)
            .map_put("score".encode(env), (score as f64).encode(env))?;

        if include_docs {
            let doc = match searcher.doc::<TantivyDocument>(doc_address) {
                Ok(doc) => doc,
                Err(_) => continue,
            };

            // Values grouped by field, in the order of their first value
            let mut fields: Vec<(&str, Vec<Term<'a>>)> = Vec::new();
            for (field, value) in doc.field_values() {
                let name = searcher.schema().get_field_name(field);
                let value = encode_value(env, value);
                match fields
                    .iter_mut()
                    .find(|(field_name, _)| *field_name == name)
                {
                    Some((_, values)) => values.push(value),
                    None => fields.push((name, vec![value])),
                }
            }

            for (name, mut values) in fields {
                let value = if values.len() == 1 {
                    values.pop().unwrap()
                } else {
                    values.encode(env)
                };
                result = result.map_put(name.encode(env), value)?;
            }
        }

        results.push(result);
    }

    Ok(results.encode(env))
}

fn encode_value<'a, 'b, V: Value<'b>>(env: Env<'a>, value: V) -> Term<'a> {
    // A stored facet is in its encoded form: it is decoded into its path
    if let Some(encoded) = value.as_facet() {
        match Facet::from_encoded(encoded.as_bytes().to_vec()) {
            Ok(facet) => facet.to_string().encode(env),
            Err(_) => encoded.encode(env),
        }
    } else if let Some(s) = value.as_str() {
        s.encode(env)
    } else if let Some(n) = value.as_u64() {
        n.encode(env)
    } else if let Some(n) = value.as_i64() {
        n.encode(env)
    } else if let Some(n) = value.as_f64() {
        n.encode(env)
    } else if let Some(b) = value.as_bool() {
        b.encode(env)
    } else if let Some(d) = value.as_datetime() {
        format!("{:?}", d).encode(env)
    } else if let Some(b) = value.as_bytes() {
        general_purpose::STANDARD.encode(b).encode(env)
    } else if let Some(obj_iter) = value.as_object() {
        let mut map = rustler::types::map::map_new(env);
        for (key, val) in obj_iter {
            if let Ok(updated) = map.map_put(key.encode(env), encode_value(env, val)) {
                map = updated;
            }
        }
        map
    } else if let Some(array_iter) = value.as_array() {
        array_iter
            .map(|val| encode_value(env, val))
            .collect::<Vec<_>>()
            .encode(env)
    } else if let Some(ip) = value.as_ip_addr() {
        // IPv4 addresses are indexed as IPv6 ones: they are given back as IPv4
        match ip.to_ipv4_mapped() {
            Some(ipv4) => ipv4.to_string().encode(env),
            None => ip.to_string().encode(env),
        }
    } else {
        rustler::types::atom::nil().encode(env)
    }
}

/// Generates a snippet of `text` for the terms of `query` in `field`.
///
/// The text is tokenized with the field's tokenizer, so matches follow the
/// same rules as the search. It does not need to be stored in the index.
/// Returns the fragment and the byte ranges `{start, end}` of its highlighted
/// terms, `end` being exclusive.
#[rustler::nif(schedule = "DirtyCpu")]
pub fn searcher_snippet(
    searcher_res: ResourceArc<SearcherResource>,
    query_res: ResourceArc<QueryResource>,
    field_name: String,
    text: String,
    max_num_chars: usize,
) -> NifResult<(String, Vec<(usize, usize)>)> {
    let field = searcher_res
        .searcher
        .schema()
        .get_field(&field_name)
        .map_err(|e| rustler::Error::Term(Box::new(format!("Field not found: {}", e))))?;

    let mut generator = SnippetGenerator::create(&searcher_res.searcher, &*query_res.query, field)
        .map_err(|e| {
            rustler::Error::Term(Box::new(format!(
                "Failed to create snippet generator: {}",
                e
            )))
        })?;
    generator.set_max_num_chars(max_num_chars);

    let snippet = generator.snippet(&text);
    let highlighted = snippet
        .highlighted()
        .iter()
        .map(|range| (range.start, range.end))
        .collect();

    Ok((snippet.fragment().to_string(), highlighted))
}
