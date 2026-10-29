use tantivy::Term;
use tantivy::schema::{Facet, Field, FieldType, Schema};
use tantivy::tokenizer::TokenStream;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::resources::convert_ip_to_ipv6;
use crate::tokenizer::global_tokenizer_manager;

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
            ));
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
    parse_rfc3339(value)
}

/// An RFC 3339 date (`2024-01-15T10:30:00Z`), to the second.
pub fn parse_rfc3339(value: &str) -> Option<tantivy::DateTime> {
    OffsetDateTime::parse(value, &Rfc3339)
        .ok()
        .map(|date| tantivy::DateTime::from_timestamp_secs(date.unix_timestamp()))
}

#[cfg(test)]
mod tests {
    use super::*;

    use tantivy::schema::{IndexRecordOption, NumericOptions, TextFieldIndexing, TextOptions};

    fn schema() -> Schema {
        let text = |tokenizer| {
            TextOptions::default().set_indexing_options(
                TextFieldIndexing::default()
                    .set_tokenizer(tokenizer)
                    .set_index_option(IndexRecordOption::WithFreqsAndPositions),
            )
        };
        let mut builder = Schema::builder();
        builder.add_text_field("title", text("default"));
        builder.add_text_field("sku", text("raw"));
        builder.add_u64_field("id", NumericOptions::default().set_indexed());
        builder.build()
    }

    fn texts(terms: &[Term]) -> Vec<String> {
        terms
            .iter()
            .map(|term| term.value().as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn text_values_are_analyzed_with_the_field_tokenizer() {
        let (_, terms) = value_terms(&schema(), "title", "Hello World").unwrap();
        assert_eq!(texts(&terms), ["hello", "world"]);

        let (_, terms) = value_terms(&schema(), "sku", "AB-12").unwrap();
        assert_eq!(texts(&terms), ["AB-12"]);
    }

    #[test]
    fn other_values_are_parsed_for_the_field_type() {
        let (_, terms) = value_terms(&schema(), "id", "42").unwrap();
        assert_eq!(terms[0].value().as_u64(), Some(42));

        assert!(value_terms(&schema(), "id", "forty-two").is_err());
        assert!(value_terms(&schema(), "nope", "x").is_err());
    }

    #[test]
    fn parses_booleans() {
        assert_eq!(parse_bool("TRUE"), Some(true));
        assert_eq!(parse_bool("no"), Some(false));
        assert_eq!(parse_bool("maybe"), None);
    }

    #[test]
    fn parses_timestamps_and_rfc3339_dates() {
        let date = parse_date("2024-01-15T10:30:00Z").unwrap();
        assert_eq!(date.into_timestamp_secs(), 1_705_314_600);
        assert_eq!(parse_date("1705314600"), Some(date));
        assert_eq!(parse_date("yesterday"), None);
    }
}
