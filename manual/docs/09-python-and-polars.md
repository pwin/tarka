# 9. Python and Polars

Spreadsheets are rarely ready to map. Columns need joining, splitting, gathering into lists,
renaming; values need reformatting. In a pipeline that work belongs to a table tool, and in
Python the table tool is [Polars](https://pola.rs). tarka's Python package, `tarka-rdf`, takes a
Polars data frame wherever the command takes a file, so the preparing and the mapping happen in
one process, without a CSV file between them.

This chapter does both: it walks through `python/prepare.py`, which made the prepared tables
Part III used, and `python/frames.py`, which maps frames and a Parquet file from Python.

## Preparing tables with Polars

OTTR templates read values as they should come out, so the prepared tables hold prefixed names,
WKT literals with their datatype, and lists. Every column is read as text, as the committee
typed it:

```python
read = lambda name: pl.read_csv(DATA / name, infer_schema=False)
```

**Prefixed names and WKT, per row.** An expression per column, all evaluated together:

```python
def prefixed(prefix: str, column: str) -> pl.Expr:
    """`inkwell` -> `shop:inkwell`; an empty cell stays empty."""
    return pl.when(pl.col(column).is_not_null()).then(pl.lit(prefix + ":") + pl.col(column))

shops = read("bookshops.csv").with_columns(
    prefixed("shop", "shop").alias("shop_iri"),
    prefixed("place", "town").alias("town"),
    prefixed("genre", "specialism").alias("specialism"),
    (pl.lit("geom:shop-") + pl.col("shop")).alias("point"),
    # OTTR has no type for WKT, so the cell carries its datatype for the bOTTR map to read
    pl.format(CRS84 + " POINT({} {})^^geo:wktLiteral", "long", "lat").alias("wkt"),
    # "scree;the-tarn" -> "book:scree;book:the-tarn"
    pl.col("stocks").str.split(";").list.eval(pl.lit("book:") + pl.element()).list.join(";"),
)
```

**A join, twice.** A trail segment's label and line come from the shops at its two ends, which
is a join of `segments.csv` to the shops, once for each end:

```python
ends = shops.select("shop", "name", "lat", "long")
segments = (
    read("segments.csv")
    .join(ends.rename({c: f"from_{c}" for c in ends.columns}), left_on="from", right_on="from_shop")
    .join(ends.rename({c: f"to_{c}" for c in ends.columns}), left_on="to", right_on="to_shop")
    .with_columns(
        pl.format("{} to {}", "from_name", "to_name").alias("label"),
        pl.format(CRS84 + " LINESTRING({} {}, {} {}, {} {})^^geo:wktLiteral",
                  "from_long", "from_lat", "via_long", "via_lat", "to_long", "to_lat").alias("wkt"),
        …
    )
)
```

A bOTTR map's H2 query cannot join (Chapter 8), and a TARQL query sees one row at a time; a
data frame does it in a line.

**Lists, from rows.** A shop's stock is 95 rows in `stock.csv`; `tt:ShopStock` wants one row
per shop with four lists. `group_by` and `agg` gather them, with typed values:

```python
stock = (
    read("stock.csv")
    .with_columns(
        (pl.lit("stock:") + pl.col("shop") + "--" + pl.col("book")).alias("record"),
        (pl.lit("book:") + pl.col("book")).alias("book"),
        pl.col("copies").cast(pl.Int64),
        pl.col("shelf_price").cast(pl.Decimal(10, 2)),
        prefixed("shop", "shop").alias("shop"),
    )
    .group_by("shop", maintain_order=True)
    .agg(pl.col("record").alias("records"), pl.col("book").alias("books"),
         pl.col("copies"), pl.col("shelf_price").alias("prices"))
)
stock.write_parquet(OUT / "shop-stock.parquet")
```

Prices are decimals with two places, not floats: `9.99` is exactly `9.99`, as money should be.
The CSV copy joins each list with `;`, because CSV cannot hold a list; the Parquet file holds
real lists, and that is the file to prefer.

```sh
python python/prepare.py
```

```text
data/prepared/bookshops.csv: 9418 bytes
data/prepared/segments.csv: 8143 bytes
data/prepared/shop-stock.csv: 6354 bytes
data/prepared/shop-stock.parquet: 5174 bytes
```

## Mapping a data frame

```python
import polars as pl
import tarka_rdf

shops = pl.read_csv("data/bookshops.csv", infer_schema=False)
query = tarka_rdf.Mapping.tarql_file("tarql/bookshops.rq")
triples = query.triplify(shops)
```

A `Mapping` is a compiled mapping: `Mapping.tarql_file(path)` or `Mapping.tarql(text)` for a
TARQL query, `Mapping.ottr(library, roots)` for templates. `triplify` gives the triples as a
frame of N-Triples terms, with the columns `subject`, `predicate` and `object` and no
duplicates, so Polars can look at them before anything is written. `python/frames.py` prints
Marginalia's first six:

```text
bookshops: 33 rows, 588 triples
| subject            | predicate    | object            |
|--------------------|--------------|-------------------|
| bt:shop-marginalia | rdf:type     | bs:Bookshop       |
| bt:shop-marginalia | rdf:type     | geo:Feature       |
| bt:shop-marginalia | rdfs:label   | "Marginalia"@en   |
| bt:shop-marginalia | bs:locatedIn | bt:place-wigtown  |
| bt:shop-marginalia | bs:founded   | "1994"^^xsd:gYear |
| bt:shop-marginalia | bs:floorArea | "85"^^xsd:decimal |
```

(shortened for print: the frame holds full IRIs). `write` writes a file, or returns the text
when no path is given:

```python
query.write(shops, "out/bookshops-frame.ttl")              # Turtle
query.write(shops, "out/shops.nt", format="ntriples")
text = query.write(shops, format="nquads", graph="https://example.org/bookshop-trail/graph/shops")
query.run_csv("data/bookshops.csv", "out/bookshops.ttl")   # straight from CSV, as the command reads it
```

The 588 triples are the command's 588. A TARQL query binds every cell as a string, as it does
for CSV, which is why the frame was read with `infer_schema=False`.

## Parquet and list columns, from Python

```python
stock = pl.read_parquet("data/prepared/shop-stock.parquet")
print(stock.schema)
templates = tarka_rdf.Mapping.ottr(["ottr"], ["tt:ShopStock"])
templates.write(stock, "out/shop-stock-frame.nt", format="ntriples")
```

```text
Schema({'shop': String, 'records': List(String), 'books': List(String), 'copies': List(Int64), 'prices': List(Decimal(precision=10, scale=2))})
```

The list columns go to the templates' list parameters as they are; a text column for a list
parameter needs its separator instead, `Mapping.ottr(library, roots, lists={"records": ";"})`,
as an `rdf:langString` parameter needs its column's language, `languages={"name": "en"}`
([Chapter 7](07-ottr-over-tables.md)). A frame whose columns are not named for the parameters
is refused before any row is made, with the nearest column named; Polars' `rename` is the cure.
And because the triples are a frame, a question can be asked before any store sees them: which
shops hold the most stock records?

```python
made = templates.triplify(stock)
busiest = (
    made.filter(pl.col("predicate") == "<https://example.org/bookshop-trail/schema#atShop>")
    .group_by("object")
    .len("records")
    .sort(["records", "object"], descending=[True, False])
    .head(5)
)
```

```text
| object                | records |
|-----------------------|---------|
| bt:shop-ex-libris     | 6       |
| bt:shop-castle-steps  | 4       |
| bt:shop-colophon      | 4       |
| bt:shop-cotton-quarto | 4       |
| bt:shop-endpapers     | 4       |
```

## How Polars types become cells

The same rules apply to a frame in Python and to a Parquet or Arrow file given to the command:

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
parameter gives `"42"^^xsd:integer`, and a list keeps its items' types: a list of decimals gives
`9.99`, a list of dates `2025-03-08`. (Before tarka 0.3.1, a list's items came through as
Polars stores them, a decimal as its scaled integer.)

## When a row cannot be made

A row with a cell its parameter cannot take is left out, the others are made, and a
`ValueError` lists the rows once the rest is done:

```text
ValueError: 1 row left out:
row 3: column shop: 'Border Print' is not an IRI
```

`strict=False` makes the triples it can and raises nothing:

```python
templates.triplify(stock, strict=False)     # 30 triples from the two good rows
```

`write` and `run_csv` take `strict` too; with a path, the file is written before the error is
raised, so the good rows are not lost.

## The rest of the package

```python
m.name, m.prefixes                    # the plan's name; the prefixes, for Turtle
m.shapes(base="https://example.org/bookshop-trail/shapes#")    # SHACL shapes, as Turtle
tarka_rdf.expand(["ottr"], ["instances/two-rivers.stottr"], "out/two-rivers.ttl")
tarka_rdf.bottr(["ottr"], ["bottr/bookshops.bottr.ttl"], "out/bookshops-bottr.ttl")
```

Each gives the same RDF as the command. The mapping runs without Python's global interpreter
lock, on every core, so a large frame is mapped in parallel while other threads go on.
[Appendix B](b-python-reference.md) lists every function and argument.

---

| [← 8. bOTTR](08-bottr.md) | [10. Shapes and SHACL →](10-shapes-and-shacl.md) |
|---|---|
