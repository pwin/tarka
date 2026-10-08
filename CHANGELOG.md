# Changelog

## Unreleased

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
