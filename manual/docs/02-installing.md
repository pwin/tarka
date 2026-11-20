# 2. Installing

tarka comes two ways, and most people want both: the **`tarka` command**, for the shell and
for pipelines, and the **`tarka-rdf` Python package**, for notebooks and data-frame work. They
share one engine and give the same RDF.

## The command

Each [release](https://github.com/pwin/tarka/releases) carries the command, built and tested on
its platform, in an archive with the licences and the README:

| Platform | Archive |
|---|---|
| Linux, x86_64 | `tarka-0.3.1-x86_64-unknown-linux-gnu.tar.gz` |
| macOS, Apple Silicon | `tarka-0.3.1-aarch64-apple-darwin.tar.gz` |
| Windows, x86_64 | `tarka-0.3.1-x86_64-pc-windows-msvc.zip` |

Unpack it and put `tarka` (or `tarka.exe`) on your `PATH`. `SHA256SUMS`, beside the archives,
lets you check what you downloaded:

```sh
sha256sum -c SHA256SUMS --ignore-missing
tarka --version
```

```text
tarka 0.3.1
```

The command needs nothing else installed: no Java, no Python.

### From source

With Rust 1.90 or later and a clone of the repository:

```sh
cargo install --path crates/tarka-cli      # installs `tarka` into ~/.cargo/bin
cargo build --release -p tarka-cli         # or build it as target/release/tarka
```

A plain `cargo build --release` in the repository builds the command too, and leaves the Python
module out on purpose: built in the same run, the module's dependencies would link Python into
the command, which then would not start without it.

## The Python package

```sh
pip install tarka-rdf
```

The package is `tarka-rdf` on PyPI and is imported as `tarka_rdf`: the name `tarka` belongs to
another project. One wheel per platform serves every CPython from 3.10 on, and it needs
[Polars](https://pola.rs), which pip installs with it.

```python
import tarka_rdf
print(tarka_rdf.__version__)        # 0.3.1
```

## The companions

tarka makes RDF. The manual also checks it, and for that it uses three more packages, each of
which also stands on its own:

| Package | What it is | Used in |
|---|---|---|
| `holosdb` | [HOLOS](https://github.com/pwin/triplestore), the RDF 1.2 store and SPARQL 1.2 engine, as a Python library. tarka's own SPARQL engine is HOLOS's | Chapters 10 and 11: loading output, SPARQL checks, SHACL with reference data |
| `shacl` | [SHACL_Engine](https://github.com/pwin/SHACL_Engine), the SHACL validator tarka embeds for `--validate` | Chapter 10 |
| `ontology-quality-suite` | the Ontology Quality Suite: a registry of checks for ontologies, mappings and data | Chapter 12 |

```sh
pip install holosdb shacl ontology-quality-suite
```

`holosdb`, the suite and recent Polars need Python 3.11 or later. This edition was checked with
tarka-rdf 0.3.1, polars 2.1.0, holosdb 0.27.0, shacl 0.3.9 and ontology-quality-suite 0.27.0.

The store itself, `holos-server`, is a separate download from the
[HOLOS releases](https://github.com/pwin/triplestore/releases); it is needed only for
[Chapter 13](13-into-a-store.md), to load tarka's output over HTTP.

## The examples

Everything the manual runs is in the repository, under `manual/examples/bookshop-trail/`:

```text
bookshop-trail/
  reference/    the courses' ontology, reference data, the SHACL course's s64 shapes, q01
  data/         the committee's spreadsheets (CSV), a faulty copy, and the prepared tables
  tarql/        TARQL queries
  ottr/         the OTTR template library (three modules)
  ottr-mistakes/  a first draft of some templates, mistakes and all, for tarka lint
  instances/    OTTR instances written by hand, for tarka expand
  bottr/        bOTTR mapping files
  python/       Polars and tarka-rdf scripts
  shapes/       SHACL shapes of the manual's own
  sparql/       SPARQL checks
  tools/        the spreadsheets' generator, a graph comparer, a query runner
  check.py      runs every example in the manual, in order, and checks each result
```

[Appendix E](e-the-example-files.md) describes every file. To run the lot:

```sh
cd manual/examples/bookshop-trail
python check.py
```

`check.py` writes its output to `out/`. Set `TARKA_EXAMPLES_OUT` to put it somewhere else, on a
larger or faster drive, say; `out/` then becomes a link to that folder, so every command in the
manual still works as written:

```sh
TARKA_EXAMPLES_OUT=E:/tarka-manual/out python check.py
```

Every command in the following chapters is run from `bookshop-trail/`.

---

| [← 1. What tarka does](01-what-tarka-does.md) | [3. The Bookshop Trail →](03-the-bookshop-trail.md) |
|---|---|
