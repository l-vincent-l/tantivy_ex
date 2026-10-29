# Schema Design Guide

A schema lists the fields of the documents, their types, and what the index
does with each of them: search them, give them back in the results, or keep
them in columns for ranges, sorting and aggregations. It is fixed when the
index is created.

## Related Documentation

- **[Core Concepts](core-concepts.md)**: schemas, indexes and documents
- **[Documents](documents.md)**: adding documents to an index
- **[Tokenizers](tokenizers.md)**: how text fields are analyzed
- **[Search](search.md)**: querying the fields

## Building a Schema

`Schema.new/0` creates an empty schema, and each `add_*_field` function
returns the schema with one more field:

```elixir
alias TantivyEx.{Schema, Index, IndexWriter, Searcher, Query, Aggregation}

schema =
  Schema.new()
  |> Schema.add_text_field("title", :text_stored)
  |> Schema.add_text_field("body", :text)
  |> Schema.add_u64_field("id", :indexed_stored)
  |> Schema.add_f64_field("price", :fast_stored)
  |> Schema.add_facet_field("category")

Schema.get_field_names(schema)
# => ["title", "body", "id", "price", "category"]
```

The schema can be inspected:

```elixir
Schema.get_field_type(schema, "price")
# => {:ok, "f64"}
Schema.get_field_type(schema, "nope")
# => {:error, "Field 'nope' not found in schema"}
Schema.field_exists?(schema, "body")
# => true
```

## Field Options

Each option is a combination of three properties:

- **indexed**: the field can be searched;
- **stored**: the field is given back in the search results;
- **fast**: the field is kept in a column, for ranges, sorting and
  aggregations.

| Option | Text fields | Numeric, date, bool, bytes, IP fields |
|---|---|---|
| `:text` | indexed | |
| `:text_stored` | indexed, stored | |
| `:indexed` | | indexed |
| `:indexed_stored` | | indexed, stored |
| `:fast` | indexed, fast | fast |
| `:fast_stored` | indexed, stored, fast | fast, stored |
| `:stored` | stored | stored |

Store only what the results display: a stored field takes space, and a large
text such as a body can often be only indexed. A field that is not indexed
cannot be searched:

```elixir
notes = Schema.add_text_field(Schema.new(), "note", :stored)

{:error, "Failed to create term query: Field 'note' is not indexed"} =
  Query.term(notes, "note", "hello")
```

### Indexed or fast numeric fields

A numeric field is either indexed or fast, not both. Choose by use:

- `:indexed` and `:indexed_stored` fields match exact values (`id:42`), but
  cannot be aggregated;
- `:fast` and `:fast_stored` fields match ranges (`price:[10 TO 20]`) and
  can be aggregated and sorted, but not searched for an exact value with a
  term: use a range of one value instead.

```elixir
{:ok, index} = Index.create_in_ram(schema)
{:ok, writer} = IndexWriter.new(index)

:ok =
  IndexWriter.add_document(writer, %{
    "title" => "Mechanical keyboard",
    "body" => "A keyboard with brown switches.",
    "id" => 1,
    "price" => 89.5,
    "category" => "/electronics/keyboards"
  })

:ok =
  IndexWriter.add_document(writer, %{
    "title" => "Wireless mouse",
    "body" => "A mouse with a long battery life.",
    "id" => 2,
    "price" => 25.0,
    "category" => "/electronics/mice"
  })

:ok = IndexWriter.commit(writer)
{:ok, catalog} = Searcher.new(index)

# An exact value on an indexed field
{:ok, [%{"title" => "Wireless mouse"}]} = Searcher.search(catalog, "id:2", 10)

# A range on a fast field
{:ok, [%{"title" => "Mechanical keyboard"}]} =
  Searcher.search(catalog, "price:[50 TO 100]", 10)

# An aggregation on a fast field
{:ok, all} = Query.all()
{:ok, %{"total" => %{"value" => total}}} =
  Aggregation.run(catalog, all, %{"total" => Aggregation.metric(:sum, "price")})
total
# => 114.5
```

The results give the stored fields, not the others (`body` here is only
indexed):

```elixir
{:ok, [result]} = Searcher.search(catalog, "id:1", 10)
result |> Map.delete("score") |> Map.keys() |> Enum.sort()
# => ["category", "id", "price", "title"]
```

## Field Types

### Text

Text fields are analyzed by a tokenizer, which splits them into terms: the
`"default"` one splits words and lowercases them.
`Schema.add_text_field_with_tokenizer/4` picks another one, such as `"raw"`,
which keeps the whole value as a single term, for identifiers and codes:

```elixir
products =
  Schema.new()
  |> Schema.add_text_field("name", :text_stored)
  |> Schema.add_text_field_with_tokenizer("sku", :text_stored, "raw")
  |> Schema.add_text_field_with_tokenizer("description", :text, "en_stem")
```

The [Tokenizers guide](tokenizers.md) lists the tokenizers and shows how to
register new ones.

### Numbers and booleans

`add_u64_field/3` (unsigned integers: identifiers, counts),
`add_i64_field/3` (signed integers) and `add_f64_field/3` (floats: prices,
ratings) take the numeric options; `add_bool_field/3` too.

```elixir
measures =
  Schema.new()
  |> Schema.add_u64_field("views", :fast_stored)
  |> Schema.add_i64_field("temperature", :indexed_stored)
  |> Schema.add_f64_field("rating", :fast_stored)
  |> Schema.add_bool_field("published", :indexed_stored)

{:ok, index} = Index.create_in_ram(measures)
{:ok, writer} = IndexWriter.new(index)
:ok = IndexWriter.add_document(writer, %{"views" => 10, "temperature" => -5, "rating" => 4.5, "published" => true})
:ok = IndexWriter.commit(writer)
{:ok, searcher} = Searcher.new(index)

{:ok, [_]} = Searcher.search(searcher, "temperature:-5 AND published:true", 10)
```

A value of the wrong type is rejected when the document is added:

```elixir
{:error, "Field 'views': Negative value not allowed for u64 field"} =
  IndexWriter.add_document(writer, %{"views" => -1})
```

### Dates

Date fields take an ISO 8601 string or a Unix timestamp in seconds, and give
back an ISO 8601 string. Make them fast to filter them by ranges:

```elixir
events = Schema.add_date_field(Schema.new(), "at", :fast_stored)
{:ok, index} = Index.create_in_ram(events)
{:ok, writer} = IndexWriter.new(index)
:ok = IndexWriter.add_document(writer, %{"at" => "2024-01-15T10:30:00Z"})
:ok = IndexWriter.add_document(writer, %{"at" => 1_717_200_000})
:ok = IndexWriter.commit(writer)
{:ok, searcher} = Searcher.new(index)

{:ok, [%{"at" => at}]} = Searcher.search(searcher, "at:[2024-06-01T00:00:00Z TO *]", 10)
at
# => "2024-06-01T00:00:00Z"
```

### Facets

A facet is a path in a hierarchy, such as `/electronics/keyboards`.
`add_facet_field/2` takes no options: facets are indexed and stored. A query
on a facet matches its descendants too:

```elixir
{:ok, query} = Query.facet_term(schema, "category", "/electronics")
{:ok, results} = Searcher.search(catalog, query, 10)
length(results)
# => 2

{:ok, [%{"category" => "/electronics/mice"}]} =
  Searcher.search(catalog, "category:/electronics/mice", 10)
```

The [Search guide](search.md) shows facet counts.

### JSON

A JSON field holds a map, or a string of a JSON object. With `:text` or
`:text_stored`, its keys can be searched as `field.key`; with `:stored`, it
is only given back:

```elixir
logs =
  Schema.new()
  |> Schema.add_text_field("message", :text_stored)
  |> Schema.add_json_field("meta", :text_stored)

{:ok, index} = Index.create_in_ram(logs)
{:ok, writer} = IndexWriter.new(index)

:ok =
  IndexWriter.add_document(writer, %{
    "message" => "Payment failed",
    "meta" => %{"service" => "billing", "attempts" => 3, "tags" => ["retry", "card"]}
  })

:ok = IndexWriter.commit(writer)
{:ok, searcher} = Searcher.new(index)

{:ok, [%{"meta" => meta}]} = Searcher.search(searcher, "meta.service:billing", 10)
meta
# => %{"service" => "billing", "attempts" => 3, "tags" => ["retry", "card"]}
```

### Bytes and IP addresses

Bytes fields take and give back base64 strings. IP address fields take IPv4
or IPv6 addresses:

```elixir
network =
  Schema.new()
  |> Schema.add_bytes_field("checksum", :stored)
  |> Schema.add_ip_addr_field("client", :indexed_stored)

{:ok, index} = Index.create_in_ram(network)
{:ok, writer} = IndexWriter.new(index)
:ok = IndexWriter.add_document(writer, %{"checksum" => Base.encode64(<<1, 2, 3>>), "client" => "192.168.1.10"})
:ok = IndexWriter.commit(writer)
{:ok, searcher} = Searcher.new(index)

{:ok, [result]} = Searcher.search(searcher, "client:192.168.1.10", 10)
{Base.decode64!(result["checksum"]), result["client"]}
# => {<<1, 2, 3>>, "192.168.1.10"}
```

## Multi-valued Fields

Any field can hold several values: give a list. The results give a list for
a field with several values, and the value itself for a field with one:

```elixir
tagged = Schema.add_text_field(Schema.new(), "tags", :text_stored)
{:ok, index} = Index.create_in_ram(tagged)
{:ok, writer} = IndexWriter.new(index)
:ok = IndexWriter.add_document(writer, %{"tags" => ["elixir", "search"]})
:ok = IndexWriter.add_document(writer, %{"tags" => "rust"})
:ok = IndexWriter.commit(writer)
{:ok, searcher} = Searcher.new(index)

{:ok, [%{"tags" => tags}]} = Searcher.search(searcher, "search", 10)
tags
# => ["elixir", "search"]
{:ok, [%{"tags" => tag}]} = Searcher.search(searcher, "rust", 10)
tag
# => "rust"
```

## Changing a Schema

The schema is stored with the index, and cannot change: opening an index with
a different schema fails.

```elixir
{:ok, _index} = Index.open_or_create("products_index", schema)

{:error, "Failed to open or create index: Schema error: 'An index exists but the schema does not match.'"} =
  Index.open_or_create("products_index", Schema.add_text_field(schema, "brand", :text_stored))
```

To add, remove or change a field, create a new index with the new schema, and
add the documents to it again from their source. Keep the source of the
documents (a database, files) for that: an index only gives back the stored
fields.

## Example Schemas

An online shop, searched by name and description, filtered by price and
category, and aggregated by brand:

```elixir
shop =
  Schema.new()
  |> Schema.add_u64_field("id", :indexed_stored)
  |> Schema.add_text_field("name", :text_stored)
  |> Schema.add_text_field_with_tokenizer("description", :text, "en_stem")
  |> Schema.add_text_field_with_tokenizer("sku", :text_stored, "raw")
  |> Schema.add_text_field("brand", :fast_stored)
  |> Schema.add_f64_field("price", :fast_stored)
  |> Schema.add_bool_field("in_stock", :indexed_stored)
  |> Schema.add_facet_field("category")
```

Application logs, searched by message, filtered by level and time, with
structured metadata:

```elixir
log_schema =
  Schema.new()
  |> Schema.add_text_field("message", :text_stored)
  |> Schema.add_text_field_with_tokenizer("level", :text_stored, "raw")
  |> Schema.add_date_field("timestamp", :fast_stored)
  |> Schema.add_ip_addr_field("client_ip", :indexed_stored)
  |> Schema.add_json_field("metadata", :text_stored)
```
