# TantivyEx

[![Elixir CI](https://github.com/l-vincent-l/tantivy_ex/actions/workflows/ci.yml/badge.svg)](https://github.com/l-vincent-l/tantivy_ex/actions)
[![License](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

![TantivyEx Logo](assets/logo.png)

**A comprehensive Elixir wrapper for the Tantivy full-text search engine.**

TantivyEx provides a complete, type-safe interface to Tantivy - Rust's fastest full-text search library. Build powerful search applications with support for all Tantivy field types, custom tokenizers, schema introspection, and advanced indexing features.

## Features

- **Field types**: text, numeric (`u64`, `i64`, `f64`), boolean, date, facet, bytes, JSON and IP address fields
- **Queries**: query strings with Tantivy's syntax, or term, phrase, range, boolean, fuzzy, regex, wildcard, phrase prefix, facet and more-like-this queries
- **Text analysis**: built-in and custom tokenizers, stemming and stop words for 18 languages, n-grams, ASCII folding
- **Snippets**: the fragment of a text matching a query, with the terms to highlight
- **Aggregations**: terms, histogram, date histogram and range buckets, with average, min, max, sum, stats and percentiles metrics
- **Storage**: indexes on disk or in memory

## Quick Start

```elixir
alias TantivyEx.{Schema, Index, IndexWriter, Searcher}

schema =
  Schema.new()
  |> Schema.add_u64_field("id", :indexed_stored)
  |> Schema.add_text_field("title", :text_stored)
  |> Schema.add_text_field("body", :text)

{:ok, index} = Index.create_in_ram(schema)

{:ok, writer} = IndexWriter.new(index)
:ok = IndexWriter.add_document(writer, %{"id" => 1, "title" => "Hello", "body" => "Full-text search in Elixir"})
:ok = IndexWriter.add_document(writer, %{"id" => 2, "title" => "Goodbye", "body" => "Nothing to see here"})
:ok = IndexWriter.commit(writer)

{:ok, searcher} = Searcher.new(index)
{:ok, [result]} = Searcher.search(searcher, "elixir", 10)
result["title"]
# => "Hello"
```

The [Quick Start Tutorial](docs/quick-start.md) goes further: persistent
indexes, the query syntax, snippets, updates and deletions.

## Installation

Add `tantivy_ex` to your list of dependencies in `mix.exs`:

<!-- skip-test -->
```elixir
def deps do
  [
    {:tantivy_ex, github: "l-vincent-l/tantivy_ex"}
  ]
end
```

The NIF is compiled from source, which requires:

- **Elixir** 1.15 or later
- **Rust** 1.91 or later

## Documentation

**[Browse All Guides →](docs/guides.md)**

- **[Installation & Setup](docs/installation-setup.md)**: installation and configuration
- **[Quick Start Tutorial](docs/quick-start.md)**: a first search application
- **[Core Concepts](docs/core-concepts.md)**: the building blocks, with a glossary
- **[Schema Management](docs/schema.md)**: field types, options and schema design
- **[Document Operations](docs/documents.md)**: adding, updating and deleting documents
- **[Indexing](docs/indexing.md)**: index creation, writing and maintenance
- **[Search Operations](docs/search.md)**: query syntax and ranking
- **[Search Results](docs/search_results.md)**: handling and formatting results
- **[Tokenizers](docs/tokenizers.md)**: text analysis, custom tokenizers and languages
- **[Aggregations](docs/aggregations.md)**: bucket and metric aggregations

## Field Types

TantivyEx supports all Tantivy field types with comprehensive options:

| Field Type | Elixir Function | Use Cases | Options |
|------------|----------------|-----------|---------|
| **Text** | `add_text_field/3` | Full-text search, titles, content | `:text`, `:text_stored`, `:stored`, `:fast`, `:fast_stored` |
| **U64** | `add_u64_field/3` | IDs, counts, timestamps | `:indexed`, `:indexed_stored`, `:fast`, `:stored`, `:fast_stored` |
| **I64** | `add_i64_field/3` | Signed integers, deltas | `:indexed`, `:indexed_stored`, `:fast`, `:stored`, `:fast_stored` |
| **F64** | `add_f64_field/3` | Prices, scores, coordinates | `:indexed`, `:indexed_stored`, `:fast`, `:stored`, `:fast_stored` |
| **Bool** | `add_bool_field/3` | Flags, binary states | `:indexed`, `:indexed_stored`, `:fast`, `:stored`, `:fast_stored` |
| **Date** | `add_date_field/3` | Timestamps, publication dates | `:indexed`, `:indexed_stored`, `:fast`, `:stored`, `:fast_stored` |
| **Facet** | `add_facet_field/2` | Categories, hierarchical data | Always indexed and stored |
| **Bytes** | `add_bytes_field/3` | Binary data, hashes, images | `:indexed`, `:indexed_stored`, `:fast`, `:stored`, `:fast_stored` |
| **JSON** | `add_json_field/3` | Structured objects, metadata | `:text`, `:text_stored`, `:stored` |
| **IP Address** | `add_ip_addr_field/3` | IPv4/IPv6 addresses | `:indexed`, `:indexed_stored`, `:fast`, `:stored`, `:fast_stored` |

## Custom Tokenizers

```elixir
# The built-in tokenizers: "default", "simple", "whitespace", "raw"...
TantivyEx.Tokenizer.register_default_tokenizers()

# A text analyzer: a base tokenizer, then filters
:ok =
  TantivyEx.Tokenizer.register_text_analyzer(
    "english_full",
    # base tokenizer
    "simple",
    # lowercase
    true,
    # stop words language
    "english",
    # stemming language
    "english",
    # remove tokens longer than 50 characters
    50
  )

TantivyEx.Tokenizer.tokenize_text("english_full", "The Running Dogs")
# => ["run", "dog"]

# Fields use a tokenizer by name
schema =
  TantivyEx.Schema.new()
  |> TantivyEx.Schema.add_text_field_with_tokenizer("content", :text, "english_full")
  |> TantivyEx.Schema.add_text_field_with_tokenizer("tags", :text, "whitespace")
  |> TantivyEx.Schema.add_text_field_with_tokenizer("sku", :text_stored, "raw")
```

The `"raw"` tokenizer keeps the whole value as a single token, for exact
matching (identifiers, codes). The [Tokenizers guide](docs/tokenizers.md)
covers regex and n-gram tokenizers, languages and ASCII folding.

## Performance Tips

1. **Index only what you search**, and store only what you display
2. **Use fast fields** (`:fast`, `:fast_stored`) for ranges, sorting and aggregations
3. **Commit in batches**: a commit makes the documents searchable, and has a cost
4. **Size the writer's memory budget** (`IndexWriter.new/2`, 50 MB by default) to your indexing volume
5. **Register tokenizers once**, at application startup

## Examples

### E-commerce Search

```elixir
# Product search schema
schema = TantivyEx.Schema.new()
|> TantivyEx.Schema.add_text_field("name", :text_stored)
|> TantivyEx.Schema.add_text_field("description", :text)
|> TantivyEx.Schema.add_f64_field("price", :fast_stored)
|> TantivyEx.Schema.add_facet_field("category")
|> TantivyEx.Schema.add_bool_field("in_stock", :fast)
|> TantivyEx.Schema.add_u64_field("product_id", :indexed_stored)
```

### Blog Search

```elixir
# Setup tokenizers for blog content
TantivyEx.Tokenizer.register_default_tokenizers()
TantivyEx.Tokenizer.register_language_analyzer("english")

# Blog post schema with optimized tokenizers
schema = TantivyEx.Schema.new()
|> TantivyEx.Schema.add_text_field_with_tokenizer("title", :text_stored, "english_text")
|> TantivyEx.Schema.add_text_field_with_tokenizer("content", :text, "english_text")
|> TantivyEx.Schema.add_text_field_with_tokenizer("tags", :text_stored, "whitespace")
|> TantivyEx.Schema.add_date_field("published_at", :fast_stored)
|> TantivyEx.Schema.add_u64_field("author_id", :fast)
```

### Log Analysis

```elixir
# Application log schema
schema = TantivyEx.Schema.new()
|> TantivyEx.Schema.add_text_field("message", :text_stored)
|> TantivyEx.Schema.add_text_field("level", :text)
|> TantivyEx.Schema.add_ip_addr_field("client_ip", :indexed_stored)
|> TantivyEx.Schema.add_date_field("timestamp", :fast_stored)
|> TantivyEx.Schema.add_json_field("metadata", :stored)
```

## Development Setup

```bash
git clone https://github.com/l-vincent-l/tantivy_ex.git
cd tantivy_ex
mix deps.get
mix test
```

## License

TantivyEx is licensed under the MIT License. See [LICENSE](LICENSE) for details.

## Acknowledgments

- **[Tantivy](https://github.com/quickwit-oss/tantivy)**: The powerful Rust search engine that powers TantivyEx
- **[Rustler](https://github.com/rusterlium/rustler)**: For excellent Rust-Elixir integration
