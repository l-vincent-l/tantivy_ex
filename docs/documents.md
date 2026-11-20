# Documents Guide

A document is a map of field names to values. This guide shows the values
each field type takes, how `TantivyEx.Document` validates and converts them,
and how to add, update and delete documents.

## Related Documentation

- **[Schema Design](schema.md)**: the field types and their options
- **[Indexing](indexing.md)**: writers, commits and index storage
- **[Search](search.md)**: querying the documents

## The Values of a Document

The examples of this guide use this schema:

```elixir
alias TantivyEx.{Schema, Index, IndexWriter, Searcher, Query, Document}

schema =
  Schema.new()
  |> Schema.add_u64_field("id", :indexed_stored)
  |> Schema.add_text_field("title", :text_stored)
  |> Schema.add_text_field_with_tokenizer("sku", :text_stored, "raw")
  |> Schema.add_text_field("tags", :text_stored)
  |> Schema.add_f64_field("price", :fast_stored)
  |> Schema.add_bool_field("in_stock", :indexed_stored)
  |> Schema.add_date_field("added_at", :fast_stored)
  |> Schema.add_facet_field("category")
  |> Schema.add_json_field("specs", :text_stored)

{:ok, index} = Index.create_in_ram(schema)
{:ok, writer} = IndexWriter.new(index)
```

Each field takes values of its type:

| Field type | Values |
|---|---|
| text | a string |
| u64, i64 | an integer (non-negative for u64) |
| f64 | a number |
| bool | `true` or `false` |
| date | an ISO 8601 string, or a Unix timestamp in seconds |
| facet | a path, such as `"/electronics/keyboards"` |
| bytes | a base64 string |
| JSON | a map, or a string of a JSON object |
| IP address | an IPv4 or IPv6 string |

A list gives a field several values. A field can be left out:

```elixir
:ok =
  IndexWriter.add_document(writer, %{
    "id" => 1,
    "title" => "Mechanical keyboard",
    "sku" => "KB-001",
    "tags" => ["keyboard", "mechanical"],
    "price" => 89.5,
    "in_stock" => true,
    "added_at" => "2024-01-15T10:30:00Z",
    "category" => "/electronics/keyboards",
    "specs" => %{"switches" => "brown", "layout" => "ISO"}
  })

:ok = IndexWriter.add_document(writer, %{"id" => 2, "title" => "Wireless mouse"})
```

`IndexWriter.add_document/2` takes the values as they are, and rejects a
value of the wrong type or an unknown field:

```elixir
{:error, "Field 'id': Expected numeric value"} =
  IndexWriter.add_document(writer, %{"id" => "3"})

{:error, "Unknown field: 'colour'"} =
  IndexWriter.add_document(writer, %{"id" => 3, "colour" => "red"})
```

## Validating and Converting

`Document.validate/2` checks a document against the schema, and converts the
values that can be: a numeric string for a numeric field, `"true"` or
`"false"` for a bool field, a `DateTime` for a date field, a path without its
leading slash for a facet field. Data from a form or a CSV file comes as
strings:

```elixir
Document.validate(%{"id" => "3", "price" => "19.99", "in_stock" => "false"}, schema)
# => {:ok, %{"id" => 3, "price" => 19.99, "in_stock" => false}}

Document.validate(%{"category" => "electronics/mice"}, schema)
# => {:ok, %{"category" => "/electronics/mice"}}
```

An invalid document gives the field and the reason:

```elixir
Document.validate(%{"id" => -1}, schema)
# => {:error, "Document validation failed: Field 'id': Invalid u64 value: -1"}

Document.validate(%{"colour" => "red"}, schema)
# => {:error, "Document validation failed: Unknown field: 'colour'"}
```

`Document.add/3` validates a document, then adds it:

```elixir
:ok =
  Document.add(
    writer,
    %{"id" => "3", "title" => "USB cable", "price" => "9", "added_at" => ~U[2024-03-01 09:00:00Z]},
    schema
  )

{:error, "Document validation failed: Field 'price': Invalid f64 value: \"cheap\""} =
  Document.add(writer, %{"id" => 4, "price" => "cheap"}, schema)

:ok = IndexWriter.commit(writer)
{:ok, searcher} = Searcher.new(index)
{:ok, [cable]} = Searcher.search(searcher, "id:3", 10)
{cable["price"], cable["added_at"]}
# => {9.0, "2024-03-01T09:00:00Z"}
```

## Adding Documents in Batches

`Document.add_batch/4` validates all the documents first. If one is invalid,
nothing is added, and the errors give the position of each invalid document
in the list:

```elixir
batch = [
  %{"id" => 10, "title" => "Monitor stand"},
  %{"id" => "eleven", "title" => "Desk lamp"},
  %{"id" => 12, "title" => "Webcam"}
]

Document.add_batch(writer, batch, schema)
# => {:error, [{1, "Field 'id': Invalid u64 value: \"eleven\""}]}
```

With `continue_on_error: true`, the valid documents are added, and the
invalid ones are returned with their errors:

```elixir
Document.add_batch(writer, batch, schema, continue_on_error: true)
# => {:ok, 2, [{1, "Field 'id': Invalid u64 value: \"eleven\""}]}

Document.add_batch(writer, [%{"id" => 13}, %{"id" => 14}], schema)
# => {:ok, 2}
```

The [Indexing guide](indexing.md) shows how to index a large source in
batches.

## Updating and Deleting Documents

Tantivy has no in-place update: `Document.update/5` deletes the documents
whose field has the given value, and adds the new version.
`Document.delete/4` only deletes them. Both need a commit to be visible, like
additions:

```elixir
:ok = Document.update(writer, "id", 3, %{"id" => 3, "title" => "USB-C cable", "price" => 12.0}, schema)
:ok = Document.delete(writer, "id", 2, schema)
:ok = IndexWriter.commit(writer)

{:ok, searcher} = Searcher.new(index)
{:ok, [updated]} = Searcher.search(searcher, "id:3", 10)
updated["title"]
# => "USB-C cable"
Searcher.search(searcher, "id:2", 10)
# => {:ok, []}
```

The new version replaces the whole document: give all its fields, not only
the changed ones. Read the others from your source of documents, or from the
stored fields of the old version.

### Identifying documents

The value is matched as a search would match it. A text value goes through
the field's tokenizer: on a `"default"` text field, `"Mechanical keyboard"`
matches every title containing these two words, in this order. Identify
documents by a numeric field, or by a text field with the `"raw"` tokenizer,
which keeps the whole value as one term:

```elixir
:ok = Document.delete(writer, "sku", "KB-001", schema)
:ok = IndexWriter.commit(writer)

{:ok, searcher} = Searcher.new(index)
Searcher.search(searcher, "id:1", 10)
# => {:ok, []}
```

Deleting a value that no document has is not an error. An unknown field is:

```elixir
:ok = Document.delete(writer, "id", 999, schema)

Document.delete(writer, "colour", "red", schema)
# => {:error, "Field 'colour' not found in schema"}
```

To delete the documents matching any query, use
`IndexWriter.delete_documents/2`:

```elixir
{:ok, cheap} = Query.range_f64(schema, "price", 0.0, 15.0)
:ok = IndexWriter.delete_documents(writer, cheap)
:ok = IndexWriter.commit(writer)

{:ok, searcher} = Searcher.new(index)
Searcher.search(searcher, "id:3", 10)
# => {:ok, []}
```

## Documents from JSON

`Document.prepare_json/3` builds a document from a JSON object, a string or
a decoded map, and validates it. A mapping picks the values by their path in
the object:

```elixir
json = ~s({"product": {"ref": 20, "name": "Keyboard tray"}, "pricing": {"amount": "24.5"}})

mapping = %{
  "product.ref" => "id",
  "product.name" => "title",
  "pricing.amount" => "price"
}

{:ok, tray} = Document.prepare_json(json, schema, mapping)
tray
# => %{"id" => 20, "title" => "Keyboard tray", "price" => 24.5}

:ok = Document.add(writer, tray, schema)
```

Without a mapping, the nested objects are flattened, their keys joined with
dots, so the keys of the object must be the field names.
