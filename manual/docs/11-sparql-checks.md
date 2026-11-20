# 11. SPARQL checks

SHACL validates nodes against shapes. Some questions are not about one node: do two
spreadsheets agree? Did the transformation make what the source data has, no more, no less? Can
the graph answer the questions it was built for? Those are SPARQL's, and HOLOS answers them:
`tools/sparql.py` loads tarka's output, with the reference data, into a HOLOS store (`holosdb`)
and runs a query.

## First, count

The first thing to look at after any run, and the first to compare between one run and the
next (`sparql/counts.rq`):

```sparql
SELECT ?class (COUNT(DISTINCT ?thing) AS ?things) WHERE {
  ?thing a ?class .
  FILTER(STRSTARTS(STR(?class), STR(bs:)))
}
GROUP BY ?class
ORDER BY ?class
```

```sh
python tools/sparql.py sparql/counts.rq out/bookshops.ttl out/stocks.ttl out/stock.ttl \
    out/events.ttl out/works.ttl out/segments.ttl out/connections.ttl --no-reference
```

```text
bs:Bookshop  33
bs:Event  59
bs:StockRecord  95
bs:TrailSegment  33
bs:Translation  6
bs:Work  74
6 rows
```

Every number is the spreadsheet's row count, and the six translations are the six rows with a
`translation_of`. A count that moves between runs, with no change to the data, is a bug; a
count that is wrong on the first run is the cheapest bug you will ever find.

## When every file is right and the set is wrong

Each spreadsheet can be internally perfect and the five can still disagree: a title a shop's row
lists that `stock.csv` has no record of, an event at a shop that does not exist, a shop in a town
the reference data does not know. No shape over one file can see it, and no shape over one node.
`sparql/cross-file-checks.rq` asks for each, as one `SELECT` whose every row is a fault:

```sparql
SELECT ?fault ?subject ?object WHERE {
  {
    # bookshops.csv says a shop stocks a title, but stock.csv has no record of it
    ?subject bs:stocks ?object .
    FILTER NOT EXISTS { ?record bs:atShop ?subject ; bs:ofWork ?object }
    BIND("stocked, but no stock record" AS ?fault)
  } UNION {
    # events.csv names a shop bookshops.csv does not have
    ?event bs:heldAt ?object .
    FILTER NOT EXISTS { ?object a bs:Bookshop }
    BIND(?event AS ?subject)
    BIND("an event at an unknown shop" AS ?fault)
  } UNION {
    # a shop in a place the reference data does not know as a settlement
    ?subject a bs:Bookshop ; bs:locatedIn ?object .
    FILTER NOT EXISTS { ?object a bs:Settlement }
    BIND("a shop in an unknown town" AS ?fault)
  }
  # … and the two other directions: a record the shop's row does not list, a title with no work
}
ORDER BY ?fault ?subject
```

Over the good spreadsheets' output, with the reference data:

```text
0 rows
```

Over the faulty bookshops instead:

```text
"a shop in an unknown town"  bt:shop-marginalia  bt:place-wigton
1 rows
```

The same fault s64 found in [Chapter 10](10-shapes-and-shacl.md), found again, more plainly.
"No rows, no faults" is easy to run in a pipeline and easy to read in a report.
[SemOps Manual §10.6](https://www.semantechs.co.uk/SemOps_Manual/10-ingest-and-transform.html)
makes the general case: the set of files is a thing in itself, and must be checked as one.

## Asking what shapes allow

The course's shapes let a shop say nothing about a cafe; the committee's spreadsheet asks every
shop. When the question is "does any shop fail to answer?", an `ASK` is the plainest form
(`sparql/every-shop-answers.rq`):

```sparql
ASK {
  ?shop a bs:Bookshop .
  FILTER (NOT EXISTS { ?shop bs:sellsSecondHand ?any } || NOT EXISTS { ?shop bs:hasCafe ?any })
}
```

```text
ASK: no      # the good spreadsheet
ASK: yes     # the faulty one: Castle Steps' "Yes"
```

`trail-extra.ttl` says the same as a shape. Write a rule as a shape when it is about one node
and should be checked as the file is written; as a query when it is about the set, or when the
answer you want is a list rather than a report.

## The round trip: is it the course's graph?

The strongest check is the simplest to state. The spreadsheets were written from the course's
data; mapping them back must give the course's data. `tools/compare.py` loads what tarka made
and the course's graph into one HOLOS store, as two named graphs, and counts three kinds of
difference, each a SPARQL query:

```sparql
# made, not in the course: a triple tarka made that the course does not have
SELECT ?s ?p ?o WHERE { GRAPH <urn:made> { ?s ?p ?o }
  FILTER NOT EXISTS { GRAPH <urn:course> { ?s ?p ?o2 } FILTER (same(?o, ?o2)) } }
```

and its mirror, *in the course, not made*, for a value the course gives a subject and property
tarka writes; and *subjects missed*, a thing of a class tarka makes that tarka did not make.
"Same" deserves care. tarka writes `"80"^^xsd:decimal` where the course has `"80.0"`: equal under
SPARQL's `=`, so the comparison uses `=` for literals. WKT literals are compared as geometries,
with `geof:sfEquals` from HOLOS's GeoSPARQL functions: the course writes `POINT(-2.36280 …)`,
tarka `POINT(-2.3628 …)`, and they are the same point.

Everything Part II's TARQL queries made:

```sh
python tools/compare.py out/bookshops.ttl out/stocks.ttl out/stock.ttl out/events.ttl out/works.ttl
```

```text
2472 triples made
made, not in the course: 0
in the course, not made: 0
subjects missed: 0
base direction (RDF 1.2), which tarka does not write: 6
    <…book-the-dark-sea-ar> <http://purl.org/dc/terms/title> "البحر المظلم"@ar--rtl
    …
```

Everything Part III's OTTR templates and bOTTR maps made, from the Parquet stock table:

```sh
python tools/compare.py out/bookshops-bottr.ttl out/shop-stock-parquet.ttl out/segments.ttl out/connections.ttl
```

```text
1587 triples made
made, not in the course: 0
in the course, not made: 0
subjects missed: 0
base direction (RDF 1.2), which tarka does not write: 0
```

Every triple the course has for the bookshops, the titles they stock, their stock, their events,
the works and the trail, and no other. The six counted apart are the three right-to-left titles
of [Chapter 5](05-tarql-in-depth.md), whose direction RDF 1.1 cannot express.

You will not usually have the target graph to compare with; this manual made its input from one
on purpose. But you often have *last month's* output, and the same comparison, last month's
graph against this month's, says exactly what a change to a mapping changed.

## Asking the graph its questions

A graph is built to answer questions. The SPARQL course's first, q01, *every bookshop on the
trail*, runs unchanged on tarka's output:

```sh
python tools/sparql.py reference/q01-every-bookshop-on-the-trail.rq out/bookshops.ttl --no-reference
```

```text
bt:shop-bookbarrow  "Bookbarrow"@en
bt:shop-borderprint  "Borderprint"@en
bt:shop-candlemas  "Candlemas Books"@en
…
33 rows
```

Keep a project's competency questions as `.rq` files beside its mappings and run them on every
build; an answer that changes is a finding. The
[SPARQL course](https://www.semantechs.co.uk/SPARQL_Course/) has seventy more for the trail.

## Running queries elsewhere

`tools/sparql.py` is a few lines of `holosdb`; the queries are plain SPARQL and run in any
store. HOLOS's command line runs them over files with no Python at all, and validates too:

```sh
holos query --data out/bookshops.ttl --data reference/03-places.ttl --query-file sparql/cross-file-checks.rq
holos validate --data out/bookshops.ttl --data reference/03-places.ttl --shapes reference/s64-the-whole-trail-in-one-shapes-graph.ttl
```

Against the server of [Chapter 13](13-into-a-store.md), any SPARQL client can.

---

| [← 10. Shapes and SHACL](10-shapes-and-shacl.md) | [12. The Ontology Quality Suite →](12-the-ontology-quality-suite.md) |
|---|---|
