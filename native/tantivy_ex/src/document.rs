use base64::{Engine as _, engine::general_purpose};
use rustler::{NifResult, ResourceArc};
use std::collections::HashMap;
use tantivy::TantivyDocument;
use tantivy::query::PhraseQuery;
use tantivy::schema::{Field, FieldType, Schema};

use crate::resources::{
    IndexWriterResource, QueryResource, SchemaResource, atoms, convert_ip_to_ipv6,
    convert_json_value_to_btreemap, error,
};
use crate::terms::{parse_rfc3339, value_terms};

// The NIFs locking the writer run on dirty IO schedulers: they wait for the
// lock while another call holds it (a commit), and for Tantivy's indexing
// queue when it is full. Waiting on normal schedulers would block the VM, and
// on dirty CPU schedulers would take them from the searches.
#[rustler::nif(schedule = "DirtyIo")]
pub fn writer_add_document(
    writer_res: ResourceArc<IndexWriterResource>,
    document: rustler::Term,
) -> NifResult<rustler::Atom> {
    let writer = writer_res.lock()?;
    let tantivy_doc = build_document(&writer.index().schema(), document).map_err(error)?;

    match writer.add_document(tantivy_doc) {
        Ok(_) => Ok(atoms::ok()),
        Err(e) => Err(error(format!("Failed to add document: {}", e))),
    }
}

#[rustler::nif(schedule = "DirtyIo")]
pub fn writer_commit(writer_res: ResourceArc<IndexWriterResource>) -> NifResult<rustler::Atom> {
    let mut writer = writer_res.lock()?;
    match writer.commit() {
        Ok(_) => Ok(atoms::ok()),
        Err(e) => Err(error(format!("Failed to commit: {}", e))),
    }
}

#[rustler::nif(schedule = "DirtyIo")]
pub fn writer_delete_documents(
    writer_res: ResourceArc<IndexWriterResource>,
    query_res: ResourceArc<QueryResource>,
) -> NifResult<rustler::Atom> {
    let writer = writer_res.lock()?;
    let query = query_res.query.box_clone();

    match writer.delete_query(query) {
        Ok(_) => Ok(atoms::ok()),
        Err(e) => Err(error(format!("Failed to delete documents by query: {}", e))),
    }
}

#[rustler::nif(schedule = "DirtyIo")]
pub fn writer_delete_all_documents(
    writer_res: ResourceArc<IndexWriterResource>,
) -> NifResult<rustler::Atom> {
    let writer = writer_res.lock()?;

    match writer.delete_all_documents() {
        Ok(_) => Ok(atoms::ok()),
        Err(e) => Err(error(format!("Failed to delete all documents: {}", e))),
    }
}

#[rustler::nif(schedule = "DirtyIo")]
pub fn writer_rollback(writer_res: ResourceArc<IndexWriterResource>) -> NifResult<rustler::Atom> {
    let mut writer = writer_res.lock()?;

    match writer.rollback() {
        Ok(_) => Ok(atoms::ok()),
        Err(e) => Err(error(format!("Failed to rollback: {}", e))),
    }
}

/// Deletes the documents whose field has the value. The value is analyzed as a
/// term query's is: with the field's tokenizer for a text field, parsed for
/// the field's type otherwise. A text value of several words deletes the
/// documents with this phrase.
#[rustler::nif(schedule = "DirtyIo")]
pub fn writer_delete_term(
    writer_res: ResourceArc<IndexWriterResource>,
    term_field: String,
    term_value: rustler::Term,
) -> NifResult<rustler::Atom> {
    let value = if let Ok(string) = term_value.decode::<String>() {
        string
    } else if let Ok(boolean) = term_value.decode::<bool>() {
        boolean.to_string()
    } else if let Ok(integer) = term_value.decode::<i64>() {
        integer.to_string()
    } else if let Ok(integer) = term_value.decode::<u64>() {
        integer.to_string()
    } else if let Ok(float) = term_value.decode::<f64>() {
        float.to_string()
    } else {
        return Err(error(format!(
            "Invalid value for field '{}': expected a string, a number or a boolean",
            term_field
        )));
    };

    let writer = writer_res.lock()?;
    let schema = writer.index().schema();
    let (_field, mut terms) = value_terms(&schema, &term_field, &value).map_err(error)?;

    match terms.len() {
        0 => Err(error(format!(
            "Value {:?} has no terms in field '{}'",
            value, term_field
        ))),
        1 => {
            writer.delete_term(terms.remove(0));
            Ok(atoms::ok())
        }
        _ => {
            let query: Box<dyn tantivy::query::Query> = Box::new(PhraseQuery::new(terms));
            writer
                .delete_query(query)
                .map_err(|e| error(format!("Failed to delete by query: {}", e)))?;
            Ok(atoms::ok())
        }
    }
}

#[rustler::nif(schedule = "DirtyIo")]
pub fn writer_add_document_with_schema(
    writer_res: ResourceArc<IndexWriterResource>,
    document: rustler::Term,
    schema_res: ResourceArc<SchemaResource>,
) -> NifResult<rustler::Atom> {
    let tantivy_doc = build_document(&schema_res.schema, document).map_err(error)?;
    let writer = writer_res.lock()?;

    match writer.add_document(tantivy_doc) {
        Ok(_) => Ok(atoms::ok()),
        Err(e) => Err(error(format!("Failed to add document: {}", e))),
    }
}

/// Builds a Tantivy document from an Elixir map of field names to values.
///
/// An unknown field, or a value that does not fit its field's type, is an
/// error. A list adds each of its values to the field, except for bytes and
/// JSON fields, whose value can itself be a list.
pub fn build_document(schema: &Schema, document: rustler::Term) -> Result<TantivyDocument, String> {
    let doc_map: HashMap<String, rustler::Term> = document
        .decode()
        .map_err(|_| "Failed to decode document map: Expected a map".to_string())?;
    let mut tantivy_doc = TantivyDocument::default();

    for (field_name, value) in doc_map {
        let field = schema
            .get_field(&field_name)
            .map_err(|_| format!("Unknown field: '{}'", field_name))?;
        let field_type = schema.get_field_entry(field).field_type();

        let values = match field_type {
            FieldType::Bytes(_) | FieldType::JsonObject(_) => vec![value],
            _ => value
                .decode::<Vec<rustler::Term>>()
                .unwrap_or_else(|_| vec![value]),
        };

        for value in values {
            add_field_to_document(&mut tantivy_doc, field, field_type, value)
                .map_err(|e| format!("Field '{}': {}", field_name, e))?;
        }
    }

    Ok(tantivy_doc)
}

pub fn add_field_to_document(
    doc: &mut TantivyDocument,
    field: Field,
    field_type: &FieldType,
    value: rustler::Term,
) -> Result<(), String> {
    match field_type {
        FieldType::Str(_) => {
            let string_val: String = value.decode().map_err(|_| "Expected string value")?;
            doc.add_text(field, &string_val);
            Ok(())
        }
        FieldType::U64(_) => {
            if let Ok(int_val) = value.decode::<u64>() {
                doc.add_u64(field, int_val);
                Ok(())
            } else if let Ok(int_val) = value.decode::<i64>() {
                if int_val >= 0 {
                    doc.add_u64(field, int_val as u64);
                    Ok(())
                } else {
                    Err("Negative value not allowed for u64 field".to_string())
                }
            } else {
                Err("Expected numeric value".to_string())
            }
        }
        FieldType::I64(_) => {
            if let Ok(int_val) = value.decode::<i64>() {
                doc.add_i64(field, int_val);
                Ok(())
            } else if value.decode::<u64>().is_ok() {
                Err("Value too large for i64 field".to_string())
            } else {
                Err("Expected numeric value".to_string())
            }
        }
        FieldType::F64(_) => {
            if let Ok(float_val) = value.decode::<f64>() {
                doc.add_f64(field, float_val);
                Ok(())
            } else if let Ok(int_val) = value.decode::<i64>() {
                doc.add_f64(field, int_val as f64);
                Ok(())
            } else if let Ok(int_val) = value.decode::<u64>() {
                doc.add_f64(field, int_val as f64);
                Ok(())
            } else {
                Err("Expected numeric value".to_string())
            }
        }
        FieldType::Bool(_) => {
            let bool_val: bool = value.decode().map_err(|_| "Expected boolean value")?;
            doc.add_bool(field, bool_val);
            Ok(())
        }
        FieldType::Date(_) => {
            if let Ok(timestamp) = value.decode::<i64>() {
                let datetime = tantivy::DateTime::from_timestamp_secs(timestamp);
                doc.add_date(field, datetime);
                Ok(())
            } else if let Ok(string_val) = value.decode::<String>() {
                let datetime = parse_rfc3339(&string_val)
                    .ok_or_else(|| "Invalid date format, expected ISO 8601".to_string())?;
                doc.add_date(field, datetime);
                Ok(())
            } else {
                Err("Expected timestamp (integer) or ISO 8601 date string".to_string())
            }
        }
        FieldType::Facet(_) => {
            let string_val: String = value
                .decode()
                .map_err(|_| "Expected string value or array of strings for facet")?;
            let facet = tantivy::schema::Facet::from_text(&string_val)
                .map_err(|_| "Invalid facet format".to_string())?;
            doc.add_facet(field, facet);
            Ok(())
        }
        FieldType::Bytes(_) => {
            if let Ok(bytes_val) = value.decode::<Vec<u8>>() {
                doc.add_bytes(field, &bytes_val);
                Ok(())
            } else if let Ok(string_val) = value.decode::<String>() {
                let bytes = general_purpose::STANDARD
                    .decode(&string_val)
                    .map_err(|_| "Invalid base64 encoding".to_string())?;
                doc.add_bytes(field, &bytes);
                Ok(())
            } else {
                Err("Expected byte array or base64 string".to_string())
            }
        }
        FieldType::JsonObject(_) => {
            // A map, or a string holding a JSON object
            let json_value = match value.decode::<String>() {
                Ok(json_str) => {
                    serde_json::from_str(&json_str).map_err(|e| format!("Invalid JSON: {}", e))?
                }
                Err(_) => convert_term_to_json_value(value),
            };
            if !json_value.is_object() {
                return Err("Expected a map or a JSON object string".to_string());
            }
            doc.add_object(field, convert_json_value_to_btreemap(json_value));
            Ok(())
        }
        FieldType::IpAddr(_) => {
            let string_val: String = value
                .decode()
                .map_err(|_| "Expected string value for IP address")?;
            let ip = string_val
                .parse::<std::net::IpAddr>()
                .map_err(|_| "Invalid IP address format".to_string())?;
            let ipv6 = convert_ip_to_ipv6(ip);
            doc.add_ip_addr(field, ipv6);
            Ok(())
        }
    }
}
pub fn convert_term_to_json_value(term: rustler::Term) -> serde_json::Value {
    if let Ok(s) = term.decode::<String>() {
        serde_json::Value::String(s)
    } else if let Ok(i) = term.decode::<i64>() {
        serde_json::Value::Number(serde_json::Number::from(i))
    } else if let Ok(u) = term.decode::<u64>() {
        serde_json::Value::Number(serde_json::Number::from(u))
    } else if let Ok(f) = term.decode::<f64>() {
        serde_json::Value::Number(
            serde_json::Number::from_f64(f).unwrap_or(serde_json::Number::from(0)),
        )
    } else if let Ok(b) = term.decode::<bool>() {
        serde_json::Value::Bool(b)
    } else if let Ok(map) = term.decode::<HashMap<String, rustler::Term>>() {
        let mut obj = serde_json::Map::new();
        for (key, value) in map {
            obj.insert(key, convert_term_to_json_value(value));
        }
        serde_json::Value::Object(obj)
    } else if let Ok(vec) = term.decode::<Vec<rustler::Term>>() {
        let array: Vec<serde_json::Value> =
            vec.into_iter().map(convert_term_to_json_value).collect();
        serde_json::Value::Array(array)
    } else {
        serde_json::Value::Null
    }
}
