# Appendix B. Python reference

```python
import tarka_rdf          # pip install tarka-rdf
```

Paths are strings or `os.PathLike`. Formats are `"turtle"` (`"ttl"`), `"ntriples"` (`"nt"`) and
`"nquads"` (`"nq"`, which needs `graph`).

## Mapping

A compiled mapping: a TARQL query, or OTTR templates.

| | |
|---|---|
| `Mapping.tarql(query, name="query")` | a mapping from the text of a TARQL query |
| `Mapping.tarql_file(path)` | a mapping from a TARQL query file |
| `Mapping.ottr(library, templates, lists=None, languages=None)` | a mapping from OTTR templates: `library` is a list of stOTTR files or directories, `templates` the root templates to instantiate for every row, `lists` the separator of text cells for list parameters (`{"skills": ";"}`; list columns need none), `languages` the language of a column's strings (`{"name": "en"}`; needed for `rdf:langString`) |
| `.name` | the plan's name (the query, or the root templates) |
| `.prefixes` | the prefixes, as `[(prefix, namespace), …]`, for Turtle output |

### Mapping.triplify

```python
triplify(df, bind_empty_strings=False, strict=True) -> polars.DataFrame
```

The RDF of a data frame, as a frame of N-Triples terms with the columns `subject`, `predicate`
and `object`, without duplicates, in the order first made. A row with a value its parameter
cannot take is left out; with `strict=True` a `ValueError` lists the rows left out.

### Mapping.write

```python
write(df, path=None, format="turtle", graph=None, dedup=0, bind_empty_strings=False, strict=True) -> str | None
```

Writes the RDF of a data frame to `path`, or returns it as text when `path` is `None`. `dedup`
is the window of `--dedup` (0: duplicates removed within each row only). Rows left out are listed
in a `ValueError` after the rest is written, unless `strict=False`.

### Mapping.run_csv

```python
run_csv(input, output=None, format="turtle", graph=None, dedup=0, delimiter=",", quote='"',
        escape="\\", has_header=True, bind_empty_strings=False, split=None, strict=True) -> str | None
```

Maps a CSV file, read as the command reads it (oxi-gen's dialect), to `output`, or returns the
text. Each `split`, `(column, new_column, separator)`, repeats a row once per part, as `--split`
does.

### Mapping.shapes

```python
shapes(base="urn:tarka:shapes:") -> str
```

SHACL shapes for the RDF this mapping makes, as Turtle. With SHACL_Engine's package:
`shacl.Shapes.from_turtle(m.shapes()).validate_turtle(m.write(df))`.

## Functions

```python
tarka_rdf.expand(library, instances, output=None, format="turtle", graph=None) -> str | None
```

Expands OTTR instances (stOTTR files) with a library, as Lutra does; writes to `output`, or
returns the text.

```python
tarka_rdf.bottr(library, maps, output=None, format="turtle", graph=None, strict=True) -> str | None
```

Runs bOTTR instance maps (Turtle files) with a library; writes to `output`, or returns the text.
Instances that cannot be made are left out, and a `ValueError` lists them after the rest are
written, unless `strict=False`.

`tarka_rdf.__version__` is the package's version.

## Data frame cells

How each Polars type becomes a cell is in [Chapter 9](09-python-and-polars.md#how-polars-types-become-cells).
A TARQL query binds every cell as a string; an OTTR parameter reads a cell as its type. The
mapping runs without the global interpreter lock, on every core.

---

| [← Appendix A. Command reference](a-command-reference.md) | [Appendix C. Differences →](c-differences.md) |
|---|---|
