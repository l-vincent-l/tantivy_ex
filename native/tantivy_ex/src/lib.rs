// Refactored lib.rs - Main entry point for the TantivyEx native module

// Module declarations
pub mod modules {
    pub mod aggregation;
    pub mod document;
    pub mod facet;
    pub mod index;
    pub mod query;
    pub mod resources;
    pub mod schema;
    pub mod search;
    pub mod terms;
    pub mod tokenizer;
}

// Import all public functions from modules
// Note: These imports appear "unused" to the compiler because they're used
// via the #[rustler::nif] macro system, not direct Rust function calls
#[allow(unused_imports)]
use modules::aggregation::*;
#[allow(unused_imports)]
use modules::document::*;
#[allow(unused_imports)]
use modules::facet::*;
#[allow(unused_imports)]
use modules::index::*;
#[allow(unused_imports)]
use modules::query::*;
#[allow(unused_imports)]
use modules::schema::*;
#[allow(unused_imports)]
use modules::search::*;
#[allow(unused_imports)]
use modules::tokenizer::*;

rustler::atoms! {
    ok,
    error,
    nil,
}

// Register all NIFs with rustler
rustler::init!("Elixir.TantivyEx.Native");
