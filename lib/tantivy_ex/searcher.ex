defmodule TantivyEx.Searcher do
  @moduledoc """
  Searcher for querying a TantivyEx index.

  The Searcher provides functionality to search documents in an index
  and retrieve search results using various query types including:
  - Simple string queries (parsed automatically)
  - Complex query objects built with TantivyEx.Query
  - Boolean combinations of multiple queries
  """

  alias TantivyEx.{Native, Index, Query}

  @type t :: reference()
  @typedoc """
  A search result: its `"score"` and, when documents are included, its stored
  fields by name. A field with several values gives a list of them.
  """
  @type search_result :: %{optional(String.t()) => term()}
  @type snippet :: %{
          fragment: String.t(),
          highlighted: [{non_neg_integer(), non_neg_integer()}]
        }

  @doc """
  Creates a new Searcher for the given index.

  ## Parameters

  - `index`: The index to search

  ## Examples

      iex> {:ok, searcher} = TantivyEx.Searcher.new(index)
      iex> is_reference(searcher)
      true
  """
  @spec new(Index.t()) :: {:ok, t()} | {:error, String.t()}
  def new(index) do
    case Native.index_reader(index) do
      {:error, reason} -> {:error, reason}
      searcher -> {:ok, searcher}
    end
  end

  @doc """
  Searches the index with the given query.

  This function supports both query strings, parsed with the index's tokenizers
  over all the indexed text fields, and Query objects created with
  TantivyEx.Query functions.

  Each result is a map with the `"score"` and, with `include_docs`, the stored
  fields of the document. A field with several values gives a list of them,
  a field with one value gives that value.

  ## Parameters

  - `searcher`: The Searcher
  - `query`: The search query (string or Query.t())
  - `limit`: Maximum number of results to return (default: 10)
  - `include_docs`: Whether to include full document content (default: true)

  ## Examples

      # Query string, over all the indexed text fields
      iex> {:ok, results} = TantivyEx.Searcher.search(searcher, "hello world", 10)

      # Using Query objects for precise control
      iex> {:ok, query} = TantivyEx.Query.term(schema, "title", "hello")
      iex> {:ok, results} = TantivyEx.Searcher.search(searcher, query, 10)

      # Boolean query example
      iex> {:ok, term1} = TantivyEx.Query.term(schema, "title", "hello")
      iex> {:ok, term2} = TantivyEx.Query.term(schema, "body", "world")
      iex> {:ok, boolean_query} = TantivyEx.Query.boolean([term1], [term2], [])
      iex> {:ok, results} = TantivyEx.Searcher.search(searcher, boolean_query, 10)
  """
  @spec search(t(), String.t() | Query.t(), pos_integer(), boolean()) ::
          {:ok, [search_result()]} | {:error, String.t()}
  def search(searcher, query, limit \\ 10, include_docs \\ true)

  def search(searcher, query, limit, include_docs) when is_binary(query) do
    searcher
    |> Native.searcher_search(query, [], limit, include_docs)
    |> search_result()
  end

  def search(searcher, query, limit, include_docs) when is_reference(query) do
    searcher
    |> Native.searcher_search_with_query(query, limit, include_docs)
    |> search_result()
  end

  @doc """
  Generates a snippet of `text`, with the terms of `query` found in `field`.

  This uses Tantivy's `SnippetGenerator`: `text` is tokenized with the field's
  tokenizer, so terms match exactly as they do in the search (case, stemming,
  whole words). The text does not need to be stored in the index: pass the
  field's text of a search result, from the index or from elsewhere.

  Returns the fragment of `text` that best matches the query, and the byte
  ranges `{start, stop}` of its highlighted terms (`stop` excluded), usable
  with `binary_part(fragment, start, stop - start)`. When no term matches,
  the fragment is empty.

  ## Parameters

  - `searcher`: The Searcher
  - `query`: The query whose terms are highlighted (Query.t())
  - `field`: The name of the text field the terms are taken from
  - `text`: The text to extract the snippet from
  - `opts`:
    - `:max_num_chars` - Maximum length of the fragment (default: 150)

  ## Examples

      iex> {:ok, parser} = TantivyEx.Query.parser(index, ["body"])
      iex> {:ok, query} = TantivyEx.Query.parse(parser, "rust")
      iex> {:ok, snippet} = TantivyEx.Searcher.snippet(searcher, query, "body", "Learn Rust today")
      iex> snippet
      %{fragment: "Learn Rust today", highlighted: [{6, 10}]}
  """
  @spec snippet(t(), Query.t(), String.t(), String.t(), keyword()) ::
          {:ok, snippet()} | {:error, String.t()}
  def snippet(searcher, query, field, text, opts \\ []) when is_reference(query) do
    max_num_chars = Keyword.get(opts, :max_num_chars, 150)

    case Native.searcher_snippet(searcher, query, field, text, max_num_chars) do
      {:error, reason} -> {:error, reason}
      {fragment, highlighted} -> {:ok, %{fragment: fragment, highlighted: highlighted}}
    end
  end

  @doc """
  Performs a search and returns full documents with metadata.

  This function always includes full document content and is optimized for cases
  where you need the complete document data.

  ## Parameters

  - `searcher`: The Searcher
  - `query`: The search query (Query.t() only - no string queries)
  - `limit`: Maximum number of results to return (default: 10)

  ## Examples

      iex> {:ok, query} = TantivyEx.Query.phrase(schema, "title", ["hello", "world"])
      iex> {:ok, results} = TantivyEx.Searcher.search_documents(searcher, query, 10)
      iex> [%{"score" => 1.0, "title" => "hello world", "body" => "..."}] = results
  """
  @spec search_documents(t(), Query.t(), pos_integer()) ::
          {:ok, [search_result()]} | {:error, String.t()}
  def search_documents(searcher, query, limit \\ 10) when is_reference(query) do
    search(searcher, query, limit, true)
  end

  @doc """
  Performs a search with a query parser for Lucene-style queries.

  This is a convenience function that parses the query string with the index's
  tokenizers and performs the search in one call.

  ## Parameters

  - `searcher`: The Searcher
  - `default_fields`: List of fields searched by terms without a field
  - `query_str`: The Lucene-style query string
  - `limit`: Maximum number of results to return (default: 10)
  - `include_docs`: Whether to include full document content (default: true)

  ## Examples

      iex> {:ok, results} = TantivyEx.Searcher.search_with_parser(
      ...>   searcher, ["title", "body"],
      ...>   "title:hello AND body:world", 10
      ...> )
  """
  @spec search_with_parser(t(), [String.t()], String.t(), pos_integer(), boolean()) ::
          {:ok, [search_result()]} | {:error, String.t()}
  def search_with_parser(searcher, default_fields, query_str, limit \\ 10, include_docs \\ true)
      when is_list(default_fields) and is_binary(query_str) do
    searcher
    |> Native.searcher_search(query_str, default_fields, limit, include_docs)
    |> search_result()
  end

  # The search NIFs return the results, or `{:error, reason}`
  defp search_result({:error, reason}), do: {:error, reason}
  defp search_result(results) when is_list(results), do: {:ok, results}
end
