use tantivy::schema::{Facet, Field, FieldType, Schema};
use tantivy::tokenizer::TokenStream;
use tantivy::Term;

use crate::modules::resources::convert_ip_to_ipv6;
use crate::modules::tokenizer::global_tokenizer_manager;

/// The terms of `value` in the field, as they are indexed, for a term query or
/// a deletion: a text value is analyzed with the field's tokenizer (giving
/// zero, one or several terms), another value is parsed for the field's type.
pub fn value_terms(
    schema: &Schema,
    field_name: &str,
    value: &str,
) -> Result<(Field, Vec<Term>), String> {
    let field = schema
        .get_field(field_name)
        .map_err(|_| format!("Field '{}' not found in schema", field_name))?;
    let invalid = |type_name: &str| {
        format!(
            "Invalid {} value for field '{}': {:?}",
            type_name, field_name, value
        )
    };

    let terms = match schema.get_field_entry(field).field_type() {
        FieldType::Str(options) => {
            let indexing = options
                .get_indexing_options()
                .ok_or_else(|| format!("Field '{}' is not indexed", field_name))?;
            text_terms(field, field_name, indexing.tokenizer(), value)?
        }
        FieldType::U64(_) => vec![Term::from_field_u64(
            field,
            value.parse().map_err(|_| invalid("u64"))?,
        )],
        FieldType::I64(_) => vec![Term::from_field_i64(
            field,
            value.parse().map_err(|_| invalid("i64"))?,
        )],
        FieldType::F64(_) => vec![Term::from_field_f64(
            field,
            value.parse().map_err(|_| invalid("f64"))?,
        )],
        FieldType::Bool(_) => vec![Term::from_field_bool(
            field,
            parse_bool(value).ok_or_else(|| invalid("boolean"))?,
        )],
        FieldType::Date(_) => vec![Term::from_field_date(
            field,
            parse_date(value).ok_or_else(|| invalid("date"))?,
        )],
        FieldType::Facet(_) => vec![Term::from_facet(
            field,
            &Facet::from_text(value).map_err(|_| invalid("facet"))?,
        )],
        FieldType::IpAddr(_) => vec![Term::from_field_ip_addr(
            field,
            convert_ip_to_ipv6(value.parse().map_err(|_| invalid("IP address"))?),
        )],
        field_type => {
            return Err(format!(
                "Unsupported field type for a term on field '{}': {:?}",
                field_name,
                field_type.value_type()
            ))
        }
    };

    Ok((field, terms))
}

/// The terms of `text` once analyzed by the tokenizer named `tokenizer`.
pub fn text_terms(
    field: Field,
    field_name: &str,
    tokenizer: &str,
    text: &str,
) -> Result<Vec<Term>, String> {
    let mut analyzer = global_tokenizer_manager().get(tokenizer).ok_or_else(|| {
        format!(
            "Tokenizer '{}' of field '{}' is not registered",
            tokenizer, field_name
        )
    })?;

    let mut terms = Vec::new();
    let mut token_stream = analyzer.token_stream(text);
    while let Some(token) = token_stream.next() {
        terms.push(Term::from_field_text(field, &token.text));
    }
    Ok(terms)
}

/// The tokenizer of a text field, or an error if the field is not an indexed
/// text field.
pub fn text_tokenizer(schema: &Schema, field: Field, field_name: &str) -> Result<String, String> {
    match schema.get_field_entry(field).field_type() {
        FieldType::Str(options) => options
            .get_indexing_options()
            .map(|indexing| indexing.tokenizer().to_string())
            .ok_or_else(|| format!("Field '{}' is not indexed", field_name)),
        _ => Err(format!("Field '{}' is not a text field", field_name)),
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.to_lowercase().as_str() {
        "true" | "t" | "1" | "yes" | "y" => Some(true),
        "false" | "f" | "0" | "no" | "n" => Some(false),
        _ => None,
    }
}

/// A Unix timestamp in seconds, or an RFC 3339 date, as the documents take.
fn parse_date(value: &str) -> Option<tantivy::DateTime> {
    if let Ok(timestamp) = value.parse::<i64>() {
        return Some(tantivy::DateTime::from_timestamp_secs(timestamp));
    }
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| tantivy::DateTime::from_timestamp_secs(date.timestamp()))
}
