use rustler::{Encoder, Env, NifResult, ResourceArc, Term};
use tantivy::Searcher;
use tantivy::aggregation::agg_req::{Aggregations, get_fast_field_names};
use tantivy::aggregation::agg_result::AggregationResults;
use tantivy::aggregation::{AggContextParams, AggregationCollector, AggregationLimitsGuard};
use tantivy::collector::{MultiCollector, TopDocs};

use crate::resources::{QueryResource, SearcherResource, error, json_to_term};
use crate::search::encode_hits;

// The aggregations are Tantivy's, in their Elasticsearch-compatible JSON:
// requests are deserialized as `Aggregations`, and results serialized from
// `AggregationResults`.

#[rustler::nif(schedule = "DirtyCpu")]
pub fn run_aggregations<'a>(
    env: Env<'a>,
    searcher_res: ResourceArc<SearcherResource>,
    query_res: ResourceArc<QueryResource>,
    aggregations_json: String,
) -> NifResult<Term<'a>> {
    let searcher = &searcher_res.searcher;
    let collector = aggregation_collector(searcher, &aggregations_json)?;

    let results = searcher
        .search(&query_res.query, &collector)
        .map_err(|e| error(format!("Error executing aggregations: {}", e)))?;

    results_term(env, &results)
}

/// The `search_limit` best hits, and the aggregations, of one search, as
/// `%{"hits" => %{"total" => %{"value" => n, "relation" => "eq"}, "hits" =>
/// hits}, "aggregations" => aggregations}`.
#[rustler::nif(schedule = "DirtyCpu")]
pub fn run_search_with_aggregations<'a>(
    env: Env<'a>,
    searcher_res: ResourceArc<SearcherResource>,
    query_res: ResourceArc<QueryResource>,
    aggregations_json: String,
    search_limit: usize,
) -> NifResult<Term<'a>> {
    let searcher = &searcher_res.searcher;

    let mut multi_collector = MultiCollector::new();
    let aggregations_handle =
        multi_collector.add_collector(aggregation_collector(searcher, &aggregations_json)?);
    let top_docs_handle =
        multi_collector.add_collector(TopDocs::with_limit(search_limit).order_by_score());

    let mut fruits = searcher
        .search(&query_res.query, &multi_collector)
        .map_err(|e| error(format!("Error executing search with aggregations: {}", e)))?;
    let aggregations = aggregations_handle.extract(&mut fruits);
    let top_docs = top_docs_handle.extract(&mut fruits);

    let total = rustler::types::map::map_new(env)
        .map_put("value".encode(env), top_docs.len().encode(env))?
        .map_put("relation".encode(env), "eq".encode(env))?;
    let hits = rustler::types::map::map_new(env)
        .map_put("total".encode(env), total)?
        .map_put(
            "hits".encode(env),
            encode_hits(env, searcher, top_docs, true)?,
        )?;

    rustler::types::map::map_new(env)
        .map_put("hits".encode(env), hits)?
        .map_put(
            "aggregations".encode(env),
            results_term(env, &aggregations)?,
        )
}

fn aggregation_collector(
    searcher: &Searcher,
    aggregations_json: &str,
) -> NifResult<AggregationCollector> {
    let aggregations: Aggregations = serde_json::from_str(aggregations_json)
        .map_err(|e| error(format!("Error parsing aggregations: {}", e)))?;

    // Tantivy gives empty results on a field missing from the schema: it is an
    // error here (a JSON field's path is a field too)
    let mut field_names: Vec<String> = get_fast_field_names(&aggregations).into_iter().collect();
    field_names.sort();
    for field_name in field_names {
        if searcher.schema().find_field(&field_name).is_none() {
            return Err(error(format!(
                "Error building aggregations: Field '{}' not found in schema",
                field_name
            )));
        }
    }

    Ok(AggregationCollector::from_aggs(
        aggregations,
        agg_context_params(searcher),
    ))
}

fn results_term<'a>(env: Env<'a>, results: &AggregationResults) -> NifResult<Term<'a>> {
    let json = serde_json::to_value(results)
        .map_err(|e| error(format!("Error serializing aggregation results: {}", e)))?;
    Ok(json_to_term(env, &json))
}

/// The aggregations' limits, and the tokenizers of the searched index, used by
/// the aggregations parsing a query string.
fn agg_context_params(searcher: &Searcher) -> AggContextParams {
    let limits = AggregationLimitsGuard::new(
        Some(500_000_000), // 500MB default memory limit
        Some(65535),       // Default bucket limit
    );
    AggContextParams::new(limits, searcher.index().tokenizers().clone())
}
