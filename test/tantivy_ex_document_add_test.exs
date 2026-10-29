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

  describe "deletion by term" do
    setup do
      schema =
        Schema.new()
        |> Schema.add_text_field_with_tokenizer("sku", :text_stored, "raw")
        |> Schema.add_text_field_with_tokenizer("body", :text_stored, "en_stem")
        |> Schema.add_date_field("at", :indexed_stored)

      {:ok, index} = Index.create_in_ram(schema)
      {:ok, writer} = IndexWriter.new(index, 50_000_000)

      :ok =
        IndexWriter.add_document(writer, %{
          "sku" => "AB-12",
          "body" => "running dogs",
          "at" => "2024-01-15T10:30:00Z"
        })

      :ok = IndexWriter.add_document(writer, %{"sku" => "CD-34", "body" => "cats", "at" => 0})
      :ok = IndexWriter.commit(writer)
      %{schema: schema, index: index, writer: writer}
    end

    defp skus(index) do
      {:ok, searcher} = Searcher.new(index)
      {:ok, results} = Searcher.search(searcher, "*", 10)
      results |> Enum.map(& &1["sku"]) |> Enum.sort()
    end

    test "analyzes the value with the field's tokenizer", ctx do
      # A raw field keeps the capitals
      assert {:ok, :deleted} = Document.delete(ctx.writer, "sku", "AB-12", ctx.schema)
      :ok = IndexWriter.commit(ctx.writer)
      assert skus(ctx.index) == ["CD-34"]

      # A stemmed field matches the inflected words
      assert {:ok, :deleted} = Document.delete(ctx.writer, "body", "cat", ctx.schema)
      :ok = IndexWriter.commit(ctx.writer)
      assert skus(ctx.index) == []
    end

    test "parses dates as the documents do", ctx do
      assert {:ok, :deleted} =
               Document.delete(ctx.writer, "at", "2024-01-15T10:30:00Z", ctx.schema)

      :ok = IndexWriter.commit(ctx.writer)
      assert skus(ctx.index) == ["CD-34"]
    end

    test "update replaces the document, without a duplicate", ctx do
      updated = %{"sku" => "AB-12", "body" => "sleeping dogs", "at" => 0}
      assert {:ok, :updated} = Document.update(ctx.writer, "sku", "AB-12", updated, ctx.schema)
      :ok = IndexWriter.commit(ctx.writer)
      assert skus(ctx.index) == ["AB-12", "CD-34"]
    end

    test "a value without terms is an error", ctx do
      assert {:error, reason} = Document.delete(ctx.writer, "body", "  ", ctx.schema)
      assert reason =~ "has no terms"
    end
  end
end
