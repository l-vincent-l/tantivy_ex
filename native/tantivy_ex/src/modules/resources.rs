use std::collections::BTreeMap;
use std::panic::RefUnwindSafe;
use std::sync::{Arc, Mutex};
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
    let mut map = BTreeMap::new();

    if let serde_json::Value::Object(obj) = value {
        for (key, val) in obj {
            let owned_value = match val {
                serde_json::Value::String(s) => OwnedValue::Str(s),
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        OwnedValue::I64(i)
                    } else if let Some(u) = n.as_u64() {
                        OwnedValue::U64(u)
                    } else if let Some(f) = n.as_f64() {
                        OwnedValue::F64(f)
                    } else {
                        OwnedValue::Str(n.to_string())
                    }
                }
                serde_json::Value::Bool(b) => OwnedValue::Bool(b),
                serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                    OwnedValue::Str(val.to_string())
                }
                serde_json::Value::Null => OwnedValue::Str("null".to_string()),
            };
            map.insert(key, owned_value);
        }
    }
    map
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
