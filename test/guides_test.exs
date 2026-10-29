defmodule TantivyEx.GuidesTest do
  @moduledoc """
  Runs the Elixir examples of the guides, so that they stay in sync with the
  API.

  The blocks of a guide run in order, as in an IEx session: each one reuses
  the variables and aliases of the previous ones. A guide runs in a new
  temporary directory, where its relative paths are created. A block preceded
  by `<!-- skip-test -->` is not run (e.g. a `mix.exs` excerpt).

  A `# => value` line checks the value of the code before it, which must
  equal `value`, an Elixir term.
  """

  use ExUnit.Case, async: false

  @guides ~w(
    README.md
    docs/quick-start.md
  )

  for guide <- @guides do
    @external_resource guide

    test guide do
      run_guide(unquote(guide))
    end
  end

  defp run_guide(guide) do
    TantivyEx.Tokenizer.register_default_tokenizers()

    dir = Path.join(System.tmp_dir!(), "tantivy_ex_guide_#{System.unique_integer([:positive])}")
    File.mkdir_p!(dir)
    cwd = File.cwd!()
    File.cd!(dir)

    try do
      guide
      |> then(&Path.join(cwd, &1))
      |> File.read!()
      |> blocks()
      |> Enum.reduce({[], Code.env_for_eval([])}, fn {code, number}, {binding, env} ->
        eval_block(guide, number, code, binding, env)
      end)
    after
      File.cd!(cwd)
      File.rm_rf!(dir)
    end
  end

  # The Elixir blocks to run, with their number in the guide
  defp blocks(text) do
    ~r/(<!-- skip-test -->\s*)?```elixir\n(.*?)```/s
    |> Regex.scan(text)
    |> Enum.with_index(1)
    |> Enum.reject(fn {[_, skip, _], _} -> skip != "" end)
    |> Enum.map(fn {[_, _, code], number} -> {code, number} end)
  end

  # A block runs in chunks, each one ending at a `# => value` line
  defp eval_block(guide, number, code, binding, env) do
    code
    |> String.split(~r/^\s*# => (.*)$/m, include_captures: true)
    |> Enum.chunk_every(2)
    |> Enum.reduce({binding, env}, fn chunk, {binding, env} ->
      {value, binding, env} = eval_chunk(guide, number, hd(chunk), binding, env)

      with [_, expected_line] <- chunk do
        expected = expected_value(guide, number, expected_line)

        if value != expected do
          flunk("""
          #{guide}, Elixir block #{number}: #{String.trim(expected_line)}
          got: #{inspect(value)}
          """)
        end
      end

      {binding, env}
    end)
  end

  defp eval_chunk(guide, number, code, binding, env) do
    Code.eval_quoted_with_env(Code.string_to_quoted!(code), binding, env)
  rescue
    e ->
      first_line =
        code
        |> String.split("\n")
        |> Enum.find(
          "",
          &(String.trim(&1) != "" and not String.starts_with?(String.trim(&1), "#"))
        )

      flunk("""
      #{guide}, Elixir block #{number}: #{String.trim(first_line)}

      #{Exception.format(:error, e, __STACKTRACE__)}
      """)
  end

  defp expected_value(guide, number, line) do
    [_, expected] = Regex.run(~r/# => (.*)$/, String.trim(line))
    {value, _} = Code.eval_string(expected)
    value
  rescue
    _ -> flunk("#{guide}, Elixir block #{number}: `#{String.trim(line)}` is not an Elixir term")
  end
end
