# Indexing Guide

Indexing turns documents into the structures a search reads. An index
writer adds and deletes documents, and a commit makes the changes visible to
new searchers. This guide covers the index storage, the writer, commits and
rollbacks, and indexing a large source.

## Related Documentation

- **[Schema Design](schema.md)**: the fields of an index
- **[Documents](documents.md)**: the values of documents, their validation, updates and deletions
- **[Search](search.md)**: querying an index

## Creating and Opening an Index

An index lives in memory or in a directory. The examples of this guide use
this schema:

```elixir
alias TantivyEx.{Schema, Index, IndexWriter, Searcher, Query, Document}

schema =
  Schema.new()
  |> Schema.add_u64_field("id", :indexed_stored)
  |> Schema.add_text_field("title", :text_stored)
  |> Schema.add_text_field_with_tokenizer("status", :text_stored, "raw")
```

`Index.create_in_ram/1` creates an index that disappears with the VM, for
tests and temporary data. `Index.create_in_dir/2` creates one in a
directory, which must not hold an index yet; `Index.open/1` opens an
existing one:

```elixir
{:ok, _ram} = Index.create_in_ram(schema)

{:ok, _disk} = Index.create_in_dir("articles_index", schema)

Index.create_in_dir("articles_index", schema)
# => {:error, "Failed to create index: Index already exists"}

{:error, "Failed to open index: " <> _} = Index.open("missing_index")
```

`Index.open_or_create/2` does either, and suits an application that creates
its index on its first start. The schema is stored with the index: opening it
with a different schema fails, as the [Schema guide](schema.md#changing-a-schema)
shows.

```elixir
{:ok, index} = Index.open_or_create("articles_index", schema)
```

## The Index Writer

`IndexWriter.new/2` creates the writer of an index. Its memory budget, 50 MB
by default, is the memory it uses to build segments before writing them; a
larger one indexes large volumes faster. Tantivy needs at least 15 MB, and a
smaller budget is raised to it.

```elixir
{:ok, writer} = IndexWriter.new(index, 100_000_000)
```

An index has one writer at a time: a second one fails while the first
exists, in this VM or in another process. The lock is released when the
writer is garbage collected.

```elixir
{:error, "Failed to create writer: Failed to acquire Lockfile: LockBusy." <> _} =
  IndexWriter.new(index)
```

Keep the writer in one process, such as a GenServer of your application,
and send it the documents to index:

```elixir
defmodule MyApp.Indexer do
  use GenServer

  alias TantivyEx.{IndexWriter, Document}

  def start_link({index, schema}), do: GenServer.start_link(__MODULE__, {index, schema})

  def add(indexer, document), do: GenServer.call(indexer, {:add, document})
  def commit(indexer), do: GenServer.call(indexer, :commit)

  @impl true
  def init({index, schema}) do
    with {:ok, writer} <- IndexWriter.new(index) do
      {:ok, %{writer: writer, schema: schema}}
    end
  end

  @impl true
  def handle_call({:add, document}, _from, state) do
    {:reply, Document.add(state.writer, document, state.schema), state}
  end

  def handle_call(:commit, _from, state) do
    {:reply, IndexWriter.commit(state.writer), state}
  end
end
```

```elixir
{:ok, notes_index} = Index.create_in_ram(schema)
{:ok, indexer} = MyApp.Indexer.start_link({notes_index, schema})

:ok = MyApp.Indexer.add(indexer, %{"id" => "1", "title" => "Sent to the indexer"})
:ok = MyApp.Indexer.commit(indexer)

{:ok, searcher} = Searcher.new(notes_index)
{:ok, [%{"id" => 1}]} = Searcher.search(searcher, "indexer", 10)
```

## Commits

Additions and deletions are visible only after a commit, and only to the
searchers created after it: a searcher sees the index as it was when it was
created.

```elixir
:ok = IndexWriter.add_document(writer, %{"id" => 1, "title" => "Draft notes", "status" => "draft"})
{:ok, before_commit} = Searcher.new(index)
{:ok, all} = Query.all()

Searcher.search(before_commit, all, 10)
# => {:ok, []}

:ok = IndexWriter.commit(writer)

# The searcher created before the commit still sees the old index
Searcher.search(before_commit, all, 10)
# => {:ok, []}

{:ok, searcher} = Searcher.new(index)
{:ok, [%{"title" => "Draft notes"}]} = Searcher.search(searcher, all, 10)
```

A commit writes the new segments to disk and waits for them: it is the
expensive step of indexing. Commit after a batch of changes, not after each
document. The changes since the last commit are lost if the VM stops before
the next one, and are found again in the source of the documents.

## Rolling Back

`IndexWriter.rollback/1` discards the changes since the last commit, for
instance when a batch fails halfway. The writer can be used again:

```elixir
:ok = IndexWriter.add_document(writer, %{"id" => 2, "title" => "Half-imported", "status" => "draft"})
:ok = IndexWriter.rollback(writer)

:ok = IndexWriter.add_document(writer, %{"id" => 3, "title" => "Launch post", "status" => "published"})
:ok = IndexWriter.commit(writer)

{:ok, searcher} = Searcher.new(index)
{:ok, results} = Searcher.search(searcher, all, 10)
results |> Enum.map(& &1["id"]) |> Enum.sort()
# => [1, 3]
```

## Deleting Documents

`IndexWriter.delete_documents/2` deletes the documents matching a query, and
`Document.delete/4` the documents having a value, as the
[Documents guide](documents.md#updating-and-deleting-documents) shows.

```elixir
{:ok, drafts} = Query.term(schema, "status", "draft")
:ok = IndexWriter.delete_documents(writer, drafts)
:ok = IndexWriter.commit(writer)

{:ok, searcher} = Searcher.new(index)
{:ok, [%{"id" => 3}]} = Searcher.search(searcher, all, 10)
```

`IndexWriter.delete_all_documents/1` deletes the committed documents. The
documents added since the last commit are kept:

```elixir
:ok = IndexWriter.add_document(writer, %{"id" => 4, "title" => "Pending", "status" => "draft"})
:ok = IndexWriter.delete_all_documents(writer)
:ok = IndexWriter.commit(writer)

{:ok, searcher} = Searcher.new(index)
{:ok, [%{"id" => 4}]} = Searcher.search(searcher, all, 10)
```

## Indexing a Large Source

Stream the documents from their source, add them in batches, and commit at
the end, or every few batches to make the progress visible and durable:

```elixir
source =
  Stream.map(1..1_000, fn n ->
    %{"id" => 100 + n, "title" => "Article #{n}", "status" => "published"}
  end)

source
|> Stream.chunk_every(250)
|> Enum.each(fn batch ->
  {:ok, _added} = Document.add_batch(writer, batch, schema)
end)

:ok = IndexWriter.commit(writer)

{:ok, searcher} = Searcher.new(index)
{:ok, published} = Query.term(schema, "status", "published")
{:ok, results} = Searcher.search(searcher, published, 2_000)
length(results)
# => 1000
```

`IndexWriter.add_document/2` skips the validation in Elixir of
`Document.add_batch/4`: use it for documents already of the right types. To rebuild an index from its source, for instance after a
schema change, index into a new directory, then switch the application to
it.

## Reopening an Index

A committed index is found again by opening its directory, after a restart
or from another process:

```elixir
{:ok, reopened} = Index.open("articles_index")
{:ok, searcher} = Searcher.new(reopened)
{:ok, results} = Searcher.search(searcher, all, 2_000)
length(results)
# => 1001
```
