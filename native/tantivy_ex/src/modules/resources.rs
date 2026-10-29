use rustler::{Encoder, Env, Term};
use std::collections::BTreeMap;
use std::panic::RefUnwindSafe;
use std::sync::{Arc, Mutex, MutexGuard};
use tantivy::schema::{OwnedValue, Schema};
use tantivy::tokenizer::TokenizerManager;
use tantivy::{query::QueryParser, Index, IndexWriter};

// Resource types for managing state
pub struct IndexResource {
    pub index: Arc<Index>,
}

#[rustler::resource_impl]
impl rustler::Resource for IndexResource {}

// Rustler catches the panics of NIFs, which requires their arguments to be
// `RefUnwindSafe`. A panic leaves no broken invariant behind a `&IndexResource`:
// the index is not mutated once in the resource, and the tokenizer manager it
// shares is behind a lock.
impl RefUnwindSafe for IndexResource {}

pub struct SchemaResource {
    pub schema: Schema,
}

#[rustler::resource_impl]
impl rustler::Resource for SchemaResource {}

pub struct IndexWriterResource {
    pub writer: Arc<Mutex<IndexWriter>>,
}

#[rustler::resource_impl]
impl rustler::Resource for IndexWriterResource {}

impl IndexWriterResource {
    /// Locks the writer. A panic while it was locked leaves it in an unknown
    /// state: it is then an error to use it, instead of a panic in every call.
    pub fn lock(&self) -> rustler::NifResult<MutexGuard<'_, IndexWriter>> {
        self.writer.lock().map_err(|_| {
            rustler::Error::Term(Box::new(
                "The index writer is unusable: an operation panicked while using it".to_string(),
            ))
        })
    }
}

pub struct SearcherResource {
    pub searcher: Arc<tantivy::Searcher>,
}

#[rustler::resource_impl]
impl rustler::Resource for SearcherResource {}

pub struct QueryResource {
    pub query: Box<dyn tantivy::query::Query>,
}

#[rustler::resource_impl]
impl rustler::Resource for QueryResource {}

// Rustler catches the panics of NIFs, which requires their arguments to be
// `RefUnwindSafe`. A panic leaves no broken invariant behind a `&QueryResource`:
// queries are not mutated once built.
impl RefUnwindSafe for QueryResource {}

pub struct QueryParserResource {
    pub parser: QueryParser,
}

#[rustler::resource_impl]
impl rustler::Resource for QueryParserResource {}

pub struct TokenizerManagerResource {
    pub manager: TokenizerManager,
}

#[rustler::resource_impl]
impl rustler::Resource for TokenizerManagerResource {}

// Helper function to convert serde_json::Value to BTreeMap<String, OwnedValue>
pub fn convert_json_value_to_btreemap(value: serde_json::Value) -> BTreeMap<String, OwnedValue> {
    // Tantivy's conversion, recursive: nested objects and arrays are kept
    match value {
        serde_json::Value::Object(obj) => obj
            .into_iter()
            .map(|(key, val)| (key, OwnedValue::from(val)))
            .collect(),
        _ => BTreeMap::new(),
    }
}

// Helper function to convert IpAddr to Ipv6Addr
pub fn convert_ip_to_ipv6(ip: std::net::IpAddr) -> std::net::Ipv6Addr {
    match ip {
        std::net::IpAddr::V4(ipv4) => ipv4.to_ipv6_mapped(),
        std::net::IpAddr::V6(ipv6) => ipv6,
    }
}

pub mod atoms {
    rustler::atoms! {
        ok,
        error,
        nil,
    }
}

/// Encodes a JSON value as Jason would decode it: maps with string keys,
/// lists, integers, floats, strings, booleans and nil.
pub fn json_to_term<'a>(env: Env<'a>, value: &serde_json::Value) -> Term<'a> {
    match value {
        serde_json::Value::Null => rustler::types::atom::nil().encode(env),
        serde_json::Value::Bool(b) => b.encode(env),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.encode(env)
            } else if let Some(u) = n.as_u64() {
                u.encode(env)
            } else {
                n.as_f64().unwrap_or(0.0).encode(env)
            }
        }
        serde_json::Value::String(s) => s.encode(env),
        serde_json::Value::Array(values) => values
            .iter()
            .map(|v| json_to_term(env, v))
            .collect::<Vec<_>>()
            .encode(env),
        serde_json::Value::Object(map) => {
            let mut term = rustler::types::map::map_new(env);
            for (key, v) in map {
                if let Ok(updated) = term.map_put(key.encode(env), json_to_term(env, v)) {
                    term = updated;
                }
            }
            term
        }
    }
}
