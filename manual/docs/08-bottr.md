# 8. bOTTR

`tarka run -l` reads cells exactly as the template's parameters say, which is right when the
cells are already terms, and stuck when they are not: a spreadsheet says `yes` for `true`, a
value needs a datatype OTTR has no type for, names in one column are in several languages.
bOTTR is OTTR's answer. A bOTTR file is Turtle, and holds **instance maps**: each takes the rows of a
query, from a CSV file, an RDF file or a SPARQL endpoint, and makes each row an instance of a
template, with an **argument map** per column saying how that column's cell becomes a term.

bOTTR is OTTR's standard ([spec](https://spec.ottr.xyz/bOTTR/0.1/)), and Lutra its reference
implementation; `tarka bottr` runs the same files, and gives the same RDF.

## An instance map

`bottr/bookshops.bottr.ttl` makes the bookshops from the prepared table, through `tt:Bookshop`:

```turtle
@prefix ottr:  <http://ns.ottr.xyz/0.4/> .
@prefix tt:    <https://example.org/bookshop-trail/template#> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
@prefix geo:   <http://www.opengis.net/ont/geosparql#> .

# The prefixes the table's cells use: "shop:inkwell" is <https://example.org/bookshop-trail/shop-inkwell>
@prefix shop:  <https://example.org/bookshop-trail/shop-> .
@prefix place: <https://example.org/bookshop-trail/place-> .
@prefix genre: <https://example.org/bookshop-trail/genre-> .
@prefix geom:  <https://example.org/bookshop-trail/geom-> .
@prefix book:  <https://example.org/bookshop-trail/book-> .

[] a ottr:InstanceMap ;
   ottr:template tt:Bookshop ;
   ottr:source [ a ottr:H2Source ] ;
   ottr:query """SELECT shop, name, town, founded, floor_area, staff, specialism, second_hand, cafe,
                        website, lat, long, point, wkt, stocks
                 FROM CSVREAD('../data/prepared/bookshops.csv')""" ;
   ottr:argumentMaps (
     [ ottr:type ottr:IRI ]                                                              # shop
     [ ottr:languageTag "en" ]                                                           # name
     [ ottr:type ottr:IRI ]                                                              # town
     [ ottr:type xsd:gYear ]                                                             # founded
     [ ottr:type xsd:decimal ]                                                           # floor_area
     [ ottr:type xsd:integer ]                                                           # staff
     [ ottr:type ottr:IRI ]                                                              # specialism
     [ ottr:type xsd:boolean ; ottr:booleanTrue ( "yes" ) ; ottr:booleanFalse ( "no" ) ]  # second_hand
     [ ottr:type xsd:boolean ; ottr:booleanTrue ( "yes" ) ; ottr:booleanFalse ( "no" ) ]  # cafe
     [ ottr:type xsd:anyURI ]                                                            # website: empty is none
     [ ottr:type xsd:decimal ]                                                           # lat
     [ ottr:type xsd:decimal ]                                                           # long
     [ ottr:type ottr:IRI ]                                                              # point
     [ ottr:datatypeSep "^^" ]                                                           # wkt: "…^^geo:wktLiteral"
     [ ottr:type ( rdf:List ottr:IRI ) ; ottr:listSep ";" ]                              # stocks
   ) .
```

The query's columns and the argument maps are **positional**: the first column is the
template's first argument, read by the first map, and so on. Column names do not have to match
parameter names, which is the first thing `tarka run -l` could not do. The maps do the rest:

| Argument map | Turns | into |
|---|---|---|
| `ottr:type ottr:IRI` | `shop:bookbarrow` | `bt:shop-bookbarrow`, with the file's prefixes |
| `ottr:languageTag "en"` | `Bookbarrow` | `"Bookbarrow"@en` |
| `ottr:type xsd:gYear` | `1990` | `"1990"^^xsd:gYear` |
| `ottr:booleanTrue ( "yes" )` | `yes` | `true`; with `ottr:booleanFalse ( "no" )`, `no` is `false`, and anything else is an error |
| `ottr:datatypeSep "^^"` | `<…> POINT(-3.13089 54.60668)^^geo:wktLiteral` | the WKT literal, with the datatype written after `^^` in the cell |
| `ottr:type ( rdf:List ottr:IRI ) ; ottr:listSep ";"` | `book:scree;book:the-tarn` | a list of two IRIs |

An empty cell (`website`, for six shops) is `none`, so the optional website is left out.

```sh
tarka bottr -l ottr/ bottr/bookshops.bottr.ttl -o out/bookshops-bottr.ttl --stats
```

```text
33 rows, 33 instances made, 0 left out, 683 triples made
```

683 triples: the 588 of Chapter 4's TARQL mapping, and the 95 `bs:stocks` links. Lutra makes
the same 683 from the same file, triple for triple, and they are the course's: three routes,
one graph.

One difference of form, not of value: a bOTTR map writes a typed literal as the cell has it, so
the floor area is `"80.0"^^xsd:decimal`, as Lutra writes it and as the course has it, where
Chapter 4's TARQL cast wrote the canonical `"80"`. (`tarka run -l` canonicalises too.) The values
are equal, and a store or a SPARQL `=` treats them as one; a comparison of strings does not, so
compare by value ([Chapter 11](11-sparql-checks.md)). The value must be valid for its type: a
bOTTR cell that is not leaves its instance out.

### Datatypes OTTR does not have

`geo:wktLiteral` is not an OTTR type, so no parameter or `ottr:type` can name it. The bOTTR way
is `ottr:datatypeSep`: the cell carries its datatype, after a separator, as a full IRI or a
prefixed name with the file's prefixes, and the literal goes to an `rdfs:Literal` parameter.
`python/prepare.py` writes the WKT cells that way. Lutra accepts it with a warning that the
datatype is unusual; tarka accepts it quietly. Writing `ottr:type geo:wktLiteral` instead gets a
warning from tarka, and an error from Lutra, and from both, plain strings.

## The query: H2's CSVREAD

The source is `ottr:H2Source`, H2 being the Java database Lutra runs the query in. tarka does
not embed a database: it reads the part of H2's SQL that reads a CSV file,

```sql
SELECT [DISTINCT] columns FROM CSVREAD('file.csv'[, columns[, options]])
```

where a column is a name, `*`, a string constant or `NULL`, and refuses anything else (`WHERE`,
functions, joins) with a message. Join and compute in the table instead, as
[Chapter 9](09-python-and-polars.md) does with Polars. The file is read in H2's dialect, which
differs from TARQL's: an unquoted field is trimmed, and an empty one is `NULL`; `""` is an
empty string. A relative path is found from the bOTTR file's directory (H2 itself would use the
working directory), and `@@THIS_DIR@@` works as in Lutra.

One trap: a column named after an SQL keyword. The segments table first had columns `from` and
`to`, and tarka, like H2, refused them:

```text
tarka: bottr/segments.bottr.ttl: instance map 1 (tt:Segment): tarka reads H2 queries of the form SELECT [DISTINCT] columns FROM CSVREAD('file.csv'[, columns[, options]]): expected a column, not from
```

They are `from_shop` and `to_shop` now.

## The segments

`bottr/segments.bottr.ttl` makes the 33 trail segments through `tt:Segment`, from a table in
which Polars has already joined each segment to the shops at its ends for its label ("The
Inkwell to Marginalia") and its line:

```sh
tarka bottr -l ottr/ bottr/segments.bottr.ttl -o out/segments.ttl --stats
```

```text
33 rows, 33 instances made, 0 left out, 396 triples made
```

Twelve triples each: the segment, its label, ends, distance and waymarking, and a
`tt:Geometry` with `sf:LineString` for `?kind`.

## An RDF source: links from the graph already made

The ontology also says which shops connect to which (`bs:connectsTo`), and those links follow
from the segments: a segment from A to B means A connects to B. The segments are already RDF, so
the links are a SPARQL query over them, and an **RDF file source** runs it
(`bottr/connections.bottr.ttl`):

```turtle
[] a ottr:InstanceMap ;
   ottr:template tt:Connects ;
   ottr:source [ a ottr:RDFFileSource ; ottr:sourceURL "../out/segments.ttl" ] ;
   ottr:query """
     PREFIX bs: <https://example.org/bookshop-trail/schema#>
     SELECT ?from ?to WHERE {
       ?segment a bs:TrailSegment ; bs:segmentFrom ?from ; bs:segmentTo ?to .
     }""" ;
   ottr:argumentMaps ( [ ottr:type ottr:IRI ] [ ottr:type ottr:IRI ] ) .
```

```sh
tarka bottr -l ottr/ bottr/connections.bottr.ttl -o out/connections.ttl --stats
```

```text
33 rows, 33 instances made, 0 left out, 33 triples made
```

The files are loaded into one dataset and queried with tarka's SPARQL engine (HOLOS's, with the
same functions as a TARQL query has); a term whose own type fits the argument map's is kept as
it is. This is graph-to-graph mapping: the second stage of a pipeline reading the first's output,
in the same language as the rest.

A **SPARQL endpoint source** (`ottr:SPARQLEndpointSource`) sends the query to a store instead,
with the SPARQL 1.1 Protocol; HOLOS serves one at `/query`, and the answer is read like an RDF
file source's. Point `ottr:sourceURL` at `http://127.0.0.1:7878/query` and the connections map
reads the segments from the store of [Chapter 13](13-into-a-store.md). A term in the answer that
is RDF 1.2 (a triple term, or a literal with a base direction) is refused, in JSON results and
XML results alike.

## What else an argument map can do

| Setting | Does |
|---|---|
| `ottr:type T` | reads the cell as type `T`: an IRI, a typed literal, a list type `( rdf:List T )` |
| `ottr:languageTag "en"` | a language-tagged string |
| `ottr:languageTagSep "@"` | the tag is in the cell: `Bookbarrow@en` |
| `ottr:datatypeSep "^^"` | the datatype is in the cell |
| `ottr:nullValue ex:Unknown` | an empty cell becomes this term instead of `none` |
| `ottr:translationTable` | a table of entries (`ottr:inValue` to `ottr:outValue`), to map codes to terms |
| `ottr:booleanTrue ( … )`, `ottr:booleanFalse ( … )` | the words for true and false |
| `ottr:listSep`, `ottr:listStart`, `ottr:listEnd` | how a list is written in a cell: `(a,(b,c))` by default |
| `ottr:blankNodeFresh ( "new" )` | a cell `new` is a fresh blank node |
| `ottr:labelledBlankPrefix` | a cell `_:x` is the same blank node wherever it appears |

## When an instance cannot be made

As in Lutra, an instance whose arguments cannot be made, or do not fit the template's
parameters, is left out and reported with every bad argument of its row; the others are made,
and tarka exits with status 1. A `Yes` where the map says `yes`:

```text
tarka: left out: bottr/bookshops.bottr.ttl: instance map 1 (tt:Bookshop), row 7: argument 8 (second_hand): the value 'Yes' is not in the lexical space of its datatype http://www.w3.org/2001/XMLSchema#boolean
33 rows, 32 instances made, 1 left out, 662 triples made
tarka: 1 of 33 instances left out
```

Compare TARQL's `IF(?second_hand = "yes", true, …)`, which leaves the triple out and says
nothing. The map leaves the whole shop out and says why: stricter, because a bOTTR map declares
what each column is.

From Python: `tarka_rdf.bottr(["ottr/"], ["bottr/bookshops.bottr.ttl"], "out/bookshops.ttl")`.

---

| [← 7. OTTR over tables](07-ottr-over-tables.md) | [9. Python and Polars →](09-python-and-polars.md) |
|---|---|
