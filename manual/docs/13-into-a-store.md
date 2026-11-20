# 13. Into a store

A file is a fine place for RDF to stop on its way somewhere. Usually that somewhere is a store,
where the graph is queried, validated with its reference data, reasoned over and served. tarka
can write the file, or skip it: `--post` streams the output straight into any store with the
SPARQL 1.1 Graph Store Protocol, as the triples are made.

## A store to load

[HOLOS](https://github.com/pwin/triplestore) serves the protocol at `/graph`, and SPARQL at
`/query`. Its server, from the HOLOS releases, starts in memory with one command:

```sh
holos-server --listen 127.0.0.1:7878
```

(`--store DIR` keeps the store on disk, in RocksDB; `--data FILE` loads files at start.) Any
store with the protocol will do: Oxigraph (`/store`), Fuseki (`/ds/data`), GraphDB.

## Loading

```sh
tarka run -q tarql/bookshops.rq -i data/bookshops.csv --post http://127.0.0.1:7878/graph \
    --graph https://example.org/bookshop-trail/graph/shops --stats
```

```text
33 records, 33 solutions, 588 triples made, 588 written
http://127.0.0.1:7878/graph?graph=https%3A%2F%2Fexample.org%2Fbookshop-trail%2Fgraph%2Fshops answered 201
```

`--graph` names the graph to load into, and without it the default graph. The protocol's
`POST` adds to a graph, so a second mapping can add its triples to the same one, and a third its
own graph:

```sh
tarka run -q tarql/stocks.rq -i data/bookshops.csv --post http://127.0.0.1:7878/graph \
    --graph https://example.org/bookshop-trail/graph/shops
tarka bottr -l ottr/ bottr/segments.bottr.ttl --post http://127.0.0.1:7878/graph \
    --graph https://example.org/bookshop-trail/graph/trail
```

```text
…/graph?graph=…%2Fgraph%2Fshops answered 204
…/graph?graph=…%2Fgraph%2Ftrail answered 201
```

201 is a graph created, 204 one added to. Ask the store what it holds:

```sh
curl -s http://127.0.0.1:7878/query -H "Accept: text/csv" --data-urlencode \
  "query=SELECT ?g (COUNT(*) AS ?triples) WHERE { GRAPH ?g { ?s ?p ?o } } GROUP BY ?g ORDER BY ?g"
```

```text
g,triples
https://example.org/bookshop-trail/graph/shops,683
https://example.org/bookshop-trail/graph/trail,396
```

`--replace` sends a `PUT` instead, which replaces the graph: load the bookshops again with it,
and `graph/shops` holds 588 triples, the titles stocked gone with the old contents. Use a graph
per source and `--replace`, and a reload is idempotent: run it twice, get the same store.

`--header "Name: value"` adds a request header (repeat it): credentials for a store that wants
them, or HOLOS's `X-Holos-Principal` and `X-Holos-Roles` behind its `--trust-forwarded-identity`.

### What streaming means

The triples are sent as N-Triples while they are made, so a load of millions is never held in
memory, and starts arriving before the run ends. A run that fails partway aborts the request
before its body ends, so the store takes **nothing** of it: no half-loaded graph. A store that
refuses the load is reported with its answer:

```text
tarka: http://127.0.0.1:7878/graph?graph=https%3A%2F%2Fexample.org%2Fpeople answered 403: …
```

Rows left out, and output that does not conform to `--validate`'s shapes, are not failures of the
run: the store has the rows that could be made, and tarka exits with status 1 or 3, so a
pipeline can still decide.

### Reading back

A store is also a source. [Chapter 8](08-bottr.md)'s connections map reads the segments from a
file; `bottr/connections-endpoint.bottr.ttl` asks the store instead, with an
`ottr:SPARQLEndpointSource` and the SPARQL 1.1 Protocol:

```turtle
[] a ottr:InstanceMap ;
   ottr:template tt:Connects ;
   ottr:source [ a ottr:SPARQLEndpointSource ; ottr:sourceURL "http://127.0.0.1:7878/query" ] ;
   ottr:query """
     PREFIX bs: <https://example.org/bookshop-trail/schema#>
     SELECT ?from ?to WHERE {
       GRAPH <https://example.org/bookshop-trail/graph/trail> {
         ?segment a bs:TrailSegment ; bs:segmentFrom ?from ; bs:segmentTo ?to .
       }
     }""" ;
   ottr:argumentMaps ( [ ottr:type ottr:IRI ] [ ottr:type ottr:IRI ] ) .
```

```text
33 rows, 33 instances made, 0 left out, 33 triples made
```

The same 33 links as from the file. tarka asks for JSON results first, and reads XML too.

## Large inputs

tarka reads CSV in batches and maps them on every core; nothing in a run is held for the whole
file except what `--validate` must see. The knobs:

| Option | Does |
|---|---|
| `--batch-size N` | records evaluated together (512). A larger batch means fewer SPARQL evaluations, and more memory per batch |
| `--jobs N` | batches worked on at once (0: one per CPU) |
| `--dedup [N]` | remove duplicate triples across rows within a window of N distinct triples (1000); without it, only within each row |
| `-g`, `--gzip` | compress the output file |
| `--test [N]` | the first N rows only (5), to try a mapping on a large file first |
| `--stats` | what was read and made, on standard error |

A row's duplicates are always removed (a shop's two `rdf:type` triples from two templates, say).
Duplicates across rows, such as the same order header repeated on each of an order's lines, stay
unless `--dedup` removes them; the window bounds the memory it takes. With `--dedup`, Turtle output
groups each window's triples by subject. A store removes duplicates anyway, so for `--post` the
window only saves bandwidth.

## How fast

tarka is checked against the tools it replaces on every change that touches their routes. On
200,000 rows of a retail orders table (8 cores), tarka 0.3.0 against TARQL 1.3 and Lutra, three
runs each in turn (Lutra once):

| Route | tarka | Reference | |
|---|---|---|---|
| TARQL query (`orders.rq`) | 5.66 / 4.83 / 4.76 s | TARQL 1.3: 18.7 / 22.7 / 17.6 s | about 3.9 times as fast |
| bOTTR map | 5.50 / 5.61 / 6.15 s | Lutra: 152.3 s | about 27 times as fast |

with the same triples: 2,775,000 distinct against TARQL's, 2,600,000 against Lutra's. Against
oxi-gen on the same data, tarka ran the TARQL query 1.7 times as fast and the OTTR templates 2.6
times as fast. Polars' `triplify`, in memory, is faster again for frames already loaded.

## Exit status

Every command uses the same codes, so a pipeline can tell what happened:

| Status | Means |
|---|---|
| 0 | the run is complete and, if validated, conforms |
| 1 | an error (no file written, or an earlier one kept), or rows or instances left out (the rest written), or lint errors |
| 2 | the command line is wrong |
| 3 | the output was written, and does not conform to the shapes |

---

| [← 12. The Ontology Quality Suite](12-the-ontology-quality-suite.md) | [14. A pipeline →](14-a-pipeline.md) |
|---|---|
