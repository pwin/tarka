"""The benchmark's inputs: tarka's retail templates with rt:OrderRow as the root, and the
retail orders as a typed Polars frame at several sizes.

    python comparisons/ottr/data.py OUT_DIR [ROWS ...]

ROWS are multiples of 8 (the fixture's 8 order lines, repeated with new order IRIs). It
writes OUT_DIR/lib/{base,domain,rows}.stottr and, for each size (always including 8),
OUT_DIR/orders_ROWS.parquet for tarka and maplib, and for Lutra, which reads no frames,
the same rows as OUT_DIR/orders_ROWS.csv with a bOTTR map over it, orders_ROWS.bottr.ttl.

All three tools get the same input, shaped so that maplib can read it:
* three template files, each with its prefixes first (maplib's parser takes @prefix
  only at the start of a document);
* full IRIs in the cells (maplib reads `ex:a` as an IRI with the scheme `ex`);
* Decimal(38, 12) columns for xsd:decimal parameters (maplib refuses Float64 there);
* tarka's ?ROWNUM replaced by an optional ?no_row read from an all-null column, since
  maplib parses neither `none` nor `ottr:none` as none.
"""

import sys
from pathlib import Path

import polars as pl

out = Path(sys.argv[1])
sizes = sorted({8, *(int(a) for a in sys.argv[2:])})
assert all(n % 8 == 0 for n in sizes), "ROWS must be multiples of 8"
fixtures = Path(__file__).resolve().parents[2] / "tests" / "fixtures" / "retail"
ottr = fixtures / "ottr"

rows = (ottr / "rows.stottr").read_text(encoding="utf-8")
prefixes = rows[: rows.index("# Prefixes used")]
order_row = rows[rows.index("rt:OrderRow [") :]
order_row = order_row.replace("? ?ROWNUM", "? xsd:integer ?no_row").replace("?tracking, ?ROWNUM)", "?tracking, ?no_row)")
assert "ROWNUM" not in order_row and "?no_row)" in order_row
lib = out / "lib"
lib.mkdir(parents=True, exist_ok=True)
for name in ["base.stottr", "domain.stottr"]:
    (lib / name).write_text((ottr / name).read_text(encoding="utf-8"), encoding="utf-8")
(lib / "rows.stottr").write_text(prefixes + order_row, encoding="utf-8")

namespaces = {
    "cust:": "http://example.com/customer/",
    "prod:": "http://example.com/product/",
    "ord:": "http://example.com/order/",
    "cur:": "http://example.com/currency/",
    "status:": "http://example.com/status/",
}


def full(c: str) -> pl.Expr:
    e = pl.col(c)
    for p, ns in namespaces.items():
        e = pl.when(e.str.starts_with(p)).then(pl.lit(ns) + e.str.strip_prefix(p)).otherwise(e)
    return e.alias(c)


base = pl.read_csv(fixtures / "orders.csv", try_parse_dates=True)
base = base.with_columns([full(c) for c in ["order", "customer", "status", "product", "currency"]])
base = base.with_columns([pl.col(c).cast(pl.Decimal(38, 12)) for c in ["unit_price", "discount_pct"]])
base = base.with_columns(pl.lit(None, dtype=pl.Int64).alias("no_row"))

# Lutra reads a CSV cell as text and keeps it as the literal's lexical form, so the typed
# values are written as tarka and maplib write them (2024-03-05T10:15:00Z, 89.5, 8990).
# An empty cell is H2's NULL, and OTTR's none.
trim = lambda e: e.str.replace(r"(\.\d*?)0+$", "${1}").str.replace(r"\.$", "")  # noqa: E731
as_text = [
    (trim(pl.col("ordered_at").dt.strftime("%Y-%m-%dT%H:%M:%S%.f")) + "Z").alias("ordered_at"),
    *[trim(pl.col(c).cast(pl.String)).alias(c) for c in ["unit_price", "discount_pct"]],
]
# the bOTTR argument maps, by position: rt:OrderRow's parameters and their types
parameters = [
    ("order", "ottr:IRI"), ("ordered_at", "xsd:dateTime"), ("customer", "ottr:IRI"), ("channel", None),
    ("status", "ottr:IRI"), ("line", "xsd:integer"), ("product", "ottr:IRI"), ("qty", "xsd:integer"),
    ("unit_price", "xsd:decimal"), ("currency", "ottr:IRI"), ("discount_pct", "xsd:decimal"), ("shipped_on", "xsd:date"),
    ("carrier", "xsd:string"), ("tracking", "xsd:string"), ("gift_message", None), ("no_row", "xsd:integer"),
]  # fmt: skip
assert base.columns == [name for name, _ in parameters]
argument_maps = " ".join(f"[ ottr:type {t} ]" if t else "[ ]" for _, t in parameters)

for n in sizes:
    copies = pl.DataFrame({"copy": pl.int_range(0, n // 8, eager=True)})
    df = base.join(copies, how="cross").with_columns((pl.col("order") + "-" + pl.col("copy").cast(pl.String)).alias("order")).drop("copy")
    df.write_parquet(out / f"orders_{n}.parquet")
    df.with_columns(as_text).write_csv(out / f"orders_{n}.csv")
    (out / f"orders_{n}.bottr.ttl").write_text(
        "@prefix ottr: <http://ns.ottr.xyz/0.4/> .\n@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n"
        "[] a ottr:InstanceMap ; ottr:template <http://example.com/retail/template#OrderRow> ;\n"
        "   ottr:source [ a ottr:H2Source ] ;\n"
        f"   ottr:query \"SELECT * FROM CSVREAD('@@THIS_DIR@@/orders_{n}.csv')\" ;\n"
        f"   ottr:argumentMaps ( {argument_maps} ) .\n",
        encoding="utf-8",
    )
