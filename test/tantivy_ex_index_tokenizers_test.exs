defmodule TantivyEx.IndexTokenizersTest do
  # The tokenizers are registered globally
  use ExUnit.Case, async: false

  alias TantivyEx.{Schema, Index, IndexWriter, Searcher, Query, Tokenizer}

  @body "Les statuts de l'association. L'école ouvre à huit heures."

  setup_all do
    {:ok, _} = Tokenizer.register_text_analyzer("fr_folded", "simple", true, nil, "fr", 40, true)
    :ok
  end

  defp index_with_tokenizer(tokenizer) do
    schema =
      Schema.new()
      |> Schema.add_text_field("title", :text_stored)
      |> Schema.add_text_field_with_tokenizer("body", :text, tokenizer)

    {:ok, index} = Index.create_in_ram(schema)
    {:ok, writer} = IndexWriter.new(index, 50_000_000)
    :ok = IndexWriter.add_document(writer, %{"title" => "Notes", "body" => @body})
    :ok = IndexWriter.commit(writer)
    index
  end

  defp hits(index, query) do
    {:ok, searcher} = Searcher.new(index)
    {:ok, parser} = Query.parser(index, ["body"])
    {:ok, query} = Query.parse(parser, query)
    {:ok, hits} = Searcher.search(searcher, query, 10)
    length(hits)
  end

  test "an index uses a registered analyzer: stemming, accents ignored" do
    index = index_with_tokenizer("fr_folded")

    assert hits(index, "statut") == 1
    assert hits(index, "ecole") == 1
    assert hits(index, "Écoles") == 1
    assert hits(index, "vélo") == 0
  end

  test "the snippet marks the words found through the analyzer" do
    index = index_with_tokenizer("fr_folded")
    {:ok, searcher} = Searcher.new(index)
    {:ok, parser} = Query.parser(index, ["body"])
    {:ok, query} = Query.parse(parser, "statut ecole")

    {:ok, %{fragment: fragment, highlighted: highlighted}} =
      Searcher.snippet(searcher, query, "body", @body)

    assert for({start, stop} <- highlighted, do: binary_part(fragment, start, stop - start)) ==
             ["statuts", "école"]
  end

  test "registering the default tokenizers keeps Tantivy's default and raw" do
    Tokenizer.register_default_tokenizers()
    index = index_with_tokenizer("default")

    # lowercased, but neither stemmed nor folded
    assert hits(index, "STATUTS") == 1
    assert hits(index, "statut") == 0
    assert hits(index, "ecole") == 0
    assert Tokenizer.tokenize_text("raw", "Les statuts") == ["Les statuts"]
  end
end
