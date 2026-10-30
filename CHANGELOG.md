# Changelog

## Unreleased

- `cargo build --release` in the repository builds a `tarka` command that starts without
  Python. It built the Python module too, whose dependencies then linked python3.dll into the
  command; the workspace's default members now leave the module out. The released binaries
  were never affected: they are built with `-p tarka-cli`.

## 0.2.1 (2026-10-29)

- `tarka lint` reports a parameter that may be blank given to a non-blank (`!`) parameter, as
  Lutra does (`non-blank-flags`, an error). `ottr:Triple`'s predicate is non-blank, so a
  parameter used as a predicate must be marked `!`; before, only Lutra said so.
- `-o FILE` is written all or nothing, for `run`, `expand` and `bottr`: under another name
  beside it, moved into place once the run is complete. A run that failed partway left a
  truncated `FILE`; now it leaves an earlier one as it was.

## 0.2.0 (2026-10-23)

- OTTR over columns (`tarka run -l`, `Mapping.ottr`) no longer drops instances silently. A
  row with a cell its parameter cannot take (`n/a` for an `ottr:IRI`) is left out and
  reported with every bad cell, the other rows are written, and tarka exits with status 1, as
  bOTTR already did. In Python, `triplify`, `write` and `run_csv` raise a `ValueError` listing
  the rows once the rest is written, unless `strict=False`. Before, the cell was taken as
  empty: a mandatory parameter dropped the instance and an optional one the triple, with
  nothing said. An empty cell is still OTTR's `none`.
- An IRI cell is read as bOTTR (and Lutra) reads one: a prefixed name with a declared prefix
  is expanded, and anything else is a full IRI. `mailto:…`, `urn:…` and `tag:…` cells no longer
  need a prefix declared, where `run -l` took only text containing `://` as a full IRI.
- Library: `tarka::run`, `tarka::run_envs`, `tarka_polars::run_frame` and
  `tarka_polars::triplify` take a callback for the rows left out, and `RunStats::left_out`
  counts them; `Conversion::apply` returns why a cell cannot be converted.
- Built with the Oxigraph crates' latest releases (spargebra 0.4.8, spareval 0.2.9,
  oxsdatatypes 0.2.4 and the rest of Oxigraph 0.5.13), which fix SPARQL bugs a TARQL query can
  meet: a signed number after an operand (`?a + 2 * 3`, `10 - 2 -3`) is grouped as SPARQL
  says, `SUBSTR` takes a start of 0 or less, `IF` infers its type, a `FILTER` in nested
  `OPTIONAL`s keeps its scope, and decimals round, multiply and print correctly. A query that
  met one of these now gives the right output, which can differ from before. Also ureq 3.5,
  and SHACL_Engine 0.3.4.
- `tarka run --shapes FILE` refuses a file that declares no shapes, before the run: validating
  against it would check nothing and report that the output conforms. `--allow-no-shapes`
  validates against it all the same. `tarka_shacl::count_shapes` counts a graph's shapes as
  SHACL_Engine finds them.

## 0.1.1 (2026-10-08)

- Python: `Mapping.triplify` is four to five times faster on large outputs (it no longer copies
  every triple to remove duplicates, and builds its columns in parallel).
- `examples/python/retail.py`: OTTR and TARQL over 200,000 typed rows, and a bOTTR map.

## 0.1.0 (2026-10-07)

The first release.

### Mappings
- TARQL queries, as oxi-gen runs them: the same RDF on oxi-gen's fixtures and this
  repository's, `BOUND(?column)` aside (true when the cell has a value, as in TARQL).
- OTTR templates (stOTTR) used as modules: libraries over many files, any template as the
  root, several roots per row; the output matches Lutra's. `tarka expand` expands instances.
- bOTTR mapping files (`tarka bottr`): instance maps over CSV (H2's `CSVREAD`), RDF files and
  SPARQL endpoints, with every argument map setting; the output matches Lutra's.
- `tarka lint`: OTTR type checking and the checks Lutra's linter makes, and more.

### Inputs and outputs
- CSV in oxi-gen's dialect; Parquet and Arrow IPC through Polars, typed columns becoming typed
  literals and list columns OTTR lists.
- N-Triples, Turtle and N-Quads; `--post` streams the output into HOLOS or any store with the
  SPARQL Graph Store Protocol.
- `tarka shapes` writes SHACL shapes for what a mapping makes; `--validate`, `--shapes` and
  `--report` check the output with SHACL_Engine.

### Python
- `tarka-rdf` on PyPI, imported as `tarka_rdf`: mappings over Polars frames and CSV, OTTR
  instances, bOTTR maps and shapes. One wheel per platform for CPython 3.10 on.

### Scope
- RDF 1.1 and SPARQL 1.1: RDF 1.2 input is refused, and the build turns no RDF 1.2 features on.
