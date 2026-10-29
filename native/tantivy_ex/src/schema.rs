use rustler::{Encoder, Env, NifResult, ResourceArc, Term};
use tantivy::schema::{
    BytesOptions, DateOptions, FacetOptions, FieldType, IndexRecordOption, IpAddrOptions,
    JsonObjectOptions, NumericOptions, Schema, TextFieldIndexing, TextOptions,
};

use crate::resources::{SchemaResource, error};

/// The indexing of text fields: with term frequencies and positions, as
/// Tantivy's `TEXT`, for BM25 scoring and phrase queries.
fn text_indexing() -> TextFieldIndexing {
    TextFieldIndexing::default().set_index_option(IndexRecordOption::WithFreqsAndPositions)
}

#[rustler::nif]
pub fn schema_builder_new() -> rustler::ResourceArc<SchemaResource> {
    let schema = Schema::builder().build();
    rustler::ResourceArc::new(SchemaResource { schema })
}

#[rustler::nif]
pub fn schema_add_text_field(
    schema_res: ResourceArc<SchemaResource>,
    field_name: String,
    options: String,
) -> NifResult<ResourceArc<SchemaResource>> {
    let field_options = match options.as_str() {
        "TEXT_STORED" => TextOptions::default()
            .set_indexing_options(text_indexing())
            .set_stored(),
        "TEXT" => TextOptions::default().set_indexing_options(text_indexing()),
        "STORED" => TextOptions::default().set_stored(),
        "FAST" => TextOptions::default()
            .set_indexing_options(text_indexing())
            .set_fast(None),
        "FAST_STORED" => TextOptions::default()
            .set_indexing_options(text_indexing())
            .set_stored()
            .set_fast(None),
        other => {
            return Err(error(format!(
                "Unknown option {:?} for field {:?}",
                other, field_name
            )));
        }
    };

    with_field(&schema_res.schema, &field_name, |builder| {
        builder.add_text_field(&field_name, field_options);
    })
}

#[rustler::nif]
pub fn schema_add_text_field_with_tokenizer(
    schema_res: ResourceArc<SchemaResource>,
    field_name: String,
    options: String,
    tokenizer: String,
) -> NifResult<ResourceArc<SchemaResource>> {
    let field_options = match options.as_str() {
        "TEXT_STORED" => TextOptions::default()
            .set_indexing_options(text_indexing().set_tokenizer(&tokenizer))
            .set_stored(),
        "TEXT" => {
            TextOptions::default().set_indexing_options(text_indexing().set_tokenizer(&tokenizer))
        }
        "STORED" => {
            // For STORED-only fields, we don't set indexing options or tokenizer
            TextOptions::default().set_stored()
        }
        other => {
            return Err(error(format!(
                "Unknown option {:?} for field {:?}",
                other, field_name
            )));
        }
    };

    with_field(&schema_res.schema, &field_name, |builder| {
        builder.add_text_field(&field_name, field_options);
    })
}

#[rustler::nif]
pub fn schema_add_u64_field(
    schema_res: ResourceArc<SchemaResource>,
    field_name: String,
    options: String,
) -> NifResult<ResourceArc<SchemaResource>> {
    let field_options = match options.as_str() {
        "INDEXED_STORED" => NumericOptions::default().set_indexed().set_stored(),
        "INDEXED" => NumericOptions::default().set_indexed(),
        "STORED" => NumericOptions::default().set_stored(),
        "FAST" => NumericOptions::default().set_fast(),
        "FAST_STORED" => NumericOptions::default().set_fast().set_stored(),
        other => {
            return Err(error(format!(
                "Unknown option {:?} for field {:?}",
                other, field_name
            )));
        }
    };

    with_field(&schema_res.schema, &field_name, |builder| {
        builder.add_u64_field(&field_name, field_options);
    })
}

#[rustler::nif]
pub fn schema_add_i64_field(
    schema_res: ResourceArc<SchemaResource>,
    field_name: String,
    options: String,
) -> NifResult<ResourceArc<SchemaResource>> {
    let field_options = match options.as_str() {
        "INDEXED_STORED" => NumericOptions::default().set_indexed().set_stored(),
        "INDEXED" => NumericOptions::default().set_indexed(),
        "STORED" => NumericOptions::default().set_stored(),
        "FAST" => NumericOptions::default().set_fast(),
        "FAST_STORED" => NumericOptions::default().set_fast().set_stored(),
        other => {
            return Err(error(format!(
                "Unknown option {:?} for field {:?}",
                other, field_name
            )));
        }
    };

    with_field(&schema_res.schema, &field_name, |builder| {
        builder.add_i64_field(&field_name, field_options);
    })
}

#[rustler::nif]
pub fn schema_add_f64_field(
    schema_res: ResourceArc<SchemaResource>,
    field_name: String,
    options: String,
) -> NifResult<ResourceArc<SchemaResource>> {
    let field_options = match options.as_str() {
        "INDEXED_STORED" => NumericOptions::default().set_indexed().set_stored(),
        "INDEXED" => NumericOptions::default().set_indexed(),
        "STORED" => NumericOptions::default().set_stored(),
        "FAST" => NumericOptions::default().set_fast(),
        "FAST_STORED" => NumericOptions::default().set_fast().set_stored(),
        other => {
            return Err(error(format!(
                "Unknown option {:?} for field {:?}",
                other, field_name
            )));
        }
    };

    with_field(&schema_res.schema, &field_name, |builder| {
        builder.add_f64_field(&field_name, field_options);
    })
}

#[rustler::nif]
pub fn schema_add_bool_field(
    schema_res: ResourceArc<SchemaResource>,
    field_name: String,
    options: String,
) -> NifResult<ResourceArc<SchemaResource>> {
    let field_options = match options.as_str() {
        "INDEXED_STORED" => NumericOptions::default().set_indexed().set_stored(),
        "INDEXED" => NumericOptions::default().set_indexed(),
        "STORED" => NumericOptions::default().set_stored(),
        "FAST" => NumericOptions::default().set_fast(),
        "FAST_STORED" => NumericOptions::default().set_fast().set_stored(),
        other => {
            return Err(error(format!(
                "Unknown option {:?} for field {:?}",
                other, field_name
            )));
        }
    };

    with_field(&schema_res.schema, &field_name, |builder| {
        builder.add_bool_field(&field_name, field_options);
    })
}

#[rustler::nif]
pub fn schema_add_date_field(
    schema_res: ResourceArc<SchemaResource>,
    field_name: String,
    options: String,
) -> NifResult<ResourceArc<SchemaResource>> {
    let field_options = match options.as_str() {
        "INDEXED_STORED" => DateOptions::default().set_indexed().set_stored(),
        "INDEXED" => DateOptions::default().set_indexed(),
        "STORED" => DateOptions::default().set_stored(),
        "FAST" => DateOptions::default().set_fast(),
        "FAST_STORED" => DateOptions::default().set_fast().set_stored(),
        other => {
            return Err(error(format!(
                "Unknown option {:?} for field {:?}",
                other, field_name
            )));
        }
    };

    with_field(&schema_res.schema, &field_name, |builder| {
        builder.add_date_field(&field_name, field_options);
    })
}

#[rustler::nif]
pub fn schema_add_facet_field(
    schema_res: ResourceArc<SchemaResource>,
    field_name: String,
    _options: String, // Facet fields don't use the same options pattern
) -> NifResult<ResourceArc<SchemaResource>> {
    // Facet fields are always indexed; they are also stored, to be in the results
    let field_options = FacetOptions::default().set_stored();

    with_field(&schema_res.schema, &field_name, |builder| {
        builder.add_facet_field(&field_name, field_options);
    })
}

#[rustler::nif]
pub fn schema_add_bytes_field(
    schema_res: ResourceArc<SchemaResource>,
    field_name: String,
    options: String,
) -> NifResult<ResourceArc<SchemaResource>> {
    let field_options = match options.as_str() {
        "INDEXED_STORED" => BytesOptions::default().set_indexed().set_stored(),
        "INDEXED" => BytesOptions::default().set_indexed(),
        "STORED" => BytesOptions::default().set_stored(),
        "FAST" => BytesOptions::default().set_fast(),
        "FAST_STORED" => BytesOptions::default().set_fast().set_stored(),
        other => {
            return Err(error(format!(
                "Unknown option {:?} for field {:?}",
                other, field_name
            )));
        }
    };

    with_field(&schema_res.schema, &field_name, |builder| {
        builder.add_bytes_field(&field_name, field_options);
    })
}

#[rustler::nif]
pub fn schema_add_json_field(
    schema_res: ResourceArc<SchemaResource>,
    field_name: String,
    options: String,
) -> NifResult<ResourceArc<SchemaResource>> {
    let field_options = match options.as_str() {
        "STORED" => JsonObjectOptions::default().set_stored(),
        "TEXT" => JsonObjectOptions::default().set_indexing_options(text_indexing()),
        "TEXT_STORED" => JsonObjectOptions::default()
            .set_indexing_options(text_indexing())
            .set_stored(),
        other => {
            return Err(error(format!(
                "Unknown option {:?} for field {:?}",
                other, field_name
            )));
        }
    };

    with_field(&schema_res.schema, &field_name, |builder| {
        builder.add_json_field(&field_name, field_options);
    })
}

#[rustler::nif]
pub fn schema_add_ip_addr_field(
    schema_res: ResourceArc<SchemaResource>,
    field_name: String,
    options: String,
) -> NifResult<ResourceArc<SchemaResource>> {
    let field_options = match options.as_str() {
        "INDEXED_STORED" => IpAddrOptions::default().set_indexed().set_stored(),
        "INDEXED" => IpAddrOptions::default().set_indexed(),
        "STORED" => IpAddrOptions::default().set_stored(),
        "FAST" => IpAddrOptions::default().set_fast(),
        "FAST_STORED" => IpAddrOptions::default().set_fast().set_stored(),
        other => {
            return Err(error(format!(
                "Unknown option {:?} for field {:?}",
                other, field_name
            )));
        }
    };

    with_field(&schema_res.schema, &field_name, |builder| {
        builder.add_ip_addr_field(&field_name, field_options);
    })
}

#[rustler::nif]
pub fn schema_get_field_names<'a>(
    env: Env<'a>,
    schema_res: ResourceArc<SchemaResource>,
) -> NifResult<Term<'a>> {
    let field_names: Vec<String> = schema_res
        .schema
        .fields()
        .map(|(field, _)| schema_res.schema.get_field_name(field).to_string())
        .collect();

    Ok(field_names.encode(env))
}

#[rustler::nif]
pub fn schema_get_field_type<'a>(
    env: Env<'a>,
    schema_res: ResourceArc<SchemaResource>,
    field_name: String,
) -> NifResult<Term<'a>> {
    match schema_res.schema.get_field(&field_name) {
        Ok(field) => {
            let field_entry = schema_res.schema.get_field_entry(field);
            let field_type_str = match field_entry.field_type() {
                FieldType::Str(_) => "text",
                FieldType::U64(_) => "u64",
                FieldType::I64(_) => "i64",
                FieldType::F64(_) => "f64",
                FieldType::Bool(_) => "bool",
                FieldType::Date(_) => "date",
                FieldType::Facet(_) => "facet",
                FieldType::Bytes(_) => "bytes",
                FieldType::JsonObject(_) => "json",
                FieldType::IpAddr(_) => "ip_addr",
            };
            Ok(field_type_str.encode(env))
        }
        Err(_) => Err(error(format!("Field '{}' not found in schema", field_name))),
    }
}

#[rustler::nif]
pub fn schema_validate<'a>(
    env: Env<'a>,
    schema_res: ResourceArc<SchemaResource>,
) -> NifResult<Term<'a>> {
    // Basic validation - check if schema has at least one field
    let field_count = schema_res.schema.fields().count();
    if field_count == 0 {
        return Err(error("Schema must have at least one field".to_string()));
    }

    let message = format!("Schema is valid with {} fields", field_count);
    Ok(message.encode(env))
}

/// The schema with one more field, added by `add`. Tantivy's schema builder
/// panics on a field name already in the schema: it is an error here.
fn with_field(
    schema: &Schema,
    field_name: &str,
    add: impl FnOnce(&mut tantivy::schema::SchemaBuilder),
) -> NifResult<ResourceArc<SchemaResource>> {
    if schema.get_field(field_name).is_ok() {
        return Err(error(format!(
            "Field {:?} already exists in the schema",
            field_name
        )));
    }

    let mut builder = Schema::builder();
    copy_existing_fields_to_builder(schema, &mut builder);
    add(&mut builder);
    Ok(ResourceArc::new(SchemaResource {
        schema: builder.build(),
    }))
}

/// Adds the fields of `schema` to `schema_builder`.
fn copy_existing_fields_to_builder(
    schema: &Schema,
    schema_builder: &mut tantivy::schema::SchemaBuilder,
) {
    for (field, field_entry) in schema.fields() {
        let field_name_existing = schema.get_field_name(field);
        match field_entry.field_type() {
            FieldType::Str(text_options) => {
                schema_builder.add_text_field(field_name_existing, text_options.clone());
            }
            FieldType::U64(int_options) => {
                schema_builder.add_u64_field(field_name_existing, int_options.clone());
            }
            FieldType::I64(int_options) => {
                schema_builder.add_i64_field(field_name_existing, int_options.clone());
            }
            FieldType::F64(float_options) => {
                schema_builder.add_f64_field(field_name_existing, float_options.clone());
            }
            FieldType::Bool(bool_options) => {
                schema_builder.add_bool_field(field_name_existing, bool_options.clone());
            }
            FieldType::Date(date_options) => {
                schema_builder.add_date_field(field_name_existing, date_options.clone());
            }
            FieldType::Facet(facet_options) => {
                schema_builder.add_facet_field(field_name_existing, facet_options.clone());
            }
            FieldType::Bytes(bytes_options) => {
                schema_builder.add_bytes_field(field_name_existing, bytes_options.clone());
            }
            FieldType::JsonObject(json_options) => {
                schema_builder.add_json_field(field_name_existing, json_options.clone());
            }
            FieldType::IpAddr(ip_options) => {
                schema_builder.add_ip_addr_field(field_name_existing, ip_options.clone());
            }
        }
    }
}
