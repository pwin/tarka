"""tarka-rdf on 200,000 typed rows: OTTR templates, a TARQL query, and a bOTTR map.

    pip install tarka-rdf
    python examples/python/retail.py        # from the root of a tarka checkout, for the fixtures

It writes orders_typed.parquet, orders_tarql.ttl and orders_from_csv.nt to the working
directory.
"""

import time

import polars as pl
import tarka_rdf

# 200,000 typed rows: the retail fixture's 8 order lines, 25,000 times over, each copy
# with its own order IRIs. Polars types the columns: qty is Int64, unit_price Float64,
# ordered_at a Datetime in UTC, shipped_on a Date.
base = pl.read_csv("tests/fixtures/retail/orders.csv", try_parse_dates=True)
copies = pl.DataFrame({"copy": pl.int_range(0, 25_000, eager=True)})
df = (
    base.join(copies, how="cross")
    .with_columns(pl.format("ord:O{}-{}", "copy", pl.col("order").str.strip_prefix("ord:O-")).alias("order"))
    .drop("copy")
)
df.write_parquet("orders_typed.parquet")
df = pl.read_parquet("orders_typed.parquet")
print(f"{df.height:,} rows: {dict(df.schema)}")


def timed(what, f):
    start = time.perf_counter()
    result = f()
    print(f"{what}: {time.perf_counter() - start:.2f} s")
    return result


# OTTR: the retail template library, rt:OrderRow as the root. Typed columns feed typed
# parameters directly (an Int64 column becomes xsd:integer literals).
ottr = tarka_rdf.Mapping.ottr(["tests/fixtures/retail/ottr"], ["rt:OrderRow"])
by_ottr = timed("OTTR over the frame", lambda: ottr.triplify(df))
print(f"  {by_ottr.height:,} distinct triples")

# TARQL: the same transformation as a SPARQL CONSTRUCT query. A TARQL row binds every
# cell as a plain string, as for CSV, so typed columns are written as text first
# (2024-03-07, 89.5 …) and the query does its own casting.
tarql = tarka_rdf.Mapping.tarql_file("tests/fixtures/retail/tarql/orders.rq")
by_tarql = timed("TARQL over the frame", lambda: tarql.triplify(df))
print(f"  {by_tarql.height:,} distinct triples")

# The two mappings describe the same RDF. Blank node labels differ between them, so
# compare the triples without blank nodes as sets, and count the rest.
def no_blanks(t):
    return t.filter(~pl.col("subject").str.starts_with("_:") & ~pl.col("object").str.starts_with("_:"))

a, b = no_blanks(by_ottr), no_blanks(by_tarql)
print(f"  same triples without blank nodes: {a.height == b.height and a.join(b, on=['subject', 'predicate', 'object'], how='anti').height == 0}")

# Either mapping can write a file instead (Turtle by default), or read CSV directly in
# oxi-gen's dialect.
timed("TARQL to Turtle", lambda: tarql.write(df, "orders_tarql.ttl"))
tarql.run_csv("tests/fixtures/retail/orders.csv", "orders_from_csv.nt", format="ntriples")

# bOTTR: instance maps over a CSV file (H2's CSVREAD), with argument maps per column.
# bOTTR maps name OTTR templates; a TARQL query is already its own mapping file.
text = tarka_rdf.bottr(["tests/fixtures/people/people.stottr"], ["tests/fixtures/bottr/people.bottr.ttl"], format="ntriples")
print(f"bOTTR people map: {text.count(chr(10))} triples")
