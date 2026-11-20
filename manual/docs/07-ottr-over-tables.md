# 7. OTTR over tables

Chapter 6 built a library. This chapter runs it, three ways: over the rows of a table with
`tarka run -l`, over instances written by hand with `tarka expand`, and, in
[Chapter 8](08-bottr.md), through bOTTR maps.

## A root template, and its columns

`tarka run -l LIBRARY -T TEMPLATE` instantiates the root template once per row. The rule that
connects them is short: **each parameter reads the column of the same name**, converted by the
parameter's type.

| Parameter type | A cell becomes |
|---|---|
| `ottr:IRI`, `owl:Class` … | an IRI: a prefixed name with a prefix the library declares (`shop:bookbarrow`), or else a full IRI (`https://…`, `mailto:…`, `urn:…`) |
| `xsd:decimal`, `xsd:date` … | a typed literal, in canonical form when the text is a valid value, and as written when it is not |
| untyped, `xsd:string` | a plain string |
| `List<T>`, `NEList<T>` | a list of `T`: a text cell split on the separator given with `--list`; a list column needs none |
| a parameter named `ROWNUM` | the row number, from 0 |

An empty cell is OTTR's `none`.

The prepared stock table has one row per shop, with columns named for `tt:ShopStock`'s
parameters:

```text
shop,records,books,copies,prices
shop:bookbarrow,stock:bookbarrow--scree;stock:bookbarrow--the-tarn,book:scree;book:the-tarn,12;14,15.50;14.50
```

The four lists are text, so each needs its separator:

```sh
tarka run -l ottr/ -T tt:ShopStock -i data/prepared/shop-stock.csv \
    --list records ";" --list books ";" --list copies ";" --list prices ";" \
    -o out/shop-stock-csv.ttl --stats
```

```text
33 records, 33 solutions, 570 triples made, 570 written
```

Forget a `--list`, and tarka says which:

```text
tarka: parameter ?records has a list type: give the separator of its text cells (--list records ';', or lists={"records": ";"} in Python)
```

Five hundred and seventy triples: 95 stock records of five triples each, and 95 `bs:stocks`
links. They are exactly what the two TARQL queries `stock.rq` and `stocks.rq` make from two
spreadsheets, from one table, and both are the course's graph
([Chapter 11](11-sparql-checks.md)).

## Parquet: lists that are lists

CSV has no lists, which is why the cells above are joined with `;`. Parquet has them, and
types: `data/prepared/shop-stock.parquet` holds the same table with list columns, integer copies
and prices as decimals with two places. Nothing needs saying about it:

```sh
tarka run -l ottr/ -T tt:ShopStock -i data/prepared/shop-stock.parquet -o out/shop-stock-parquet.ttl --stats
```

```text
33 records, 33 solutions, 570 triples made, 570 written
```

`-i` reads Parquet (`.parquet`) and Arrow IPC (`.arrow`, `.ipc`, `.feather`) as well as CSV,
through Polars. A list column goes to a list parameter as it is, and its items keep their
types: the prices `15.50` and `14.50`, decimals in Parquet, come out as `15.5` and `14.5`, the
same values in canonical form. [Chapter 9](09-python-and-polars.md) has the full table of how
Polars types become cells.

## When a cell cannot be what its parameter needs

A cell that cannot become its parameter's kind of term leaves its row out. The rest are made,
and tarka says which rows, why, and exits with status 1:

```text
tarka: left out: row 3: column shop: 'Border Print' is not an IRI
3 records, 2 solutions, 30 triples made, 30 written
tarka: 1 of 3 rows left out
```

A shop called `Border Print`, with a space, cannot be an IRI, so the row could only be made
wrong; it is left out, and every bad cell in it is listed, not just the first. This is bOTTR's
rule, and Lutra's.

A typed literal is different. A cell `seven` for an `xsd:integer` parameter becomes
`"seven"^^xsd:integer`, kept as written, which is what OTTR says, and SHACL validation is how
such cells are found ([Chapter 10](10-shapes-and-shacl.md)).

## What a table must say, and what it cannot

`tarka run -l` is exact: it reads what is in the cells, as the parameters' types say, and
nothing more. That has consequences worth knowing before they surprise you. The bookshops show
them all. Run `tt:Bookshop` over the prepared bookshops table:

```sh
tarka run -l ottr/ -T tt:Bookshop -i data/prepared/bookshops.csv --list stocks ";" --test 2 --stats
```

```text
tarka: ?name is an rdf:langString, and a cell has no language tag: give the column's language (--lang name TAG, or languages={"name": TAG} in Python), or map the table with bOTTR and ottr:languageTag
```

**A language tag comes from `--lang`, not from a cell.** A cell holds text, and an
`rdf:langString` parameter needs a tag as well, so tarka refuses the template before it reads a
row. Every shop's name is English, so one tag serves the whole column: `--lang name en` makes
`"Bookbarrow"@en`. (`--lang` also tags an untyped or `rdfs:Literal` parameter's strings.) A
column whose rows are in different languages needs bOTTR, which can read each cell's tag.

With the language given, the run stops at the next problem:

```sh
tarka run -l ottr/ -T tt:Bookshop -i data/prepared/bookshops.csv --list stocks ";" --lang name en --test 2 --stats
```

```text
tarka: the input has no column for the mandatory parameters floorArea (is it floor_area?), secondHand (is it second_hand?), so every instance would be left out (the columns: shop, name, town, founded, floor_area, staff, specialism, second_hand, cafe, website, lat, long, point, wkt, stocks). A parameter reads the column of its own name
```

**A column name is the parameter's name, exactly.** A parameter with no column of its name
would be `none` in every row, and a mandatory one would then leave out every instance. So
tarka checks the columns first, and refuses, with status 1 and the nearest names: here the
table says `floor_area` and `second_hand` where the template says `?floorArea` and
`?secondHand`. Rename the columns (the header row, or Polars' `rename`), or put a row template
between the table and the template, whose parameters are named for the columns. A missing
column for an *optional* parameter is only a warning, since the run can still make something.
(Before tarka 0.3.1, both runs above made nothing, with status 0.)

With the columns renamed, the run goes through, and shows what remains:

```sh
sed '1s/floor_area/floorArea/; 1s/second_hand/secondHand/' data/prepared/bookshops.csv > out/bookshops-renamed.csv
tarka run -l ottr/ -T tt:Bookshop -i out/bookshops-renamed.csv --list stocks ";" --lang name en --test 2
```

```text
shop:bookbarrow geo:hasDefaultGeometry <geom:shop-bookbarrow> ;
	rdfs:label "Bookbarrow"@en ;
	bs:hasCafe "no"^^xsd:boolean ;
	bs:locatedIn <place:keswick> ;
	…
<geom:shop-bookbarrow> geo:asWKT "<http://www.opengis.net/def/crs/OGC/1.3/CRS84> POINT(-3.13089 54.60668)^^geo:wktLiteral" ;
```

**A datatype OTTR does not have cannot come from a parameter.** OTTR has no type for
`geo:wktLiteral`, so `tt:Geometry` takes an `rdfs:Literal`, and a cell gives it a plain string,
the `^^geo:wktLiteral` in the cell included.

**A boolean is `true` or `false`.** The spreadsheet's `yes` and `no` become
`"no"^^xsd:boolean`, kept as written and ill-typed, for SHACL to find
([Chapter 10](10-shapes-and-shacl.md)).

**Every prefix the cells use must be declared.** A cell `geom:shop-bookbarrow`, with no `geom:`
prefix in the library, is still a valid IRI, with the scheme `geom`, and becomes
`<geom:shop-bookbarrow>`, as bOTTR reads it. `ottr/rows.stottr` declares `shop:`, `book:` and
`stock:` for exactly this reason.

A library can declare the prefixes, but no template can type the WKT or read `yes` as `true`.
Those two are why the bookshops are made through bOTTR in the next chapter: a bOTTR map says per column how a cell becomes a term, with
a language tag, a datatype from the cell, yes/no read as booleans. For a table whose cells are
already right, like the stock, `tarka run -l` is the shortest way.

## Several roots

`-T` repeats. Each root is instantiated for every row, with its own parameters read from the
same columns, and a root's mandatory parameters drop only its own instance:

```sh
tarka run -l vendor/retail/ -l my-templates.stottr -T my:Customer -T my:Audit -i customers.csv
```

Libraries repeat too (`-l`): a published library, and your own module on top of it.

## Instances written by hand: tarka expand

Not everything starts in a table. The committee has a new member, Two Rivers Books in Exeter,
and rather than wait for the next spreadsheet, someone writes it as OTTR instances
(`instances/two-rivers.stottr`):

```stottr
tt:Bookshop(
    bt:shop-two-rivers,
    "Two Rivers Books"@en,
    bt:place-exeter,
    "2026"^^xsd:gYear,
    "64.5"^^xsd:decimal,
    "2"^^xsd:integer,
    bt:genre-nature-writing,
    true,
    true,
    "https://example.org/shops/two-rivers"^^xsd:anyURI,
    "50.72381"^^xsd:decimal,
    "-3.52695"^^xsd:decimal,
    bt:geom-shop-two-rivers,
    "<http://www.opengis.net/def/crs/OGC/1.3/CRS84> POINT(-3.52695 50.72381)"^^geo:wktLiteral,
    (bt:book-hedgerow-alphabet, bt:book-drainage)
) .

tt:Connects(bt:shop-west-quay, bt:shop-two-rivers) .
```

(and a `tt:Segment` for the walk from West Quay Books). Instances are written the way OTTR
writes terms, so everything a table cannot say can be said here: the name has its language, the
point its datatype, the list is a list.

```sh
tarka expand -l ottr/ instances/two-rivers.stottr -o out/two-rivers.ttl
```

```turtle
shop:two-rivers geo:hasDefaultGeometry bt:geom-shop-two-rivers ;
	geo:hasGeometry bt:geom-shop-two-rivers ;
	a geo:Feature , bs:Bookshop ;
	rdfs:label "Two Rivers Books"@en ;
	wgs84:lat 50.72381 ;
	wgs84:long -3.52695 ;
	bs:floorArea 64.5 ;
	bs:founded "2026"^^xsd:gYear ;
	bs:hasCafe true ;
	bs:locatedIn bt:place-exeter ;
	bs:sellsSecondHand true ;
	bs:specialises bt:genre-nature-writing ;
	bs:staffCount 2 ;
	bs:stocks book:drainage , book:hedgerow-alphabet ;
	bs:website "https://example.org/shops/two-rivers"^^xsd:anyURI .
```

`tarka expand` expands instances as Lutra does, and its output is Lutra's; the Turtle uses the
library's prefixes, which is why the shop is written `shop:two-rivers`. Blank nodes made by
templates are fresh for each instance; a blank node written in the instances is one node
throughout.

---

| [← 6. Templates](06-templates.md) | [8. bOTTR →](08-bottr.md) |
|---|---|
