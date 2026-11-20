defmodule TantivyEx.Document do
  @moduledoc """
  Document operations checked against a schema: validation, additions one by
  one or in batches, updates and deletions by term.

  A document is a map of field names to values. `validate/2` checks each
  field against the schema and converts the values that can be: a numeric
  string for a numeric field, a `DateTime` for a date field, a map for a
  JSON field. The other functions validate the documents before adding them.

      schema =
        TantivyEx.Schema.new()
        |> TantivyEx.Schema.add_u64_field("id", :indexed_stored)
        |> TantivyEx.Schema.add_text_field("title", :text_stored)

      {:ok, index} = TantivyEx.Index.create_in_ram(schema)
      {:ok, writer} = TantivyEx.IndexWriter.new(index)

      :ok = TantivyEx.Document.add(writer, %{"id" => "1", "title" => "Hello"}, schema)
      {:ok, 2} = TantivyEx.Document.add_batch(writer, [%{"id" => 2}, %{"id" => 3}], schema)
      :ok = TantivyEx.Document.update(writer, "id", 1, %{"id" => 1, "title" => "Hi"}, schema)
      :ok = TantivyEx.IndexWriter.commit(writer)

  Like `TantivyEx.IndexWriter.add_document/2`, the changes become visible to
  new searchers on the next commit.
  """

  alias TantivyEx.{Native, Schema, IndexWriter}

  @type document :: map()
  @type batch_errors :: [{non_neg_integer(), String.t()}]

  @doc """
  Validates a document against the schema, and converts its values to the
  types of their fields.

  An unknown field, or a value that does not fit its field, is an error.

  ## Examples

      iex> schema = TantivyEx.Schema.add_f64_field(TantivyEx.Schema.new(), "price", :fast_stored)
      iex> TantivyEx.Document.validate(%{"price" => "29.99"}, schema)
      {:ok, %{"price" => 29.99}}
      iex> TantivyEx.Document.validate(%{"prize" => 1.0}, schema)
      {:error, "Document validation failed: Unknown field: 'prize'"}
  """
  @spec validate(document(), Schema.t()) :: {:ok, document()} | {:error, String.t()}
  def validate(document, schema) when is_map(document) do
    case validate_fields(document, field_types(schema)) do
      {:ok, document} -> {:ok, document}
      {:error, reason} -> {:error, "Document validation failed: #{reason}"}
    end
  end

  @doc """
  Validates a list of documents.

  Returns the validated documents, or the errors of the invalid ones, with
  their position in the list.

  ## Examples

      iex> schema = TantivyEx.Schema.add_u64_field(TantivyEx.Schema.new(), "id", :indexed)
      iex> TantivyEx.Document.validate_batch([%{"id" => 1}, %{"id" => -1}], schema)
      {:error, [{1, "Field 'id': Invalid u64 value: -1"}]}
  """
  @spec validate_batch([document()], Schema.t()) :: {:ok, [document()]} | {:error, batch_errors()}
  def validate_batch(documents, schema) when is_list(documents) do
    case split_valid(documents, field_types(schema)) do
      {valid, []} -> {:ok, Enum.map(valid, &elem(&1, 1))}
      {_valid, errors} -> {:error, errors}
    end
  end

  @doc """
  Validates a document and adds it to the index.
  """
  @spec add(IndexWriter.t(), document(), Schema.t()) :: :ok | {:error, String.t()}
  def add(writer, document, schema) do
    with {:ok, document} <- validate(document, schema) do
      IndexWriter.add_document(writer, document)
    end
  end

  @doc """
  Validates a list of documents and adds them to the index.

  By default, the documents are added only if all of them are valid:
  otherwise, nothing is added and the errors are returned, with the position
  of each invalid document in the list. Returns the number of documents added.

  ## Options

  - `:continue_on_error` - add the valid documents and skip the others
    (default `false`). Returns `{:ok, added, errors}`.
  """
  @spec add_batch(IndexWriter.t(), [document()], Schema.t(), keyword() | map()) ::
          {:ok, non_neg_integer()}
          | {:ok, non_neg_integer(), batch_errors()}
          | {:error, batch_errors()}
  def add_batch(writer, documents, schema, options \\ []) when is_list(documents) do
    [continue_on_error: continue_on_error] =
      Keyword.validate!(Enum.to_list(options), continue_on_error: false)

    case split_valid(documents, field_types(schema)) do
      {valid, []} ->
        with {:ok, added, []} <- add_all(writer, valid, false), do: {:ok, added}

      {valid, errors} when continue_on_error ->
        {:ok, added, add_errors} = add_all(writer, valid, true)
        {:ok, added, Enum.sort(errors ++ add_errors)}

      {_valid, errors} ->
        {:error, errors}
    end
  end

  @doc """
  Replaces the documents whose field `term_field` has the value `term_value`
  with `document`.

  Tantivy has no in-place update: the matching documents are deleted, and
  the new one is added. The value is matched as by `delete/4`.
  """
  @spec update(IndexWriter.t(), String.t(), term(), document(), Schema.t()) ::
          :ok | {:error, String.t()}
  def update(writer, term_field, term_value, document, schema) do
    with {:ok, document} <- validate(document, schema),
         :ok <- delete(writer, term_field, term_value, schema) do
      IndexWriter.add_document(writer, document)
    end
  end

  @doc """
  Deletes the documents whose field `term_field` has the value `term_value`.

  A text value is analyzed with the field's tokenizer, as in a search: on a
  field with the `"default"` tokenizer, `"Hello World"` deletes the documents
  containing the phrase "hello world". Use a `"raw"` field for identifiers
  that must match exactly. The other values are parsed for the field's type.
  """
  @spec delete(IndexWriter.t(), String.t(), term(), Schema.t()) :: :ok | {:error, String.t()}
  def delete(writer, term_field, term_value, schema) do
    if Schema.field_exists?(schema, term_field) do
      Native.writer_delete_term(writer, term_field, term_value(term_value))
    else
      {:error, "Field '#{term_field}' not found in schema"}
    end
  end

  @doc """
  Builds a document from a JSON object (a map, or a string to decode), and
  validates it.

  With a `field_mapping` of paths in the object (`"author.name"`) to field
  names, the document takes these values. Without, the nested objects are
  flattened, their keys joined with dots.

  ## Examples

      iex> schema = TantivyEx.Schema.add_text_field(TantivyEx.Schema.new(), "author", :text_stored)
      iex> TantivyEx.Document.prepare_json(~s({"meta": {"author": "Ada"}}), schema, %{"meta.author" => "author"})
      {:ok, %{"author" => "Ada"}}
  """
  @spec prepare_json(map() | String.t(), Schema.t(), %{String.t() => String.t()}) ::
          {:ok, document()} | {:error, String.t()}
  def prepare_json(json, schema, field_mapping \\ %{})

  def prepare_json(json, schema, field_mapping) when is_binary(json) do
    case Jason.decode(json) do
      {:ok, object} when is_map(object) -> prepare_json(object, schema, field_mapping)
      {:ok, _} -> {:error, "JSON decode failed: not an object"}
      {:error, reason} -> {:error, "JSON decode failed: #{Exception.message(reason)}"}
    end
  end

  def prepare_json(object, schema, field_mapping) when is_map(object) do
    object
    |> extract_fields(field_mapping)
    |> validate(schema)
  end

  defp field_types(schema) do
    Map.new(Schema.get_field_names(schema), fn name ->
      {:ok, type} = Schema.get_field_type(schema, name)
      {name, type}
    end)
  end

  # The valid documents and the errors, each with its position in the list
  defp split_valid(documents, field_types) do
    {valid, errors} =
      documents
      |> Enum.with_index()
      |> Enum.reduce({[], []}, fn {document, index}, {valid, errors} ->
        case validate_fields(document, field_types) do
          {:ok, document} -> {[{index, document} | valid], errors}
          {:error, reason} -> {valid, [{index, reason} | errors]}
        end
      end)

    {Enum.reverse(valid), Enum.reverse(errors)}
  end

  # Adds the documents, stopping at the first error unless `continue`
  defp add_all(writer, indexed_documents, continue) do
    Enum.reduce_while(indexed_documents, {:ok, 0, []}, fn {index, document},
                                                          {:ok, added, errors} ->
      case IndexWriter.add_document(writer, document) do
        :ok -> {:cont, {:ok, added + 1, errors}}
        {:error, reason} when continue -> {:cont, {:ok, added, errors ++ [{index, reason}]}}
        {:error, reason} -> {:halt, {:error, [{index, reason}]}}
      end
    end)
  end

  defp validate_fields(document, field_types) do
    Enum.reduce_while(document, {:ok, %{}}, fn {name, value}, {:ok, acc} ->
      with {:ok, type} <- Map.fetch(field_types, name),
           {:ok, value} <- convert_values(value, type) do
        {:cont, {:ok, Map.put(acc, name, value)}}
      else
        :error -> {:halt, {:error, "Unknown field: '#{name}'"}}
        {:error, reason} -> {:halt, {:error, "Field '#{name}': #{reason}"}}
      end
    end)
  end

  # A list holds the values of a multi-valued field, except for a JSON field
  defp convert_values(values, type) when is_list(values) and type != "json" do
    Enum.reduce_while(values, {:ok, []}, fn value, {:ok, acc} ->
      case convert(value, type) do
        {:ok, value} -> {:cont, {:ok, [value | acc]}}
        error -> {:halt, error}
      end
    end)
    |> case do
      {:ok, values} -> {:ok, Enum.reverse(values)}
      error -> error
    end
  end

  defp convert_values(value, type), do: convert(value, type)

  defp convert(value, "text") when is_binary(value), do: {:ok, value}
  defp convert(value, "text"), do: {:ok, to_string(value)}

  defp convert(value, "u64") when is_integer(value) and value >= 0, do: {:ok, value}
  defp convert(value, "u64") when is_binary(value), do: parse_integer(value, "u64", 0)
  defp convert(value, "i64") when is_integer(value), do: {:ok, value}
  defp convert(value, "i64") when is_binary(value), do: parse_integer(value, "i64", nil)

  defp convert(value, "f64") when is_number(value), do: {:ok, value * 1.0}

  defp convert(value, "f64") when is_binary(value) do
    case Float.parse(value) do
      {float, ""} -> {:ok, float}
      _ -> invalid("f64", value)
    end
  end

  defp convert(value, "bool") when is_boolean(value), do: {:ok, value}
  defp convert("true", "bool"), do: {:ok, true}
  defp convert("false", "bool"), do: {:ok, false}
  defp convert(1, "bool"), do: {:ok, true}
  defp convert(0, "bool"), do: {:ok, false}

  defp convert(value, "date") when is_integer(value), do: {:ok, value}
  defp convert(%DateTime{} = value, "date"), do: {:ok, DateTime.to_iso8601(value)}

  defp convert(value, "date") when is_binary(value) do
    case DateTime.from_iso8601(value) do
      {:ok, _date, _offset} -> {:ok, value}
      {:error, _} -> parse_integer(value, "date", nil)
    end
  end

  defp convert("/" <> _ = value, "facet"), do: {:ok, value}
  defp convert(value, "facet") when is_binary(value), do: {:ok, "/" <> value}

  defp convert(value, "bytes") when is_binary(value) do
    case Base.decode64(value) do
      {:ok, _bytes} -> {:ok, value}
      :error -> {:error, "Invalid base64 bytes value: #{inspect(value)}"}
    end
  end

  defp convert(value, "json") when is_map(value) or is_list(value), do: {:ok, value}

  defp convert(value, "json") when is_binary(value) do
    case Jason.decode(value) do
      {:ok, _} -> {:ok, value}
      {:error, reason} -> {:error, "Invalid JSON value: #{Exception.message(reason)}"}
    end
  end

  defp convert(value, "ip_addr") when is_binary(value) do
    case :inet.parse_address(String.to_charlist(value)) do
      {:ok, _} -> {:ok, value}
      {:error, _} -> {:error, "Invalid IP address: #{inspect(value)}"}
    end
  end

  defp convert(value, type), do: invalid(type, value)

  defp parse_integer(string, type, min) do
    case Integer.parse(string) do
      {integer, ""} when is_nil(min) or integer >= min -> {:ok, integer}
      _ -> invalid(type, string)
    end
  end

  defp invalid(type, value), do: {:error, "Invalid #{type} value: #{inspect(value)}"}

  defp term_value(%DateTime{} = date), do: DateTime.to_iso8601(date)
  defp term_value(value), do: value

  defp extract_fields(object, mapping) when map_size(mapping) == 0, do: flatten(object, nil)

  defp extract_fields(object, mapping) do
    for {path, field} <- mapping,
        value = path |> String.split(".") |> Enum.reduce(object, &child/2),
        value != nil,
        into: %{},
        do: {field, value}
  end

  defp child(key, %{} = object), do: Map.get(object, key)
  defp child(_key, _value), do: nil

  defp flatten(object, prefix) do
    Enum.reduce(object, %{}, fn {key, value}, acc ->
      key = if prefix, do: "#{prefix}.#{key}", else: key

      if is_map(value),
        do: Map.merge(acc, flatten(value, key)),
        else: Map.put(acc, key, value)
    end)
  end
end
