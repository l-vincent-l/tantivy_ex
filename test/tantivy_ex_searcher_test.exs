defmodule TantivyEx.SearcherTest do
  use ExUnit.Case, async: true
  alias TantivyEx.{Schema, Index, IndexWriter, Searcher, Query, Facet}

  setup do
    schema =
      Schema.new()
      |> Schema.add_text_field("title", :text_stored)
      |> Schema.add_text_field("body", :text_stored)
      |> Schema.add_text_field("tags", :text_stored)
      |> Schema.add_u64_field("id", :indexed_stored)
      |> Schema.add_facet_field("category")

    {:ok, index} = Index.create_in_ram(schema)
    {:ok, writer} = IndexWriter.new(index, 50_000_000)

    docs = [
      %{"id" => 1, "title" => "Hello World", "body" => "A greeting", "category" => "/lang/en"},
      %{"id" => 2, "title" => "Goodbye", "body" => "Hello again", "category" => "/lang/en"},
      %{"id" => 3, "title" => "Bonjour", "body" => "Salut", "category" => "/lang/fr"}
    ]

    Enum.each(docs, &(:ok = IndexWriter.add_document(writer, &1)))
    :ok = IndexWriter.commit(writer)
    {:ok, searcher} = Searcher.new(index)

    %{schema: schema, index: index, writer: writer, searcher: searcher}
  end

  defp ids(results), do: results |> Enum.map(& &1["id"]) |> Enum.sort()

  describe "search/4 with a query string" do
    test "parses the string over the indexed text fields", %{searcher: searcher} do
      # "Hello" is in the title of 1 and the body of 2, lowercased by the tokenizer
      assert {:ok, results} = Searcher.search(searcher, "hello", 10)
      assert ids(results) == [1, 2]

      assert {:ok, results} = Searcher.search(searcher, "title:hello", 10)
      assert ids(results) == [1]

      assert {:ok, []} = Searcher.search(searcher, "missing", 10)
    end

    test "finds phrases and terms of a field", %{searcher: searcher} do
      # Text fields are indexed with positions
      assert {:ok, results} = Searcher.search(searcher, ~s("hello world"), 10)
      assert ids(results) == [1]

      assert {:ok, results} = Searcher.search(searcher, "body:hello", 10)
      assert ids(results) == [2]
    end

    test "finds phrases in a field with its own tokenizer" do
      schema = Schema.add_text_field_with_tokenizer(Schema.new(), "text", :text_stored, "en_stem")
      {:ok, index} = Index.create_in_ram(schema)
      {:ok, writer} = IndexWriter.new(index, 50_000_000)
      :ok = IndexWriter.add_document(writer, %{"text" => "The runners were running"})
      :ok = IndexWriter.commit(writer)
      {:ok, searcher} = Searcher.new(index)

      {:ok, query} = Query.phrase(schema, "text", ["were", "run"])
      assert {:ok, [_]} = Searcher.search(searcher, query, 10)
    end

    test "returns an error on an invalid query", %{searcher: searcher} do
      assert {:error, reason} = Searcher.search(searcher, "unknown_field:hello", 10)
      assert reason =~ "Failed to parse query"
    end
  end

  describe "errors" do
    test "a failure is an error tuple with the message", %{searcher: searcher} do
      assert {:error, "Failed to parse query: " <> _} = Searcher.search(searcher, "nope:x")
    end

    test "an argument of the wrong type raises" do
      assert_raise ArgumentError, fn -> Searcher.new("not an index") end
    end
  end

  describe "search_with_parser/5" do
    test "searches the given default fields", %{searcher: searcher} do
      assert {:ok, results} = Searcher.search_with_parser(searcher, ["body"], "hello")
      assert ids(results) == [2]
    end

    test "returns an error on an unknown default field", %{searcher: searcher} do
      assert {:error, reason} = Searcher.search_with_parser(searcher, ["nope"], "hello")
      assert reason =~ "Field not found"
    end
  end

  describe "results" do
    test "have a score and the stored fields, without doc_id", %{searcher: searcher} do
      assert {:ok, [result]} = Searcher.search(searcher, "bonjour", 10)
      assert is_float(result["score"])
      assert result["title"] == "Bonjour"
      assert result["category"] == "/lang/fr"
      refute Map.has_key?(result, "doc_id")

      assert {:ok, [result]} = Searcher.search(searcher, "bonjour", 10, false)
      assert Map.keys(result) == ["score"]
    end

    test "give every value of a multi-valued field", %{
      index: index,
      schema: schema,
      writer: writer
    } do
      doc = %{"id" => 4, "title" => "Tagged", "tags" => ["elixir", "search"]}
      :ok = IndexWriter.add_document(writer, doc)
      :ok = IndexWriter.commit(writer)
      {:ok, searcher} = Searcher.new(index)

      {:ok, query} = Query.term(schema, "tags", "search")
      assert {:ok, [result]} = Searcher.search(searcher, query, 10)
      assert result["tags"] == ["elixir", "search"]
      # A field with one value gives that value
      assert result["title"] == "Tagged"
    end

    test "give JSON objects as maps, and IPv4 addresses as such" do
      schema =
        Schema.new()
        |> Schema.add_json_field("meta", :text_stored)
        |> Schema.add_ip_addr_field("ip", :indexed_stored)

      {:ok, index} = Index.create_in_ram(schema)
      {:ok, writer} = IndexWriter.new(index, 50_000_000)
      meta = %{"color" => "red", "size" => 3, "tags" => ["a", "b"]}
      :ok = IndexWriter.add_document(writer, %{"meta" => meta, "ip" => "192.168.1.10"})
      :ok = IndexWriter.commit(writer)
      {:ok, searcher} = Searcher.new(index)

      # JSON fields are indexed: their keys can be searched
      assert {:ok, [result]} = Searcher.search(searcher, "meta.color:red", 10)
      assert result["meta"] == meta
      assert result["ip"] == "192.168.1.10"
    end

    test "return the error of a failing search", %{schema: schema, searcher: searcher} do
      # "id" has no positions: a phrase query on it fails
      {:ok, query} = Query.phrase(schema, "id", ["1", "2"])
      assert {:error, _reason} = Searcher.search(searcher, query, 10)
    end
  end

  describe "facet collector" do
    test "counts the added facets", %{searcher: searcher} do
      {:ok, collector} = Facet.collector_for_field("category")
      assert :ok = Facet.add_facet(collector, "/lang")
      {:ok, query} = Query.all()

      assert {:ok, counts} = Facet.search(searcher, query, collector)
      assert counts != %{}
    end

    test "can be shared by concurrent processes", %{searcher: searcher} do
      {:ok, collector} = Facet.collector_for_field("category")
      {:ok, query} = Query.all()

      1..20
      |> Enum.map(fn i ->
        Task.async(fn ->
          :ok = Facet.add_facet(collector, "/lang/#{rem(i, 2)}")
          Facet.search(searcher, query, collector)
        end)
      end)
      |> Task.await_many()
      |> Enum.each(&assert {:ok, _} = &1)
    end
  end
end
