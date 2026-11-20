"""Prepares the committee's spreadsheets for the OTTR and bOTTR mappings, with Polars.

OTTR templates read each cell as the type of their parameter and compute nothing, so a
table for them holds values as they should come out: IRIs as prefixed names (`shop:inkwell`),
geometries as WKT (with their datatype, which OTTR has no type for). This script makes
those tables from data/*.csv:

  data/prepared/bookshops.csv     one row per shop, with its point and its titles
  data/prepared/segments.csv      one row per trail segment, joined to the shops at its ends
  data/prepared/shop-stock.csv    one row per shop, its stock as four parallel lists
  data/prepared/shop-stock.parquet  the same, with real list columns and typed values

    python python/prepare.py
"""

from pathlib import Path

import polars as pl

HERE = Path(__file__).resolve().parents[1]
DATA, OUT = HERE / "data", HERE / "data" / "prepared"
CRS84 = "<http://www.opengis.net/def/crs/OGC/1.3/CRS84>"


def prefixed(prefix: str, column: str) -> pl.Expr:
    """`inkwell` -> `shop:inkwell`; an empty cell stays empty."""
    return pl.when(pl.col(column).is_not_null()).then(pl.lit(prefix + ":") + pl.col(column))


def main() -> None:
    OUT.mkdir(exist_ok=True)
    # Every column as text, as the committee typed it: "80.0" stays "80.0"
    read = lambda name: pl.read_csv(DATA / name, infer_schema=False)

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
    shops.select(
        pl.col("shop_iri").alias("shop"), "name", "town", "founded", "floor_area", "staff", "specialism",
        "second_hand", "cafe", "website", "lat", "long", "point", "wkt", "stocks",
    ).write_csv(OUT / "bookshops.csv")

    # A segment's label and line come from the shops at its two ends: a join, twice
    ends = shops.select("shop", "name", "lat", "long")
    segments = (
        read("segments.csv")
        .join(ends.rename({c: f"from_{c}" for c in ends.columns}), left_on="from", right_on="from_shop")
        .join(ends.rename({c: f"to_{c}" for c in ends.columns}), left_on="to", right_on="to_shop")
        .with_columns(
            (pl.lit("seg:") + pl.col("from") + "-" + pl.col("to")).alias("segment"),
            (pl.lit("geom:seg-") + pl.col("from") + "-" + pl.col("to")).alias("line"),
            pl.format("{} to {}", "from_name", "to_name").alias("label"),
            pl.format(CRS84 + " LINESTRING({} {}, {} {}, {} {})^^geo:wktLiteral",
                      "from_long", "from_lat", "via_long", "via_lat", "to_long", "to_lat").alias("wkt"),
            # "from" and "to" are SQL keywords, which a bOTTR map's H2 query cannot name
            prefixed("shop", "from").alias("from_shop"),
            prefixed("shop", "to").alias("to_shop"),
        )
        .sort("segment")
    )
    segments.select("segment", "from_shop", "to_shop", "label", "km", "waymarked", "line", "wkt").write_csv(OUT / "segments.csv")

    # A shop's stock as four parallel lists, for an OTTR zipMin
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
    # CSV cannot hold a list: each becomes one cell, joined with ";"
    stock.with_columns(
        pl.col(c).list.eval(pl.element().cast(pl.String)).list.join(";") for c in ["records", "books", "copies", "prices"]
    ).write_csv(OUT / "shop-stock.csv")

    for f in sorted(OUT.iterdir()):
        print(f"{f.relative_to(HERE).as_posix()}: {f.stat().st_size} bytes")


if __name__ == "__main__":
    main()
