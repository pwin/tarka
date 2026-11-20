# 4. A first mapping

TARQL is the quickest way from a spreadsheet to RDF: a SPARQL `CONSTRUCT` query, run once for
every row, with the row's cells already bound to variables named after the columns. If you can
write SPARQL, you can write a TARQL mapping, and this chapter writes two.

## Five lines that work

`tarql/bookshops-first.rq` makes each row of `bookshops.csv` a bookshop with a name and a town:

```sparql
PREFIX bt:   <https://example.org/bookshop-trail/>
PREFIX bs:   <https://example.org/bookshop-trail/schema#>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>

CONSTRUCT {
  ?shop_iri a bs:Bookshop ;
      rdfs:label   ?label ;
      bs:locatedIn ?town_iri .
}
WHERE {
  BIND(IRI(CONCAT(STR(bt:), "shop-", ?shop))  AS ?shop_iri)
  BIND(IRI(CONCAT(STR(bt:), "place-", ?town)) AS ?town_iri)
  BIND(STRLANG(?name, "en")                    AS ?label)
}
```

There is no `FROM` and no triple pattern in the `WHERE`: the data is the row. For each row,
`?shop`, `?name`, `?town` and every other column hold the cells, as plain strings. The `BIND`s
compute what the template needs, and the template says what to write:

- `IRI(CONCAT(STR(bt:), "shop-", ?shop))` turns `bookbarrow` into `bt:shop-bookbarrow`.
  `STR(bt:)` is the namespace as a string, so the IRI follows the prefix if you ever change it.
- `STRLANG(?name, "en")` gives the name its language tag, as the trail's data has it.

Run it on the first two rows:

```sh
tarka run -q tarql/bookshops-first.rq -i data/bookshops.csv --test 2
```

```turtle
@prefix bs: <https://example.org/bookshop-trail/schema#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix bt: <https://example.org/bookshop-trail/> .
bt:shop-bookbarrow a bs:Bookshop ;
	rdfs:label "Bookbarrow"@en ;
	bs:locatedIn bt:place-keswick .
bt:shop-bookwyrm a bs:Bookshop ;
	rdfs:label "The Bookwyrm"@en ;
	bs:locatedIn bt:place-york .
```

`--test` reads only the first rows (five without a number), which is the way to try a mapping
on a large file. Without it, and with `-o`, the whole file goes to a file, and `--stats` says
what happened:

```sh
tarka run -q tarql/bookshops-first.rq -i data/bookshops.csv -o out/bookshops-first.ttl --stats
```

```text
33 records, 33 solutions, 99 triples made, 99 written
```

Thirty-three rows, one solution each, three triples each. Options and file names may come in
any order; `-q` is the query and `-i` the input in every example here.

## What comes out

Turtle is the default, with the query's prefixes. The others are a flag away:

```sh
tarka run -q tarql/bookshops-first.rq -i data/bookshops.csv --ntriples
tarka run -q tarql/bookshops-first.rq -i data/bookshops.csv --format nquads --graph https://example.org/bookshop-trail/graph/shops
```

```text
<https://example.org/bookshop-trail/shop-bookbarrow> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.org/bookshop-trail/schema#Bookshop> <https://example.org/bookshop-trail/graph/shops> .
```

N-Quads needs `--graph`, the graph every triple goes into, and says so if it is missing
(`tarka: N-Quads output needs --graph`). `-g` gzips the file. The format does **not** follow the
file name: `-o out/shops.nt` without `--ntriples` writes Turtle into a file called `.nt`, as
oxi-gen does.

Two properties of the output matter in a pipeline:

- **It is the same every time.** N-Triples and N-Quads come out in the order the mapping makes
  the triples, row by row; Turtle groups each window's triples by subject. Blank node labels are
  stable. The same input gives the same bytes.
- **A file is written all or nothing.** `-o FILE` is written under another name beside it and
  moved into place when the run is complete. A run that fails partway (a malformed row, a full
  disk) leaves an earlier `FILE` as it was, never half of a new one. If `FILE` is open in
  another program that locks it, tarka says `cannot write FILE: Access is denied` and exits
  with status 1, leaving the old file alone.

## How a row binds

TARQL's rule is simple, and worth knowing exactly:

- **Each column binds the variable of the same name**, as a plain string (`"80.0"`, not the
  number 80). Only the columns the query mentions are bound.
- **An empty cell binds nothing.** Its variable is unbound, so any expression using it is
  unbound too, and a template triple with an unbound variable is not written. That is how
  optional columns work, with no special syntax; the full mapping below relies on it.
  `--bind-empty-strings` binds empty cells as `""` instead.
- **`?ROWNUM`** is the row's number, counting from 0 (TARQL proper counts from 1;
  [Appendix C](c-differences.md)).
- **`BOUND(?column)`** is true when the cell has a value, as in TARQL.

## The whole bookshop

`tarql/bookshops.rq` makes everything the course's data says about a shop: both classes, every
property, and the point geometry. The template:

```sparql
CONSTRUCT {
  ?shop_iri a bs:Bookshop, geo:Feature ;
      rdfs:label             ?label ;
      bs:locatedIn           ?town_iri ;
      bs:founded             ?founded_year ;
      bs:floorArea           ?area ;
      bs:staffCount          ?staff_count ;
      bs:specialises         ?genre_iri ;
      bs:sellsSecondHand     ?sells_second_hand ;
      bs:hasCafe             ?has_cafe ;
      bs:website             ?site ;
      wgs84:lat              ?latitude ;
      wgs84:long             ?longitude ;
      geo:hasGeometry        ?point ;
      geo:hasDefaultGeometry ?point .
  ?point a sf:Point, geo:Geometry ;
      geo:asWKT ?wkt .
}
```

The `WHERE` clause does four kinds of work.

**IRIs from short names**, as before, one per kind of thing:

```sparql
  BIND(IRI(CONCAT(STR(bt:), "shop-", ?shop))        AS ?shop_iri)
  BIND(IRI(CONCAT(STR(bt:), "place-", ?town))       AS ?town_iri)
  BIND(IRI(CONCAT(STR(bt:), "genre-", ?specialism)) AS ?genre_iri)
  BIND(IRI(CONCAT(STR(bt:), "geom-shop-", ?shop))   AS ?point)
```

**Typed values.** A cell is a string until you say otherwise. A cast checks the text and writes
the value in canonical form; `STRDT` gives a datatype without checking:

```sparql
  BIND(STRDT(?founded, xsd:gYear)       AS ?founded_year)
  BIND(xsd:decimal(?floor_area)         AS ?area)
  BIND(xsd:integer(?staff)              AS ?staff_count)
  BIND(xsd:decimal(?lat)                AS ?latitude)
  BIND(xsd:decimal(?long)               AS ?longitude)
  BIND(STRDT(?website, xsd:anyURI)      AS ?site)          # unbound when the cell is empty
```

A cast that fails leaves its variable unbound: `xsd:integer("n/a")` binds nothing, so that row
gets no `bs:staffCount` and the run goes on. `STRDT("c.1972", xsd:gYear)` writes an ill-typed
literal, which validation will catch ([Chapter 10](10-shapes-and-shacl.md)). Use a cast where
SPARQL has one for the type, and `STRDT` where it does not (`xsd:gYear`, `xsd:anyURI`).

**Canonical forms.** A cast writes the value the way XSD writes it canonically: `80.0` becomes
`"80"^^xsd:decimal`, and `9.50` becomes `"9.5"`. They are the same values as the spreadsheet's,
equal under SPARQL's `=` and to any store, but not the same strings. oxi-gen does the same,
and the OTTR route does too, so both kinds of mapping give the same terms.

**yes and no.** SPARQL has no table for this, but `IF` does it, and leaves anything else
unbound, for the shapes to find:

```sparql
  BIND(IF(?second_hand = "yes", true, IF(?second_hand = "no", false, ?unset)) AS ?sells_second_hand)
```

`?unset` is never bound, so `IF` returns nothing for a cell that is neither `yes` nor `no`.

**The point.** GeoSPARQL writes longitude first, after the coordinate reference system:

```sparql
  BIND(STRDT(CONCAT("<http://www.opengis.net/def/crs/OGC/1.3/CRS84> POINT(", ?long, " ", ?lat, ")"),
             geo:wktLiteral) AS ?wkt)
```

Run it:

```sh
tarka run -q tarql/bookshops.rq -i data/bookshops.csv -o out/bookshops.ttl --stats
```

```text
33 records, 33 solutions, 588 triples made, 588 written
```

Castle Steps Books, in Hay-on-Wye, has no website in the spreadsheet, and so none in the graph:

```turtle
bt:shop-castle-steps geo:hasDefaultGeometry bt:geom-shop-castle-steps ;
	geo:hasGeometry bt:geom-shop-castle-steps ;
	a geo:Feature , bs:Bookshop ;
	rdfs:label "Castle Steps Books"@en ;
	wgs84:lat 52.07542 ;
	wgs84:long -3.11883 ;
	bs:floorArea "230"^^xsd:decimal ;
	bs:founded "1962"^^xsd:gYear ;
	bs:hasCafe true ;
	bs:locatedIn bt:place-hay-on-wye ;
	bs:sellsSecondHand true ;
	bs:specialises bt:genre-history ;
	bs:staffCount 8 .
```

Turtle's shorthand is at work here: `52.07542` is an `xsd:decimal` and `8` an `xsd:integer`,
while `"230"^^xsd:decimal` is written out, because a bare `230` would read as an integer.

That is the whole of the shops. Is it right? Twenty-seven of the thirty-three shops have
websites in the course data, and twenty-seven `bs:website` triples were made; in
[Chapter 11](11-sparql-checks.md), a comparison against the course's graph confirms every one
of the 588 triples.

---

| [← 3. The Bookshop Trail](03-the-bookshop-trail.md) | [5. TARQL in depth →](05-tarql-in-depth.md) |
|---|---|
