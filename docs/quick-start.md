# Quick Start Tutorial

Let's build a small blog search engine. Each step runs in order in an IEx
session (`iex -S mix`), and reuses the variables of the previous ones.

## Step 1: Define Your Schema

The schema lists the fields of the documents, and what the index does with
each of them. Its functions return the updated schema.

```elixir
alias TantivyEx.{Schema, Index, IndexWriter, Searcher, Query, Document}

schema =
  Schema.new()
  |> Schema.add_u64_field("id", :indexed_stored)
  |> Schema.add_text_field("title", :text_stored)
  |> Schema.add_text_field("body", :text_stored)
  |> Schema.add_text_field("author", :text_stored)
  |> Schema.add_f64_field("rating", :fast_stored)
  |> Schema.add_facet_field("category")
```

- `:text_stored` fields are searchable and returned in the results; `:text`
  fields are only searchable.
- `:indexed_stored` numeric fields can be searched for exact values,
  `:fast_stored` ones are used for ranges, sorting and aggregations.
- A facet field holds hierarchical categories, such as `/programming/elixir`.

## Step 2: Create Your Index

```elixir
# A persistent index, created in the directory if it does not exist yet
{:ok, index} = Index.open_or_create("blog_index", schema)
```

`Index.create_in_ram/1` creates an index in memory instead, convenient in
tests; `Index.open/1` opens an existing one.

## Step 3: Add Documents

```elixir
posts = [
  %{
    "id" => 1,
    "title" => "Getting Started with Elixir",
    "body" => "Elixir is a dynamic, functional language for building maintainable applications.",
    "author" => "Jane Doe",
    "rating" => 4.5,
    "category" => "/programming/elixir"
  },
  %{
    "id" => 2,
    "title" => "Advanced Phoenix Patterns",
    "body" => "Phoenix, written in Elixir, builds real-time web applications.",
    "author" => "John Smith",
    "rating" => 4.8,
    "category" => "/programming/phoenix"
  },
  %{
    "id" => 3,
    "title" => "Rust Performance Tips",
    "body" => "Rust offers zero-cost abstractions and memory safety without a garbage collector.",
    "author" => "Alice Johnson",
    "rating" => 4.2,
    "category" => "/programming/rust"
  }
]

{:ok, writer} = IndexWriter.new(index)
Enum.each(posts, fn post -> :ok = IndexWriter.add_document(writer, post) end)

# The documents become searchable once committed
:ok = IndexWriter.commit(writer)
```

A document with a field missing from the schema, or a value of the wrong
type, is rejected with an error naming the field:

```elixir
{:error, "Unknown field: 'titel'"} = IndexWriter.add_document(writer, %{"titel" => "Typo"})
```

## Step 4: Search Your Content

A query string is parsed over the text fields, with the same analysis as the
indexed text: "ELIXIR" finds "Elixir".

```elixir
{:ok, searcher} = Searcher.new(index)

# The word is in the first title, and in the second body
{:ok, results} = Searcher.search(searcher, "ELIXIR", 10)
length(results)
# => 2
```

Each result is a map with the `"score"` and the stored fields, best match
first:

```elixir
[%{"score" => score, "title" => title} | _] = results
title
# => "Getting Started with Elixir"
is_float(score)
# => true
```

The query syntax selects fields, combines terms and filters ranges:

```elixir
# A field
{:ok, [%{"id" => 2}]} = Searcher.search(searcher, "title:phoenix", 10)

# Every term required
{:ok, [%{"id" => 2}]} = Searcher.search(searcher, "elixir AND phoenix", 10)

# A phrase
{:ok, [%{"id" => 3}]} = Searcher.search(searcher, ~s("memory safety"), 10)

# A range, on a fast field
{:ok, results} = Searcher.search(searcher, "rating:[4.5 TO *]", 10)
length(results)
# => 2
```

Queries can also be built with `TantivyEx.Query`, and combined:

```elixir
{:ok, elixir} = Query.term(schema, "body", "elixir")
{:ok, rust} = Query.facet_term(schema, "category", "/programming/rust")
{:ok, query} = Query.boolean([], [elixir, rust], [])

# Either clause: should clauses
{:ok, results} = Searcher.search(searcher, query, 10)
length(results)
# => 3
```

## Step 5: Show Snippets

`Searcher.snippet/5` gives the fragment of a text that best matches a query,
with the byte ranges of the matched terms, to highlight them:

```elixir
{:ok, query} = Query.term(schema, "body", "applications")
{:ok, [post | _]} = Searcher.search(searcher, query, 1)

{:ok, %{fragment: fragment, highlighted: [{start, stop} | _]}} =
  Searcher.snippet(searcher, query, "body", post["body"], max_num_chars: 60)

binary_part(fragment, start, stop - start)
# => "applications"
```

## Step 6: Update and Delete Documents

Tantivy has no in-place update: `Document.update/5` deletes the documents
whose field has the given value, and adds the new version. Give documents an
identifier of your own, such as `"id"` here.

```elixir
updated = %{
  "id" => 3,
  "title" => "Rust Performance Tips, Revised",
  "body" => "Rust offers zero-cost abstractions.",
  "author" => "Alice Johnson",
  "rating" => 4.6,
  "category" => "/programming/rust"
}

{:ok, :updated} = Document.update(writer, "id", 3, updated, schema)
{:ok, :deleted} = Document.delete(writer, "id", 1, schema)
:ok = IndexWriter.commit(writer)

# A searcher sees the index as it was when created: take a new one
{:ok, searcher} = Searcher.new(index)
{:ok, results} = Searcher.search(searcher, "*", 10)
results |> Enum.map(& &1["id"]) |> Enum.sort()
# => [2, 3]
```

## Next Steps

- **[Core Concepts](core-concepts.md)**: the building blocks of an index
- **[Schema Design](schema.md)**: field types and options
- **[Documents](documents.md)** and **[Indexing](indexing.md)**: adding, updating and deleting
- **[Search](search.md)**: queries in depth
- **[Tokenizers](tokenizers.md)**: text analysis, languages and stemming
- **[Aggregations](aggregations.md)**: counts, statistics and histograms
