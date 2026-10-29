//! The NIFs of `TantivyEx.Native`. Rustler registers every `#[rustler::nif]`
//! function and `#[rustler::resource_impl]` resource of the crate.

mod aggregation;
mod document;
mod facet;
mod index;
mod query;
mod resources;
mod schema;
mod search;
mod terms;
mod tokenizer;

rustler::init!("Elixir.TantivyEx.Native");
