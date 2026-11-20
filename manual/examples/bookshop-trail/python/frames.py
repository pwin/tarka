"""tarka from Python: mappings over Polars data frames, and Parquet with list columns.

    python python/frames.py

Writes out/bookshops-frame.ttl and out/shop-stock-frame.nt, and prints what it made.
"""

from pathlib import Path

import polars as pl
import tarka_rdf

HERE = Path(__file__).resolve().parents[1]
OUT = HERE / "out"


def short(frame: pl.DataFrame) -> pl.DataFrame:
    """The frame with the trail's IRIs written short, for printing: bt:…, bs:…, rdf:…"""
    names = {"https://example.org/bookshop-trail/schema#": "bs:", "https://example.org/bookshop-trail/": "bt:",
             "http://www.w3.org/1999/02/22-rdf-syntax-ns#": "rdf:", "http://www.w3.org/2000/01/rdf-schema#": "rdfs:",
             "http://www.w3.org/2001/XMLSchema#": "xsd:", "http://www.opengis.net/ont/geosparql#": "geo:"}
    def shorten(column: pl.Expr) -> pl.Expr:
        for ns, prefix in names.items():
            column = column.str.replace_all(f"<{ns}([^>]*)>", prefix + "$1")
        return column
    return frame.with_columns(shorten(pl.col(c)) for c, t in frame.schema.items() if t == pl.String)


def main() -> None:
    OUT.mkdir(exist_ok=True)

    # 1. A TARQL query over a data frame. Read every column as text, as TARQL reads CSV:
    #    with types inferred, "80.0" would arrive as the float 80.0 and be written "80.0"
    #    all the same, but a column of shop names could not be told from one of numbers.
    shops = pl.read_csv(HERE / "data" / "bookshops.csv", infer_schema=False)
    query = tarka_rdf.Mapping.tarql_file(HERE / "tarql" / "bookshops.rq")
    triples = query.triplify(shops)  # a frame of N-Triples terms: subject, predicate, object
    print(f"{query.name}: {shops.height} rows, {triples.height} triples")
    print(short(triples.filter(pl.col("subject") == "<https://example.org/bookshop-trail/shop-marginalia>").head(6)))
    query.write(shops, OUT / "bookshops-frame.ttl")

    # 2. OTTR templates over a Parquet file. The list columns go to the templates' list
    #    parameters as they are, with no separator; the Decimal prices keep their scale.
    stock = pl.read_parquet(HERE / "data" / "prepared" / "shop-stock.parquet")
    print(stock.schema)
    templates = tarka_rdf.Mapping.ottr([HERE / "ottr"], ["tt:ShopStock"])
    templates.write(stock, OUT / "shop-stock-frame.nt", format="ntriples")

    # 3. The triples are a frame, so Polars can ask questions of them before any store
    #    does: the five shops with the most stock records.
    made = templates.triplify(stock)
    busiest = (
        made.filter(pl.col("predicate") == "<https://example.org/bookshop-trail/schema#atShop>")
        .group_by("object")
        .len("records")
        .sort(["records", "object"], descending=[True, False])
        .head(5)
    )
    print(short(busiest))


if __name__ == "__main__":
    pl.Config.set_tbl_formatting("ASCII_MARKDOWN")
    pl.Config.set_fmt_str_lengths(60)
    pl.Config.set_tbl_hide_dataframe_shape(True)
    pl.Config.set_tbl_hide_column_data_types(True)
    main()
