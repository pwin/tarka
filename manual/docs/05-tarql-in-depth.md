# 5. TARQL in depth

Chapter 4 mapped one spreadsheet, the easy way. The other four bring the problems real
spreadsheets bring: lists in a cell, dates in local formats, prices with currency signs,
columns that are mostly empty, values whose meaning depends on another column. This chapter
takes them one at a time, then the CSV options, then the functions a TARQL query can call
beyond SPARQL's own, and ends with what tarka refuses.

## A list in one cell

Each shop's row lists the titles it stocks, separated by semicolons: `scree;the-tarn`. The
ontology wants one `bs:stocks` triple per title. There are two ways to get them.

**Jena's `apf:strSplit`**, as TARQL proper has it, splits the cell inside the query
(`tarql/stocks.rq`):

```sparql
PREFIX apf: <http://jena.apache.org/ARQ/property#>

CONSTRUCT {
  ?shop_iri bs:stocks ?book_iri .
}
WHERE {
  BIND(IRI(CONCAT(STR(bt:), "shop-", ?shop)) AS ?shop_iri)
  ?book apf:strSplit (?stocks ";")
  BIND(IRI(CONCAT(STR(bt:), "book-", ?book)) AS ?book_iri)
}
```

`?book apf:strSplit (?stocks ";")` looks like a triple pattern, and is a *property function*:
it binds `?book` to each part of `?stocks` in turn, so one row gives as many solutions as it
has titles. The separator is a regular expression, and each part is trimmed, exactly as Jena
does it.

```sh
tarka run -q tarql/stocks.rq -i data/bookshops.csv --test 2
```

```turtle
bt:shop-bookbarrow bs:stocks bt:book-scree , bt:book-the-tarn .
bt:shop-bookwyrm bs:stocks bt:book-the-dark-sea , bt:book-the-leat , bt:book-y-mor-tywyll .
```

**oxi-gen's `--split`** splits the cell before the query sees it, repeating the row once per
part with the part in a new column (`tarql/stocks-split.rq` uses `?book` and no property
function):

```sh
tarka run -q tarql/stocks-split.rq -i data/bookshops.csv --split stocks book ";"
```

The triples are the same. `--stats` shows where the work happens:

```text
33 records, 95 solutions, 95 triples made, 95 written     # apf:strSplit
95 records, 95 solutions, 95 triples made, 95 written     # --split
```

`apf:strSplit` reads 33 rows and makes 95 solutions of them; `--split` turns the file into 95
rows first. Use the property function when the query is shared with TARQL proper, or when only
part of the mapping is per title; use `--split` for a query that must also run in oxi-gen.

A property function is evaluated where it stands, at the top level of the `WHERE` clause, in
the order written: patterns before it see the row, patterns after it see each part. One inside
`OPTIONAL`, `UNION` or a nested group is refused (see below).

## Dates, prices and casts

`events.csv` has British dates and prices with a pound sign:

```text
event,shop,date,title,kind,featuring,attendance,ticket
bookbarrow-2025-09-06,bookbarrow,06/09/2025,Workshop with Bram Tillotson,Workshop,bram-tillotson,16,£28.00
```

`tarql/events.rq` rearranges the date and casts it, and strips the sign before casting the
price:

```sparql
  # dd/mm/yyyy to yyyy-mm-dd, then cast: a date that is not one leaves ?day unbound
  BIND(xsd:date(CONCAT(SUBSTR(?date, 7, 4), "-", SUBSTR(?date, 4, 2), "-", SUBSTR(?date, 1, 2))) AS ?day)

  BIND(xsd:integer(?attendance)                     AS ?people)
  BIND(xsd:decimal(REPLACE(?ticket, "^£", ""))      AS ?price)
```

```turtle
bt:event-bookbarrow-2025-09-06 a bs:Event ;
	rdfs:label "Workshop with Bram Tillotson"@en ;
	bs:attendance 16 ;
	bs:eventDate "2025-09-06"^^xsd:date ;
	bs:eventKind "Workshop" ;
	bs:featuring bt:author-bram-tillotson ;
	bs:heldAt bt:shop-bookbarrow ;
	bs:ticketPrice "28"^^xsd:decimal .
```

The cast is the check: `31/02/2025` would become `2025-02-31`, which `xsd:date()` refuses, so
`?day` is unbound and the event gets no date, a fault the shapes in
[Chapter 10](10-shapes-and-shacl.md) report. `xsd:date()`, `xsd:time()` and `xsd:duration()`
casts are there because HOLOS's engine has them, as Jena does; they give what TARQL 1.3 gives.

## Optional columns, and a class that depends on one

`works.csv` describes 74 books. Most are not in a series, and only six are translations, so
those columns are mostly empty:

```text
book,title,title_language,author,publisher,year,genre,pages,rrp,language,isbn,series,series_position,translation_of,translated_by
the-dark-sea-ar,البحر المظلم,ar,elin-morgan,thornfield,2019,translated-fiction,168,20.28,cy,9786429677030,,,the-dark-sea,noor-haddad
```

An empty cell leaves its variable unbound, and every expression built on it unbound too, so
the triples that use it are simply not written. That makes optional columns free:

```sparql
  BIND(IRI(CONCAT(STR(bt:), "series-", ?series))         AS ?series_iri)
  BIND(xsd:integer(?series_position)                     AS ?position)
  BIND(IRI(CONCAT(STR(bt:), "book-", ?translation_of))   AS ?original_iri)
  BIND(IRI(CONCAT(STR(bt:), "author-", ?translated_by))  AS ?translator_iri)
```

A translation is also a `bs:Translation`. The class depends on a column, so it is a variable in
the template (`?work a bs:Work, ?translation_class`), bound only when the column has a value:

```sparql
  BIND(IF(BOUND(?translation_of), bs:Translation, ?unset) AS ?translation_class)
```

The title is in the language of the book in hand, which the spreadsheet gives in its own
column, so the language tag comes from the data:

```sparql
  BIND(STRLANG(?title, ?title_language) AS ?title_tagged)
```

```turtle
bt:book-the-dark-sea-ar dct:title "البحر المظلم"@ar ;
	a bs:Translation , bs:Work ;
	rdfs:label "البحر المظلم"@ar ;
	bs:author bt:author-elin-morgan ;
	bs:genre bt:genre-translated-fiction ;
	bs:isbn "9786429677030" ;
	bs:originalLanguage "cy" ;
	bs:pages 168 ;
	bs:publicationYear "2019"^^xsd:gYear ;
	bs:publishedBy bt:pub-thornfield ;
	bs:rrp 20.28 ;
	bs:translatedBy bt:author-noor-haddad ;
	bs:translationOf bt:book-the-dark-sea .
```

*The Dark Sea* was first written in Welsh (`bs:originalLanguage "cy"`); this is its Arabic
translation, titled in Arabic. Here the trail meets the edge of RDF 1.1. The course data writes
this title `"البحر المظلم"@ar--rtl`: Arabic, *written right to left*, a base direction that only
RDF 1.2 can express. tarka writes RDF 1.1, so its title has the language and not the direction.
Three titles are like this; they are the only difference between this manual's output and the
course's graph, and [Chapter 11](11-sparql-checks.md) counts them separately.

## Reading the CSV

tarka reads CSV as oxi-gen does: a header row, commas, double quotes, and a backslash escaping
inside quotes. The options change each of those, with oxi-gen's names:

| Option | Does |
|---|---|
| `-d ';'`, `--delimiter` | another field delimiter |
| `-t`, `--tab` | tab-separated fields |
| `--quote-char "'"` | another quote character |
| `-p '\'`, `--escape-char` | the escape character inside quoted fields (default `\`) |
| `-H`, `--no-header-row` | no header: the columns are `?a`, `?b` … `?z`, then `?A` … `?Z` |
| `-n`, `--normalize` | upper-case the column names, so `floor_area` binds `?FLOOR_AREA` |
| `--bind-empty-strings` | an empty cell binds `""` instead of nothing |
| `--test [N]` | read only the first N rows (5) |

The file is read as UTF-8 on every platform. A query may name its input with `FROM <file.csv>`
when no `-i` is given, as in TARQL.

`tarql:expandPrefixedName(?cell)` turns a cell like `bt:shop-inkwell` into the IRI the query's
prefixes make of it, and `tarql:expandPrefix("bt")` gives a prefix's namespace; the `tarql:`
prefix is declared for you, in oxi-gen's namespace, and TARQL's own works too.

## Functions beyond SPARQL

A TARQL query is evaluated by HOLOS's engine, and can call four libraries beyond SPARQL's own:
Jena's, as TARQL proper has them, and HOLOS's XPath, SPIN and GeoSPARQL functions.
`tarql/functions-tour.rq` uses many of them on the bookshops, and writes a small guide to the
trail in a namespace of its own (`guide:`), since these values illustrate the functions rather
than belong in the ontology:

```sparql
  # apf:splitIRI: an IRI split where Jena splits it, before the XML name at its end
  ?shop_iri apf:splitIRI (?namespace ?local_part)
  BIND(afn:localname(?shop_iri) AS ?local)

  ?welcome apf:concat ("Welcome to " ?name)

  BIND(spif:titleCase(REPLACE(?specialism, "-", " ")) AS ?specialism_name)
  BIND(afn:strjoin(" · ", ?name, ?specialism_name, CONCAT("since ", ?founded)) AS ?card)

  BIND(fn:upper-case(?name) AS ?sign)
  BIND(afn:substr(afn:sha1sum(?shop), 0, 8) AS ?key)

  BIND(xsd:decimal(?floor_area) / afn:max(xsd:integer(?staff), 1) AS ?per_person)
  BIND(ROUND(afn:sqrt(xsd:decimal(?floor_area) / afn:pi()) * 10) / 10 AS ?radius)

  BIND(STRDT(CONCAT("<http://www.opengis.net/def/crs/OGC/1.3/CRS84> POINT(", ?long, " ", ?lat, ")"), geo:wktLiteral) AS ?here)
  BIND(ROUND(geof:distance(?here, "<http://www.opengis.net/def/crs/OGC/1.3/CRS84> POINT(-4.44577 54.86733)"^^geo:wktLiteral, uom:metre) / 1000) AS ?km)
```

```sh
tarka run -q tarql/functions-tour.rq -i data/bookshops.csv --test 3
```

```turtle
bt:shop-bookbarrow guide:areaPerPerson 26.666666666666666667 ;
	guide:card "Bookbarrow · Mountaineering · since 1990" ;
	guide:circleRadius "5"^^xsd:double ;
	guide:key "11e3d719" ;
	guide:kmFromWigtown "89"^^xsd:double ;
	guide:namespace bt: ;
	guide:shortName "shop-bookbarrow" ;
	guide:sign "BOOKBARROW" ;
	guide:specialism "Mountaineering" ;
	guide:welcome "Welcome to Bookbarrow" .
```

Bookbarrow, in Keswick, is 89 km from Wigtown as the crow flies; The Bookwyrm, in York, 240.
The libraries, in full:

| Library | Namespace | Functions |
|---|---|---|
| Jena property functions | `apf:` `<http://jena.apache.org/ARQ/property#>` | `strSplit`, `splitIRI`, `concat`, `str`, `assign` |
| Jena functions | `afn:` `<http://jena.apache.org/ARQ/function#>` | `localname`, `namespace`, `strjoin`, `substr` (and `substring`), `sha1sum`, `min`, `max`, `sqrt`, `pi`, `e`, `now` |
| XPath | `fn:` `<http://www.w3.org/2005/xpath-functions#>` | `upper-case`, `lower-case`, `string-length`, `string`, `concat`, `substring`, `substring-before`, `substring-after`, `contains`, `starts-with`, `ends-with`, `normalize-space`, `translate`, `compare`, `abs`, `ceiling`, `floor`, `round`, `not`, `boolean`, `year-from-dateTime` … `seconds-from-dateTime` |
| SPIN | `spif:` `<http://spinrdf.org/spif#>` | `trim`, `indexOf`, `lastIndexOf`, `buildString`, `upperCase`, `lowerCase`, `titleCase`, `unCamelCase`, `encodeURL`, `decodeURL`, `generateUUID`, `name` |
| GeoSPARQL | `geof:` `<http://www.opengis.net/def/function/geosparql/>` | `distance`, `buffer`, `envelope`, `convexHull`, `boundary`, `sfContains`, `sfIntersects`, `sfWithin`, `sfEquals` and the other Simple Features, Egenhofer and RCC8 relations |

Jena's functions answer as Jena 4.8's do, which TARQL 1.3 records in `tests/fixtures/jena`;
the old namespaces (`http://jena.hpl.hp.com/ARQ/…`) and the `java:` form work too. Four more,
for completeness:

```sparql
  ?text  apf:str    ?shop_iri .                 # "https://example.org/bookshop-trail/shop-bookbarrow"
  ?alias apf:assign ?name .                     # "Bookbarrow"
  BIND(afn:namespace(?shop_iri) AS ?ns)          # "https://example.org/bookshop-trail/"
  BIND(afn:min(xsd:integer(?staff), 4) AS ?n)    # 3
```

`afn:now()` is the time the run started, the same for every row; it makes the output differ
from run to run, which is why the tour leaves it out. Two Jena functions are not here:
`afn:bnode`, which gives a blank node's label (tarka makes those afresh), and `afn:sprintf`,
which formats as Java does.

## What tarka refuses

A query tarka cannot evaluate as written is refused before any row is read, with a message,
rather than run to give nothing:

```text
tarka: q.rq: list:member is not supported: it reads the data, and a TARQL row has none
tarka: q.rq: afn:sprintf is not supported: Jena formats as Java's String.format does, which tarka does not reproduce
tarka: q.rq: a property function (apf:…) is evaluated at the top level of the WHERE clause, not inside OPTIONAL, UNION, MINUS, GRAPH, a sub-query or a nested group
tarka: q.rq: an RDF 1.2 triple term is not supported (tarka works with RDF 1.1 and SPARQL 1.1)
tarka: the mapping cannot be evaluated: rows 0–0: The custom function <http://e/nope> is not supported
```

The property functions that read the graph (`list:member`, `rdfs:member`, `apf:bag` …) have no
graph to read in a TARQL row; Jena's `apf:textMatch` needs a text index. RDF 1.2's triple terms,
SPARQL 1.2's functions and literals with a base direction are refused whichever way the parser
reads them: tarka's output is RDF 1.1, always.

## Where TARQL proper differs

tarka follows oxi-gen where TARQL and oxi-gen disagree. In practice:

- **`?ROWNUM` counts from 0**; TARQL counts from 1. Add 1 in the query to match TARQL.
- **A backslash escapes inside quotes**, as in oxi-gen; TARQL takes it literally. Use
  `-p` to change the escape character.
- **Files are read as UTF-8**; TARQL on Windows reads them in the platform's encoding.
- **Typed values come out in canonical form**: `"2"^^xsd:double` where Jena writes `"2.0e0"`.

[Appendix C](c-differences.md) has the full list, for oxi-gen and Lutra too.

---

| [← 4. A first mapping](04-a-first-mapping.md) | [6. Templates →](06-templates.md) |
|---|---|
