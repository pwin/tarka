# Appendix A. Command reference

`tarka <command> [options]`. Options and file names may come in any order. Every command that
writes RDF takes the output options below.

## tarka run

Maps a table to RDF with a TARQL query (`-q`) or OTTR templates (`-l … -T …`).

| Option | Does |
|---|---|
| `-q`, `--query FILE` | a TARQL query (a SPARQL `CONSTRUCT`) |
| `-l`, `--library PATH` | an OTTR template file or directory (repeat for more) |
| `-T`, `--template NAME` | a root template, by IRI or prefixed name; its parameters are columns (repeat for several roots per row) |
| `--list COLUMN SEPARATOR` | the text cells of a list-typed parameter are split on `SEPARATOR` (repeat per column) |
| `-i`, `--input FILE` | the input: CSV, or Parquet (`.parquet`) or Arrow IPC (`.arrow`, `.ipc`, `.feather`); default: the query's `FROM`, else standard input |
| `-d`, `--delimiter C` | the field delimiter (`,`) |
| `-t`, `--tab` | fields are tab-separated |
| `-p`, `--escape-char C` | the escape character inside quoted fields (`\`) |
| `--quote-char C` | the quote character (`"`) |
| `-n`, `--normalize` | upper-case the column names |
| `-H`, `--no-header-row` | no header row: the columns are `a`…`z`, then `A`…`Z` |
| `--split ORIGINAL SPLIT DELIMITER` | repeat each row once per part of column `ORIGINAL` split on `DELIMITER`, the part in column `SPLIT` |
| `--bind-empty-strings` | bind empty cells as `""` instead of leaving them unbound |
| `--test [N]` | read only the first N rows (5) |
| `--validate` | validate the output against the mapping's own shapes |
| `--shapes FILE` | validate against the SHACL shapes in `FILE` (repeat; with `--validate`, as well) |
| `--report FILE` | write the validation report to `FILE`, in Turtle |
| `--allow-no-shapes` | validate against a `--shapes` file that declares no shapes, instead of refusing it |
| `--batch-size N` | records evaluated together (512) |
| `--jobs N` | batches worked on at once (0: one per CPU) |
| `--stats` | print what was read and made to standard error |

## tarka expand

Expands OTTR instances (stOTTR files) to RDF, as Lutra does.

```text
tarka expand -l LIBRARY [OPTIONS] INSTANCES...
```

## tarka bottr

Runs bOTTR instance maps: OTTR templates over CSV (H2's `CSVREAD`), RDF files and SPARQL
endpoints.

```text
tarka bottr -l LIBRARY [OPTIONS] MAPS...
```

`--stats` prints rows, instances made and left out, and triples.

## The output options

For `run`, `expand` and `bottr`:

| Option | Does |
|---|---|
| `-o`, `--output FILE` | the output file, written all or nothing (default: standard output) |
| `--format F` | `turtle` (default), `ntriples` or `nquads`; not taken from the file name |
| `--ntriples` | the same as `--format ntriples` |
| `--graph IRI` | the named graph for N-Quads output; with `--post`, the graph to load into |
| `-g`, `--gzip` | compress the output file |
| `--dedup [N]` | remove duplicate triples across rows within a window of N distinct triples (1000); without it, within each row only |
| `--post ENDPOINT` | load the output into a SPARQL Graph Store Protocol endpoint instead of writing it (HOLOS: `http://127.0.0.1:7878/graph`) |
| `--replace` | replace the graph (`PUT`) instead of adding to it (`POST`) |
| `--header "Name: value"` | send this HTTP header with the load (repeat) |

## tarka shapes

Writes SHACL shapes for the RDF a mapping makes.

| Option | Does |
|---|---|
| `-q`, `--query FILE` | a TARQL query |
| `-l`, `--library PATH`, `-T`, `--template NAME`, `--list COLUMN SEPARATOR` | OTTR templates, as for `run` |
| `-o`, `--output FILE` | the output file (default: standard output) |
| `--base IRI` | the namespace of the shapes' IRIs (`urn:tarka:shapes:`) |

## tarka lint

Checks OTTR template libraries: undefined templates, arity, types, cycles, non-blank flags and
more.

| Option | Does |
|---|---|
| `-l`, `--library PATH` | a template file or directory (repeat) |
| `--errors-only` | report errors only |
| `--deny-warnings` | fail on warnings too |

## Exit status

| Status | Means |
|---|---|
| 0 | complete; conforming, if validated; a clean library, for `lint` |
| 1 | an error (the output not written, or an earlier file kept); rows or instances left out (the rest written); lint errors (or warnings, with `--deny-warnings`) |
| 2 | the command line is wrong |
| 3 | the output was written and does not conform to the shapes |

---

| [← 14. A pipeline](14-a-pipeline.md) | [Appendix B. Python reference →](b-python-reference.md) |
|---|---|
