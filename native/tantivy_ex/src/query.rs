use rustler::{NifResult, ResourceArc};
use std::ops::Bound;
use tantivy::Term as TantivyTerm;
use tantivy::query::{
    AllQuery, BooleanQuery, EmptyQuery, ExistsQuery, FuzzyTermQuery, MoreLikeThisQuery, Occur,
    PhrasePrefixQuery, PhraseQuery, Query, QueryParser, RangeQuery, RegexQuery, TermQuery,
};
use tantivy::schema::{IndexRecordOption, OwnedValue};

use crate::resources::{
    IndexResource, QueryParserResource, QueryResource, SchemaResource, error, schema_field,
};
use crate::terms::{text_terms, text_tokenizer, value_terms};

#[rustler::nif]
pub fn query_parser_new(
    index_res: ResourceArc<IndexResource>,
    default_fields: Vec<String>,
) -> NifResult<ResourceArc<QueryParserResource>> {
    if default_fields.is_empty() {
        return Err(error(
            "At least one default field is required for query parser",
        ));
    }

    let schema = index_res.index.schema();
    let fields = default_fields
        .iter()
        .map(|field_name| schema_field(&schema, field_name))
        .collect::<NifResult<Vec<_>>>()?;

    let parser = QueryParser::for_index(&index_res.index, fields);
    Ok(ResourceArc::new(QueryParserResource { parser }))
}

#[rustler::nif]
pub fn query_parser_parse(
    parser_res: ResourceArc<QueryParserResource>,
    query_str: String,
) -> NifResult<ResourceArc<QueryResource>> {
    if query_str.trim().is_empty() {
        return Err(error("Query string cannot be empty"));
    }

    match parser_res.parser.parse_query(&query_str) {
        Ok(query) => Ok(ResourceArc::new(QueryResource { query })),
        Err(e) => Err(error(format!("Failed to parse query: {}", e))),
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
    let field = schema_field(schema, &field_name)?;
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
    let field = schema_field(&schema_res.schema, &field_name)?;

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
    let field = schema_field(&schema_res.schema, &field_name)?;

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
    let field = schema_field(&schema_res.schema, &field_name)?;

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

    for query_res in must_queries {
        clauses.push((Occur::Must, query_res.query.box_clone()));
    }

    for query_res in should_queries {
        clauses.push((Occur::Should, query_res.query.box_clone()));
    }

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
    let field = schema_field(schema, &field_name)?;
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
    let field = schema_field(&schema_res.schema, &field_name)?;

    match RegexQuery::from_pattern(&pattern, field) {
        Ok(query) => Ok(ResourceArc::new(QueryResource {
            query: Box::new(query),
        })),
        Err(e) => Err(error(format!("Failed to create regex query: {}", e))),
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
    let field = schema_field(&schema_res.schema, &field_name)?;

    let regex_pattern = wildcard_regex(&pattern);

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
    let field = schema_field(&schema_res.schema, &field_name)?;

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
    let doc_data: serde_json::Value = match serde_json::from_str(&doc_json) {
        Ok(data) => data,
        Err(e) => return Err(error(format!("JSON parse error: {}", e))),
    };

    let mut doc_fields = Vec::new();
    if let serde_json::Value::Object(obj) = doc_data {
        for (field_name, value) in obj {
            if let Ok(field) = schema_res.schema.get_field(&field_name) {
                let mut field_values = Vec::new();

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
        return Err(error("No valid field values found in document"));
    }

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

    let query = builder.with_document_fields(doc_fields);

    Ok(ResourceArc::new(QueryResource {
        query: Box::new(query),
    }))
}

/// The terms of the query, as text, sorted and without duplicates (e.g. to
/// highlight them).
#[rustler::nif(schedule = "DirtyCpu")]
pub fn query_extract_terms(
    query_res: ResourceArc<QueryResource>,
    _schema_res: ResourceArc<SchemaResource>,
) -> Vec<String> {
    let mut terms = std::collections::BTreeSet::new();

    query_res.query.query_terms(&mut |term, _need_position| {
        let value = term.value();
        let text = if let Some(text) = value.as_str() {
            text.to_string()
        } else if let Some(number) = value.as_u64() {
            number.to_string()
        } else if let Some(number) = value.as_i64() {
            number.to_string()
        } else if let Some(number) = value.as_f64() {
            number.to_string()
        } else if let Some(text) = value.as_bytes().and_then(|b| std::str::from_utf8(b).ok()) {
            text.to_string()
        } else {
            return;
        };

        if !text.is_empty() {
            terms.insert(text);
        }
    });

    terms.into_iter().collect()
}

#[rustler::nif]
pub fn facet_term_query(
    schema_res: ResourceArc<SchemaResource>,
    field_name: String,
    facet_path: String,
) -> NifResult<ResourceArc<QueryResource>> {
    let field = crate::facet::facet_field(&schema_res.schema, &field_name)?;

    let facet = match tantivy::schema::Facet::from_text(&facet_path) {
        Ok(f) => f,
        Err(e) => return Err(error(format!("Invalid facet path '{}': {}", facet_path, e))),
    };

    let term = TantivyTerm::from_facet(field, &facet);
    let query = TermQuery::new(term, tantivy::schema::IndexRecordOption::Basic);
    let boxed_query: Box<dyn tantivy::query::Query> = Box::new(query);

    Ok(ResourceArc::new(QueryResource { query: boxed_query }))
}

/// The regular expression of a wildcard pattern: `*` matches any characters,
/// `?` one character, and the other characters themselves.
fn wildcard_regex(pattern: &str) -> String {
    let mut regex = String::new();
    for c in pattern.chars() {
        match c {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            c if "\\.+()|[]{}^$#&-~".contains(c) => {
                regex.push('\\');
                regex.push(c);
            }
            c => regex.push(c),
        }
    }
    regex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards_escape_the_other_characters() {
        assert_eq!(wildcard_regex("ab*"), "ab.*");
        assert_eq!(wildcard_regex("a?c"), "a.c");
        assert_eq!(wildcard_regex("a.c"), "a\\.c");
        assert_eq!(wildcard_regex("(x)"), "\\(x\\)");
    }
}
