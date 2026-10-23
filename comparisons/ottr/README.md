# tarka, maplib and Lutra

Three implementations of [OTTR](https://ottr.xyz) templates, run on the same templates and the
same rows:

* [**Lutra**](https://gitlab.com/ottr/lutra/lutra), the reference implementation, in Java, from
  the authors of OTTR. It expands instance files and bOTTR maps.
* [**maplib**](https://github.com/DataTreehouse/maplib), in Rust on Polars, an in-memory
  knowledge graph: data frames are mapped into it, and the graph is then queried, updated,
  validated and reasoned over in place.
* **tarka**, in Rust on Polars, a triplifier: rows go in and RDF comes out, to a file or a
  store. Besides OTTR it runs TARQL queries, bOTTR maps and oxi-gen's command line.

On the mapping measured here, tarka wrote the RDF 1.8 to 1.9 times as fast as maplib, and 42 to
66 times as fast as Lutra. maplib held its graph in half the memory tarka's in-memory result
took. tarka's output is the same as Lutra's; maplib's is not, because it builds nested
instances that OTTR drops.

Measured on 23 October 2026 with Lutra 0.6.21 (its latest release, on Java 25), maplib
0.20.26 and tarka-rdf 0.1.1 from PyPI, on Polars 2.0.0, on one laptop (Intel Core i7-1185G7,
4 cores and 8 threads, 32 GB, Windows 11). Corrections are welcome.

## What each does

| | tarka | maplib | Lutra |
|---|---|---|---|
| Mapping languages | OTTR (stOTTR), TARQL (SPARQL `CONSTRUCT`), bOTTR maps, oxi-gen's command line | OTTR (stOTTR, or templates built in Python); `map_default`, which maps a frame with no template; JSON through Façade-X | OTTR (stOTTR, wOTTR), bOTTR maps, tabOTTR spreadsheets |
| Inputs | CSV, Parquet, Arrow IPC, Polars frames. bOTTR also reads SQL over CSV, RDF files and SPARQL endpoints | Polars frames; RDF files (including HDT and CIM/XML); JSON | Instance files; through bOTTR, SQL (JDBC, or H2 over CSV), SPARQL endpoints and RDF files; spreadsheets |
| Used from | A command line, Rust, or Python | Python | A command line, or Java |
| After mapping | Writes Turtle, N-Triples or N-Quads, or loads the output into a store over the SPARQL Graph Store Protocol | SPARQL `SELECT`, `CONSTRUCT` and `INSERT` on the graph in memory; writes N-Triples, Turtle, RDF/XML or HDT | Writes RDF, or the instances in another OTTR format; lints and documents template libraries |
| SHACL | Open source ([SHACL_Engine](https://github.com/pwin/SHACL_Engine)), and shapes made from the mapping | Not in the open-source release; its README offers it on request | None |
| Reasoning | None | Datalog, not in the open-source release | None |
| Scale | The command line streams CSV in batches | In memory: about 100M triples on 32 GB, by its README | In memory: 8.7 GB for a million rows here (below) |
| Licence | MIT or Apache-2.0 | Apache-2.0 for the core | LGPL-2.1 |

## The RDF each makes

`bench.py` maps the 8 rows of tarka's retail fixture with each tool, and `compare.py` compares
the graphs, blank nodes matched by structure. **tarka's graph is isomorphic to Lutra's**: 104
triples, every blank node in place. maplib's has 120. Its triples without blank nodes are the
same (22), and the rest differ in one way: OTTR drops an instance when a mandatory parameter's
argument is `none`, its blank nodes with it, and maplib does not do that in nested templates.
It makes the instance with what it has:

| Node | Lutra and tarka | maplib |
|---|---|---|
| `schema:PriceSpecification` | 6, each with a price | 8, 2 of them without a price (rows with no `unit_price`) |
| `ex:Shipment` | 4, each with a date | 8, 4 of them without `ex:shippedOn` |
| `schema:OrderItem` | 7, each with a line number | 8, 1 of them without `ex:lineNumber` |

That is 15 triples a row from maplib against 13 from the others.

`edge_cases.py` shows how each tool treats cells it cannot take as they are (Lutra through a
bOTTR map that declares `ex:`):

| Cell | Lutra | tarka 0.1.1 | maplib |
|---|---|---|---|
| A prefixed name (`ex:ada`) | Expanded: `<http://example.com/ada>` | Expanded | Kept as an IRI with the scheme `ex`: `<ex:ada>`, without a warning |
| `mailto:ada@example.com`, with no `mailto:` prefix | A full IRI | Dropped without a word: 0.1.1 wants the prefix declared | A full IRI |
| Text that is not an IRI (`n/a`), for an optional parameter | That instance is left out, the others made, `[ERROR] Error in argument.`, exit status 1 | The value is dropped without a word | The whole frame is refused (`validate_iris=True` by default), naming the column |
| A null for a mandatory parameter of the root template | OTTR's `none`: that instance is dropped, the others made | The same | The whole frame is refused |

tarka's [#14](https://github.com/pwin/tarka/pull/14) makes tarka do what Lutra does in the
second and third rows: it reads a `mailto:` cell as a full IRI, and leaves out a row with a
cell its parameter cannot take, makes the others and reports it (the command line exits with
status 1, and Python raises a `ValueError` unless `strict=False`).

maplib's stOTTR parser is narrower than Lutra's and tarka's in three ways that the retail
templates meet: it does not parse the `none` keyword; it takes `ottr:none` as an ordinary IRI,
and writes it into triples; and it takes `@prefix` only at the start of a document. Lutra
lints the retail library as one file, with its 25 `@prefix` lines throughout, and expands
both `none` and `ottr:none` to nothing. maplib also takes `xsd:decimal` arguments only from
`Decimal(38, 12)` columns, not floats. `data.py` shapes the input around all four, and every
tool gets that input.

## Performance

The retail templates, with `rt:OrderRow` as the root, over the retail orders repeated with new
order IRIs. tarka and maplib map a typed Polars frame; Lutra, which reads no frames, maps the
same rows from a CSV file through a bOTTR map, each value written in the form the others write
it (`2024-03-05T10:15:00Z`, `89.5`). tarka and maplib write N-Triples, Lutra Turtle.

| Rows | Task | tarka | maplib | Lutra |
|---|---|---|---|---|
| 200,000 | write a file | 4.4 s, 450 MB | 8.5 s, 732 MB | 289 s, 2.9 GB |
| 1,000,000 | write a file | 27.8 s, 2.0 GB | 50.8 s, 3.0 GB | 1,170 s, 8.7 GB |
| 200,000 | map into memory | 3.4 s, 1.4 GB | 7.8 s, 0.7 GB | |
| 1,000,000 | map into memory | 28.6 s, 6.2 GB | 40.9 s, 3.0 GB | |

Each tarka and maplib figure is the median of three runs, Lutra's is one run; every run was in
a new process, and the tools took turns. tarka's and maplib's time and memory are their Python
process's, with the frame loaded first (about 100 MB at 200,000 rows and 300 MB at a million);
Lutra's are its Java process's, from start to exit, reading the CSV (45 MB and 224 MB).

* **Writing a file**, tarka was 1.9 times as fast as maplib at 200,000 rows and 1.8 times at a
  million, and 66 and 42 times as fast as Lutra.
* **Into memory is not the same result.** tarka's `triplify` returns a frame of distinct
  triples as N-Triples text; maplib's `map` builds a store that SPARQL can query. maplib keeps
  its terms in columns, dictionary-encoded, which is why it needs half the memory. tarka was
  2.3 times as fast at 200,000 rows and 1.4 times at a million.
* **maplib does more work here.** It made 15% more triples, the incomplete nodes above. That
  does not explain the difference.
* **Lutra's time is almost all mapping.** It takes 2.7 s and 220 MB to start and load the
  library (its time for the 8-row fixture). From 200,000 rows to a million it took 4.0 times as
  long, where tarka took 6.3 times and maplib 6.0 times. At a million rows it reached its
  default heap limit, a quarter of the memory (8 GB here); more rows need a larger `-Xmx`.
* **tarka slows faster in memory.** Five times the rows took tarka's `triplify` 8.4 times as
  long, and maplib's `map` 5.3 times.
* **tarka's memory grows with the frame,** from 300 MB to 2.0 GB at a million rows, even when
  writing a file: it converts every column for every row before it makes any triples. Its
  command line streams CSV in batches; the frame route could do the same.
* **tarka's and Lutra's files repeat some triples.** An order's own triples come once for each
  of its lines: 15.5 million statements for 13 million distinct triples, in both (tarka's
  `dedup` sets a window). maplib's store holds each triple once.
* **The laptop throttled.** These runs came after minutes of Lutra at full load, with the
  processor at 1.8 GHz against its 3.0 GHz base. The same tarka and maplib runs that morning,
  before Lutra, took 27 to 40% less time (2.7 and 5.7 s, 17.4 and 33.2 s writing; 2.3 and
  5.7 s, 17.7 and 26.5 s in memory), and tarka's lead over maplib differed by at most 0.25
  (2.1, 1.9, 2.5 and 1.5 times, against 1.9, 1.8, 2.3 and 1.4 here). The raw results of both
  sessions are in `results/`.

## Which to use

* **Lutra**, to know what a template library means: it is the reference, and lints and
  documents libraries; for instance files, spreadsheets, and data that fits its heap.
* **maplib**, to keep the graph in Python and work on it: queries, updates, and its licensed
  SHACL and Datalog, with frames that already hold full IRIs and a graph that fits in memory.
* **tarka**, to make RDF from tables: as a command or a pipeline step, from OTTR, TARQL or bOTTR
  mappings, with the same output as Lutra, loaded into a store, or checked with open-source
  SHACL.

## Running it

From the root of a tarka checkout, in a fresh virtual environment, with Java 21 or later and
`LUTRA_JAR` set to the path of [Lutra's jar](https://gitlab.com/ottr/lutra/lutra) (without it,
Lutra is left out):

```sh
pip install -r comparisons/ottr/requirements.txt
python comparisons/ottr/bench.py                  # 200,000 and 1,000,000 rows: about 35 minutes
python comparisons/ottr/bench.py --rows 8000 --runs 1                      # a quick check
python comparisons/ottr/edge_cases.py
python comparisons/ottr/bench.py --summarize comparisons/ottr/results/2026-10-23-afternoon.jsonl
```

The inputs and results go to `target/comparisons/ottr` (`--out` to change it). Lutra runs once
for each size (`--lutra-runs`), and is stopped after an hour (`LUTRA_TIMEOUT`); a run that
fails is recorded as failed. The full run writes files of about 1.4 GB, one at a time, and
needs about 9 GB of memory.

| File | What it does |
|---|---|
| `data.py` | The templates, the typed frames, and the same rows as CSV with a bOTTR map for Lutra |
| `run.py` | One timed run of one tool, in its own process, printed as JSON. Lutra's triples are counted by loading its file into [HOLOS](https://github.com/pwin/triplestore) (`holosdb`), which holds each triple once |
| `bench.py` | The 8-row comparison, then the timed runs, in turn, and a table of medians |
| `compare.py` | Two RDF files as graphs: isomorphic or not, and the incomplete nodes |
| `edge_cases.py` | Prefixed names, `mailto:`, cells that are not IRIs, and nulls, in each tool |
| `results/` | Raw results, one JSON line per run: the morning's two tools, the afternoon's three. The afternoon's Lutra counts are the statements in its file, counted before `run.py` loaded the file into HOLOS; `run.py` now gives distinct triples (2.6 million at 200,000 rows, as tarka's `triplify` does) |
