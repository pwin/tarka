# tarka

tarka turns CSV, Parquet files and Polars data frames into RDF. The mapping can be a **TARQL query** (a SPARQL `CONSTRUCT`
run once per row, as in [TARQL](https://tarql.github.io/) and
[oxi-gen](https://github.com/semanticarts/oxi-gen)) or a set of **OTTR templates**
([ottr.xyz](https://ottr.xyz), the stOTTR syntax). Both compile to one plan, so they behave
the same and run at the same speed.

```sh
tarka run -q people.rq -i people.csv -o people.ttl                          # TARQL
tarka run -l templates/ -T ex:Person -i people.csv -o people.ttl            # OTTR
tarka run -q people.rq -i people.parquet -o people.ttl                      # Parquet or Arrow IPC input
tarka expand -l templates/ instances.stottr -o people.ttl                   # OTTR instances, as Lutra does
```

* **TARQL, as written for oxi-gen.** `tarka run` takes oxi-gen's options (`-q -i -o --ntriples
  -d -t -p -H -n --split --dedup --test --bind-empty-strings --gzip`) and gives the same RDF,
  term for term, on oxi-gen's test fixtures and on this repository's own, apart from the
  differences listed below. The WHERE clause is evaluated by spareval, the engine oxi-gen and
  HOLOS use.
* **OTTR templates, used as modules.** Load a library from any number of files and directories
  (`-l` repeats), use any template in it as the root (`-T`), and give several roots to instantiate
  for every row (`-T` repeats). A template's parameters are read from the CSV columns of the same
  names, converted by their types: IRIs from prefixed names or full IRIs, `xsd:*` literals,
  lists split on a separator (`--list COLUMN SEPARATOR`). Mandatory parameters, defaults, blank
  nodes, `cross`, `zipMin` and `zipMax` follow OTTR; the output matches Lutra's.
* **Polars data frames.** Parquet and Arrow IPC files are read with Polars, and the Python module
  maps a `polars.DataFrame` to a frame of triples. Typed columns (numbers, dates, datetimes)
  become typed literals, and list columns become OTTR lists.
* **Faster than oxi-gen.** Rows are evaluated in batches on every core. On a 200,000-row file
  (the retail orders mapping, 8 cores), tarka ran the TARQL query 1.7 times as fast as oxi-gen,
  and the OTTR templates 2.6 times as fast.

tarka works with RDF 1.1 and SPARQL 1.1.

## Install

```sh
cargo install --path crates/tarka-cli        # Rust 1.90 or later; installs the `tarka` command
pip install ./crates/tarka-python            # the Python module (builds with maturin)
```

## TARQL

```sparql
PREFIX ex:     <http://example.com/ns#>
PREFIX schema: <https://schema.org/>
CONSTRUCT {
  ?person a schema:Person ; schema:name ?name ; schema:email ?email .
}
WHERE {
  BIND(tarql:expandPrefixedName(?id) AS ?person)
}
```

Each row binds the variables named after its columns; an empty cell leaves its variable
unbound. `?ROWNUM` is the row number, from 0. `tarql:expandPrefixedName` and
`tarql:expandPrefix` resolve against the query's prefixes, in oxi-gen's namespace
(`https://semanticarts.com/tarql/`, predeclared as `tarql:`) or TARQL's. A `FROM <file.csv>`
names the input when there is no `-i`.

## OTTR

```stottr
ex:Person [ ottr:IRI ?id, xsd:string ?name, ? xsd:string ?email, ? ottr:IRI ?manager ] :: {
    ottr:Triple(?id, rdf:type, schema:Person),
    ottr:Triple(?id, schema:name, ?name),
    ottr:Triple(?id, schema:email, ?email),
    ex:ReportsTo(?id, ?manager)
} .
```

`tarka run -l people.stottr -T ex:Person -i people.csv` reads the columns `id`, `name`, `email`
and `manager`. The type of a parameter says how a cell is read:

| Parameter type | A cell becomes |
|---|---|
| `ottr:IRI`, `owl:Class`, `owl:NamedIndividual` … | an IRI: a prefixed name (`ex:alice`) or a full IRI |
| `xsd:date`, `xsd:decimal` … | a typed literal, in canonical form if the text is a valid value |
| untyped, `xsd:string` | a plain string |
| `List<T>` | a list of `T`: a text cell is split on the separator given with `--list`; a list column needs none |
| a parameter named `ROWNUM` | the row number |

A root template whose WHERE clause is recorded in `tq:` annotations (as written by
[ottr2sparql](https://github.com/pwin/ottr2sparql)'s `decompose`) is evaluated with that
WHERE clause instead.

### Modular templates

```sh
# a published library, your own module on top of it, two roots per row
tarka run -l vendor/retail/ -l my-templates.stottr -T my:Customer -T my:Audit -i customers.csv
```

Templates may live in any file; a signature in one file can be defined in another. A root
template's mandatory parameters drop only its own instance, so roots combine freely.

## Data frames

`-i` takes a Parquet (`.parquet`) or Arrow IPC (`.arrow`, `.ipc`, `.feather`) file as well as
CSV. A frame's values become cells as follows, and the mapping then runs as it does over CSV:

| Polars type | A cell holds |
|---|---|
| `String`, `Categorical`, `Enum` | the string (empty or blank: no value) |
| integers, `Decimal`, `Boolean` | the value: `42`, `9.95`, `true` |
| `Float32`, `Float64` | the shortest form that reads back as the value; `INF`, `-INF`, `NaN` |
| `Date`, `Time` | `2024-03-05`, `10:15:00` |
| `Datetime` | `2024-03-05T10:15:00`; in UTC, ending in `Z`, when the column has a time zone |
| `Duration` | `PT90S` |
| `List` | a list, for an OTTR list parameter (a TARQL query cannot bind one) |
| null | no value |

A typed OTTR parameter reads a cell as its type, so an integer column feeding an `xsd:integer`
parameter gives `"42"^^xsd:integer`. TARQL binds every cell as a plain string, as it does for CSV.

## Python

```python
import polars as pl
import tarka

m = tarka.Mapping.tarql_file("people.rq")              # or tarka.Mapping.tarql(query_text)
m = tarka.Mapping.ottr(["templates/"], ["ex:Person"])  # library files or directories, root templates

df = pl.read_parquet("people.parquet")
triples = m.triplify(df)          # a DataFrame of N-Triples terms: subject, predicate, object
m.write(df, "people.ttl")         # or format="ntriples", or "nquads" with graph="http://…"
text = m.write(df)                # without a path, the RDF is returned as text
m.run_csv("people.csv", "people.ttl")                  # CSV in oxi-gen's dialect
tarka.expand(["templates/"], ["instances.stottr"], "people.ttl")
```

`lists={"skills": ";"}` gives the separator of a text column that feeds an OTTR list parameter.
The mapping runs without the GIL, on every core.

## As a library

```rust
use tarka::io::{CsvOptions, CsvSource, OutputOptions, RdfWriter};
use tarka::ottr::{CompileOptions, Library};

let lib = Library::load(&["templates/"])?;
let plan = tarka::ottr::compile(&lib, "ex:Person", &CompileOptions::default())?;
// or: let plan = tarka::tarql::parse_tarql(&query_text, "people")?;
let source = CsvSource::new(std::fs::File::open("people.csv")?, CsvOptions::default())?;
let columns = source.columns().to_vec();
let mut out = RdfWriter::create(None, &plan.prefixes, OutputOptions::default())?;
tarka::run(&plan, &columns, source, &mut out, &tarka::RunOptions::default())?;
```

## Where tarka differs from oxi-gen

* **`BOUND(?column)` is true when the cell has a value**, as in TARQL. oxi-gen substitutes cells
  into the query, after which `BOUND(?column)` is always false, and `FILTER(BOUND(?column))`
  aborts it.
* **Errors are messages, not crashes.** A query that calls a function spareval does not have
  (`xsd:date(…)`) is reported before any input is read; a malformed row stops the run with its
  row number.
* **Blank node labels are stable**: the same input gives the same output.

Typed values are written the way spareval writes them, as oxi-gen does: `"129.90"^^xsd:decimal`
becomes `"129.9"`, `"1"^^xsd:boolean` becomes `true`. The OTTR route does the same, so both forms
of a mapping give the same terms.

## Layout

| Crate | What it does |
|---|---|
| `tarka-core` | the plan, literal forms, and the shape executor |
| `tarka-tarql` | TARQL to plan; WHERE clauses evaluated with spareval |
| `tarka-ottr` | stOTTR parser, template libraries, template to plan |
| `tarka-io` | CSV input in oxi-gen's dialect; N-Triples, Turtle and N-Quads output |
| `tarka` | the row engine, and the library API |
| `tarka-polars` | Polars data frames, Parquet and Arrow IPC input |
| `tarka-cli` | the `tarka` command |
| `tarka-python` | the `tarka` Python module |

How it works is in [docs/DESIGN.md](docs/DESIGN.md).

## Tests

```sh
cargo test --workspace
OXI_GEN=path/to/oxi_gen LUTRA_JAR=path/to/lutra.jar cargo test --workspace   # also compare with them live

python -m venv .venv && .venv/bin/pip install maturin polars pytest rdflib
(cd crates/tarka-python && ../../.venv/bin/maturin develop) && .venv/bin/pytest crates/tarka-python/tests
```

Every mapping is checked against a reference output: oxi-gen's for TARQL, Lutra's for OTTR. The
retail suite writes one transformation of three complex CSV files both as TARQL and as a modular
OTTR library, and both must give oxi-gen's output exactly. The same mappings run over data frames
must give the same RDF as over CSV.

The fixtures in `tests/fixtures/oxigen` come from oxi-gen and stay under its Apache-2.0 licence
(see `tests/fixtures/oxigen/LICENSE`).

## Licence

Dual licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your option.
Copyright (c) 2026 pwin (Peter Winstanley).
