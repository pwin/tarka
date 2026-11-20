# 6. Templates

A TARQL query says, for one spreadsheet, how its rows become triples. An OTTR template says
what a *thing* looks like in the graph, once, under a name, and every mapping that makes such
things uses the name. "A bookshop is these twelve triples, a point, and a link to each title it
stocks" is written once, in `tt:Bookshop`, and a spreadsheet, a Parquet file, a hand-written instance and a database query
all make bookshops by instantiating it. The pattern is reviewed, type-checked and changed in
one place.

OTTR is the Reasonable Ontology Templates language ([ottr.xyz](https://ottr.xyz)). This
chapter teaches the parts of its stOTTR syntax the trail uses, which is most of them, through
the trail's own library, and then shows `tarka lint` checking a library before any data is
read.

## A template

```stottr
tt:Named [ ! ottr:IRI ?thing, ! owl:Class ?class, rdf:langString ?name ] :: {
    ottr:Triple(?thing, rdf:type, ?class),
    ottr:Triple(?thing, rdfs:label, ?name)
} .
```

A template has a **signature**, its name and parameters in square brackets, and a **pattern**
after `::`, the instances it expands to. `ottr:Triple(s, p, o)` is the one built-in template:
it expands to a triple. Everything else is built from it.

Each parameter has a **type** and may have **modifiers** and a **default**:

| Written | Means |
|---|---|
| `ottr:IRI ?thing` | an IRI |
| `xsd:decimal ?floorArea`, `xsd:gYear ?founded` … | a literal of that datatype |
| `rdf:langString ?name` | a literal with a language tag |
| `rdfs:Literal ?wkt` | any literal |
| `owl:Class ?class` | an IRI used as a class |
| `NEList<ottr:IRI> ?books` | a non-empty list of IRIs; `List<…>` may be empty |
| `?thing`, no type | anything |
| `? xsd:anyURI ?website` | **optional**: may be given `none` |
| `! ottr:IRI ?shop` | **non-blank**: may not be a blank node |
| `! owl:Class ?kind = sf:Point` | a **default**, used when the argument is `none` |

What `none` does is the heart of OTTR. An instance whose argument for a **mandatory** parameter
is `none` is not made at all, and neither is anything it would have expanded to. An optional
parameter may be `none`; when it is, the instances that use it get `none`, and for
`ottr:Triple`, whose parameters are all mandatory, that means the triple is not written. So
optional values need no special handling, as in TARQL, but here it is declared: the signature
says which values a thing may lack.

## The trail's library

The library is three files in `ottr/`, a module each, and tarka loads a directory as one
library: a template may be used in one file and defined in another.

```text
ottr/base.stottr    general patterns: tt:Geometry, tt:Named
ottr/trail.stottr   the trail's things: tt:Bookshop, tt:Stocks, tt:StockRecord, tt:Segment, tt:Connects
ottr/rows.stottr    row templates: tt:ShopStock, one row of a prepared table in
```

### General patterns

`tt:Geometry` is GeoSPARQL's feature-and-geometry pattern, with nothing of the trail in it:

```stottr
tt:Geometry [ ! ottr:IRI ?feature, ! ottr:IRI ?geometry, rdfs:Literal ?wkt, ! owl:Class ?kind = sf:Point ] :: {
    ottr:Triple(?feature, rdf:type, geo:Feature),
    ottr:Triple(?feature, geo:hasGeometry, ?geometry),
    ottr:Triple(?feature, geo:hasDefaultGeometry, ?geometry),
    ottr:Triple(?geometry, rdf:type, geo:Geometry),
    ottr:Triple(?geometry, rdf:type, ?kind),
    ottr:Triple(?geometry, geo:asWKT, ?wkt)
} .
```

Six triples, always together, always right: no mapping can give a feature a geometry and
forget `geo:hasDefaultGeometry`. `?kind` defaults to `sf:Point`, and a caller with a line says
`sf:LineString`. `?wkt` is an `rdfs:Literal` because OTTR has no type for WKT; the datatype
comes with the value ([Chapter 8](08-bottr.md) shows how).

### The trail's things

`tt:Bookshop` is the whole of a bookshop, built on the general patterns:

```stottr
tt:Bookshop [
    ! ottr:IRI ?shop,
    rdf:langString ?name,
    ! ottr:IRI ?town,
    xsd:gYear ?founded,
    xsd:decimal ?floorArea,
    xsd:integer ?staff,
    ! ottr:IRI ?specialism,
    xsd:boolean ?secondHand,
    xsd:boolean ?cafe,
    ? xsd:anyURI ?website,
    xsd:decimal ?lat,
    xsd:decimal ?long,
    ! ottr:IRI ?point,
    rdfs:Literal ?wkt,
    ? ! NEList<ottr:IRI> ?stocks
] :: {
    tt:Named(?shop, bs:Bookshop, ?name),
    ottr:Triple(?shop, bs:locatedIn, ?town),
    ottr:Triple(?shop, bs:founded, ?founded),
    ottr:Triple(?shop, bs:floorArea, ?floorArea),
    ottr:Triple(?shop, bs:staffCount, ?staff),
    ottr:Triple(?shop, bs:specialises, ?specialism),
    ottr:Triple(?shop, bs:sellsSecondHand, ?secondHand),
    ottr:Triple(?shop, bs:hasCafe, ?cafe),
    ottr:Triple(?shop, bs:website, ?website),
    ottr:Triple(?shop, wgs84:lat, ?lat),
    ottr:Triple(?shop, wgs84:long, ?long),
    tt:Geometry(?shop, ?point, ?wkt, none),
    cross | tt:Stocks(?shop, ++?stocks)
} .
```

Read the signature and you know the contract. Every parameter is mandatory except the website
and the titles stocked: a row without a town, a year or a point is not a bookshop, and makes
nothing, rather than a shop with holes in it. `tt:Geometry(?shop, ?point, ?wkt, none)` passes
`none` for `?kind`, so the default, `sf:Point`, applies. OTTR wants every argument, in order,
so a default is asked for, not assumed.

The last line expands a list. `cross | tt:Stocks(?shop, ++?stocks)` makes one `tt:Stocks` for
each element of `?stocks`, with `?shop` the same in each; `++` marks the argument to expand:

```stottr
tt:Stocks [ ! ottr:IRI ?shop, ! ottr:IRI ?book ] :: {
    ottr:Triple(?shop, bs:stocks, ?book)
} .
```

`?stocks` is `? ! NEList<ottr:IRI>`: optional, and when given, a non-empty list of IRIs that
are not blank nodes, since `tt:Stocks` needs a non-blank `?book`. When it is `none`, the
expansion makes nothing.

### Expanding in step: zipMin

A shop's stock arrives, in the prepared tables, as one row with four parallel lists: the stock
records' IRIs, the books, the copies and the prices. `tt:ShopStock` turns such a row into
records:

```stottr
tt:ShopStock [
    ! ottr:IRI ?shop,
    ! NEList<ottr:IRI> ?records,
    ! NEList<ottr:IRI> ?books,
    NEList<xsd:integer> ?copies,
    NEList<xsd:decimal> ?prices
] :: {
    cross  | tt:Stocks(?shop, ++?books),
    zipMin | tt:StockRecord(++?records, ?shop, ++?books, ++?copies, ++?prices)
} .
```

`zipMin` takes the `++` lists in step, first elements together, then second elements, and stops
at the end of the shortest (`zipMax` goes on to the longest, padding with `none`). For
Bookbarrow, whose row holds two books, one row makes two records and two links:

```turtle
shop:bookbarrow bs:stocks book:scree , book:the-tarn .
stock:bookbarrow--scree a bs:StockRecord ;
	bs:atShop shop:bookbarrow ;
	bs:copies 12 ;
	bs:ofWork book:scree ;
	bs:shelfPrice 15.5 .
stock:bookbarrow--the-tarn a bs:StockRecord ;
	bs:atShop shop:bookbarrow ;
	bs:copies 14 ;
	bs:ofWork book:the-tarn ;
	bs:shelfPrice 14.5 .
```

TARQL needs two spreadsheets and two queries for this: `stock.csv` for the records and
`bookshops.csv`, split, for the links. The template does it from one row, and says so in its
signature.

## Checking a library: tarka lint

A template library is code, and it has bugs. `tarka lint` type-checks it with OTTR's type
system, and reports what Lutra's linter reports, and more, before any data is read:

```sh
tarka lint -l ottr/
```

```text
8 templates: 0 errors, 0 warnings
```

`ottr-mistakes/events.stottr` is a first draft of some event templates, with the mistakes first
drafts have. Lint them beside the library they build on:

```sh
tarka lint -l ottr/ -l ottr-mistakes/
```

```text
ottr-mistakes/events.stottr:16: error: ?property may be blank, but is given to the non-blank parameter ?predicate of ottr:Triple: mark ?property non-blank (!) [non-blank-flags]
ottr-mistakes/events.stottr:21: warning: parameter ?kind is not used in the pattern [unused-parameter]
ottr-mistakes/events.stottr:22: error: tt:Named takes 3 arguments, but is given 2 [arity]
ottr-mistakes/events.stottr:24: error: ?attendance (xsd:integer) does not fit the type ottr:IRI of argument 2 of tt:Stocks [type]
ottr-mistakes/events.stottr:24: error: ?attendance may be blank, but is given to the non-blank parameter ?book of tt:Stocks: mark ?attendance non-blank (!) [non-blank-flags]
ottr-mistakes/events.stottr:25: error: oops:Venue is not defined [undefined-template]
10 templates: 5 errors, 1 warning
tarka: the library has 5 errors
```

Each line names the file, the line, what is wrong, and the rule (in brackets), and most say what
to do. In order:

- **non-blank-flags.** `oops:Fact` uses `?property` as a predicate. A predicate cannot be a blank
  node, so `ottr:Triple` marks it `!`, and a parameter passed to it must be `!` too. Without the
  mark, a blank node could reach a predicate position, which no RDF can hold.
- **unused-parameter.** `?kind` is declared and never used: a column that would be read and
  thrown away.
- **arity.** `tt:Named` takes three arguments; the draft forgot the name.
- **type.** An attendance figure is an `xsd:integer`, and `tt:Stocks` wants an IRI for its book.
  This is the mistake a type system exists for: copied from the wrong line, it would put numbers
  where links belong, and only a reader of the output would notice.
- **undefined-template.** `oops:Venue` is used and never defined.

`tarka lint` exits with status 1 when there are errors, so it can stop a pipeline;
`--deny-warnings` fails on warnings too, and `--errors-only` hides them. The full list of rules
is in `tarka lint`'s section of the README. When this manual's own library was first written,
lint found three errors in it, all `non-blank-flags` on lists, and one `arity` error, a default
asked for without `none`; they are fixed above, and lint says so.

---

| [← 5. TARQL in depth](05-tarql-in-depth.md) | [7. OTTR over tables →](07-ottr-over-tables.md) |
|---|---|
