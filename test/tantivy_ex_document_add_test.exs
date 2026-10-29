defmodule TantivyExDocumentAddTest do
  use ExUnit.Case, async: true
  alias TantivyEx.{Schema, Index, IndexWriter, Document, Query, Searcher}

  test "document add and search" do
    # Create a schema
    schema =
      Schema.new()
      |> Schema.add_text_field("title", :text_stored)
      |> Schema.add_text_field("body", :text)

    # Create an index
    {:ok, index} = Index.create_in_ram(schema)
    {:ok, writer} = IndexWriter.new(index, 50_000_000)

    # Add a simple document
    doc = %{
      "title" => "Hello World",
      "body" => "This is a test document"
    }

    # Add the document and commit
    assert {:ok, _} = Document.add(writer, doc, schema)
    assert :ok = IndexWriter.commit(writer)

    # Search for the document
    {:ok, searcher} = Searcher.new(index)

    # Search by title as an exact term
    {:ok, query} = Query.term(schema, "title", "Hello World")
    {:ok, results} = Searcher.search(searcher, query, 10)

    # We should find one document
    assert length(results) == 1
    found_doc = hd(results)
    assert found_doc["title"] == "Hello World"
  end

  describe "invalid documents" do
    setup do
      schema =
        Schema.new()
        |> Schema.add_text_field("title", :text_stored)
        |> Schema.add_u64_field("count", :indexed_stored)
        |> Schema.add_i64_field("delta", :indexed_stored)
        |> Schema.add_json_field("meta", :stored)

      {:ok, index} = Index.create_in_ram(schema)
      {:ok, writer} = IndexWriter.new(index, 50_000_000)
      %{schema: schema, writer: writer}
    end

    test "are rejected by IndexWriter.add_document/2", %{writer: writer} do
      for {doc, message} <- [
            {%{"titel" => "Typo"}, "Unknown field: 'titel'"},
            {%{"title" => 42}, "Field 'title': Expected string value"},
            {%{"count" => "ten"}, "Field 'count': Expected numeric value"},
            {%{"count" => -1}, "Field 'count': Negative value not allowed"},
            {%{"delta" => 18_446_744_073_709_551_615}, "Field 'delta': Value too large"},
            {%{"meta" => "{not json"}, "Field 'meta': Invalid JSON"},
            {%{"meta" => "[1, 2]"}, "Field 'meta': Expected a map"},
            {%{"title" => ["ok", 1]}, "Field 'title': Expected string value"}
          ] do
        assert {:error, reason} = IndexWriter.add_document(writer, doc)
        assert reason =~ message
      end
    end

    test "are rejected by Document.add/3", %{writer: writer, schema: schema} do
      assert {:error, reason} = Document.add(writer, %{"count" => -1}, schema)
      assert reason =~ "count"
    end

    test "accept a JSON field as a map or a JSON string", %{writer: writer} do
      assert :ok = IndexWriter.add_document(writer, %{"meta" => %{"a" => 1}})
      assert :ok = IndexWriter.add_document(writer, %{"meta" => ~s({"a": 1})})
    end
  end
end
