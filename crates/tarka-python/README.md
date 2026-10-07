# tarka-rdf

Python bindings for [tarka](https://github.com/pwin/tarka): turn Polars data frames, Parquet
and CSV files into RDF with TARQL queries (SPARQL `CONSTRUCT`, as TARQL and oxi-gen run them)
or OTTR templates (stOTTR), and run bOTTR mapping files. The engine is Rust; mapping runs
release the GIL and use every core.

```sh
pip install tarka-rdf
```

```python
import polars as pl
import tarka_rdf

m = tarka_rdf.Mapping.tarql_file("people.rq")                # a TARQL query
m = tarka_rdf.Mapping.ottr(["templates/"], ["ex:Person"])    # or OTTR templates: files or directories, root templates

df = pl.read_parquet("people.parquet")
triples = m.triplify(df)     # a DataFrame of N-Triples terms: subject, predicate, object
m.write(df, "people.ttl")    # Turtle, or format="ntriples" / "nquads" (with graph=…)
m.run_csv("people.csv", "people.ttl")                        # CSV in oxi-gen's dialect
shapes = m.shapes()          # SHACL shapes for what the mapping makes, as Turtle

tarka_rdf.expand(["templates/"], ["instances.stottr"], "people.ttl")    # OTTR instances, as Lutra does
tarka_rdf.bottr(["templates/"], ["people.bottr.ttl"], "people.ttl")     # bOTTR instance maps
```

Typed columns become typed literals for typed OTTR parameters (`xsd:integer`, `xsd:date` …),
and list columns feed OTTR list parameters. The package is called `tarka-rdf` and imported as
`tarka_rdf`, because `tarka` on PyPI is another project.

Licensed MIT OR Apache-2.0.
