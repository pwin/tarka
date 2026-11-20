"""Writes the Bookshop Trail committee's spreadsheets (data/*.csv) from the course dataset
(reference/*.ttl), in the conventions a person keeps a spreadsheet in: short names instead of
IRIs, yes/no, UK dates, prices with a pound sign, a list of titles in one cell.

The manual's mappings turn these back into RDF; `check.py` compares what they make with the
course's own graph. Run from the example's directory:

    python tools/make-spreadsheets.py
"""

import csv
import sys
from pathlib import Path

import holosdb

HERE = Path(__file__).resolve().parents[1]
BT = "https://example.org/bookshop-trail/"
PREFIXES = """PREFIX bt: <https://example.org/bookshop-trail/>
PREFIX bs: <https://example.org/bookshop-trail/schema#>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
PREFIX dct: <http://purl.org/dc/terms/>
PREFIX wgs84: <http://www.w3.org/2003/01/geo/wgs84_pos#>
"""


def slug(iri: str, kind: str) -> str:
    """bt:shop-inkwell -> inkwell"""
    return iri.removeprefix(BT).removeprefix(kind + "-")


def yes_no(value: str) -> str:
    return {"true": "yes", "false": "no"}[value]


def uk_date(iso: str) -> str:
    year, month, day = iso.split("-")
    return f"{day}/{month}/{year}"


ESCAPES = {"t": "\t", "n": "\n", "r": "\r", '"': '"', "'": "'", "\\": "\\", "b": "\b", "f": "\f"}


def lexical(term: str | None) -> str:
    """The IRI or lexical form of a term in N-Triples form, as written in the data."""
    if term is None:
        return ""
    if term.startswith("<"):
        return term[1:-1]
    body, out, i = term[1 : term.rindex('"')], [], 0
    while i < len(body):
        c = body[i]
        if c != "\\":
            out.append(c)
            i += 1
        elif body[i + 1] in "uU":
            n = 4 if body[i + 1] == "u" else 8
            out.append(chr(int(body[i + 2 : i + 2 + n], 16)))
            i += 2 + n
        else:
            out.append(ESCAPES[body[i + 1]])
            i += 2
    return "".join(out)


def rows(store, query):
    return [{v: lexical(s.term(v)) for v in s.variables} for s in store.query(PREFIXES + query)]


def write(name, header, records):
    path = HERE / "data" / name
    with path.open("w", newline="", encoding="utf-8") as f:
        w = csv.writer(f, lineterminator="\n")
        w.writerow(header)
        w.writerows(records)
    print(f"{name}: {len(records)} rows")


def main() -> None:
    store = holosdb.Store()
    for f in sorted((HERE / "reference").glob("0*.ttl")):
        store.load(str(f))

    # Bookshops, with the titles each stocks in one cell
    shops = rows(store, """
        SELECT ?shop ?name ?town ?founded ?area ?staff ?genre ?second ?cafe ?web ?lat ?long
               (GROUP_CONCAT(STR(?work); SEPARATOR=" ") AS ?stocks)
        WHERE {
          ?shop a bs:Bookshop ; rdfs:label ?name ; bs:locatedIn ?town ; bs:founded ?founded ;
                bs:floorArea ?area ; bs:staffCount ?staff ; bs:specialises ?genre ;
                bs:sellsSecondHand ?second ; bs:hasCafe ?cafe ; wgs84:lat ?lat ; wgs84:long ?long .
          OPTIONAL { ?shop bs:website ?web }
          OPTIONAL { ?shop bs:stocks ?work }
        }
        GROUP BY ?shop ?name ?town ?founded ?area ?staff ?genre ?second ?cafe ?web ?lat ?long
        ORDER BY ?shop""")
    write(
        "bookshops.csv",
        ["shop", "name", "town", "founded", "floor_area", "staff", "specialism", "second_hand", "cafe", "website", "lat", "long", "stocks"],
        [
            [
                slug(r["shop"], "shop"), r["name"], slug(r["town"], "place"), r["founded"], r["area"], r["staff"],
                slug(r["genre"], "genre"), yes_no(r["second"]), yes_no(r["cafe"]), r["web"], r["lat"], r["long"],
                ";".join(sorted(slug(w, "book") for w in r["stocks"].split())),
            ]
            for r in shops
        ],
    )

    events = rows(store, """
        SELECT ?event ?shop ?date ?title ?kind ?author ?attendance ?price WHERE {
          ?event a bs:Event ; bs:heldAt ?shop ; bs:eventDate ?date ; rdfs:label ?title ;
                 bs:eventKind ?kind ; bs:featuring ?author ; bs:attendance ?attendance ;
                 bs:ticketPrice ?price .
        } ORDER BY ?event""")
    write(
        "events.csv",
        ["event", "shop", "date", "title", "kind", "featuring", "attendance", "ticket"],
        [
            [slug(r["event"], "event"), slug(r["shop"], "shop"), uk_date(r["date"]), r["title"], r["kind"],
             slug(r["author"], "author"), r["attendance"], "£" + r["price"]]
            for r in events
        ],
    )

    works = rows(store, """
        SELECT ?work ?title (LANG(?title) AS ?title_language) ?author ?publisher ?year ?genre ?pages ?rrp ?language ?isbn
               ?series ?position ?original ?translator WHERE {
          ?work a bs:Work ; dct:title ?title ; bs:author ?author ; bs:publishedBy ?publisher ;
                bs:publicationYear ?year ; bs:genre ?genre ; bs:pages ?pages ; bs:rrp ?rrp ;
                bs:originalLanguage ?language .
          OPTIONAL { ?work bs:isbn ?isbn }
          OPTIONAL { ?work bs:inSeries ?series ; bs:seriesPosition ?position }
          OPTIONAL { ?work bs:translationOf ?original }
          OPTIONAL { ?work bs:translatedBy ?translator }
        } ORDER BY ?work""")
    write(
        "works.csv",
        ["book", "title", "title_language", "author", "publisher", "year", "genre", "pages", "rrp", "language", "isbn",
         "series", "series_position", "translation_of", "translated_by"],
        [
            [slug(r["work"], "book"), r["title"], r["title_language"], slug(r["author"], "author"), slug(r["publisher"], "pub"),
             r["year"], slug(r["genre"], "genre"), r["pages"], r["rrp"], r["language"], r["isbn"],
             slug(r["series"], "series") if r["series"] else "", r["position"],
             slug(r["original"], "book") if r["original"] else "",
             slug(r["translator"], "author") if r["translator"] else ""]
            for r in works
        ],
    )

    stock = rows(store, """
        SELECT ?shop ?work ?copies ?price WHERE {
          ?record a bs:StockRecord ; bs:atShop ?shop ; bs:ofWork ?work ; bs:copies ?copies ;
                  bs:shelfPrice ?price .
        } ORDER BY ?shop ?work""")
    write(
        "stock.csv",
        ["shop", "book", "copies", "shelf_price"],
        [[slug(r["shop"], "shop"), slug(r["work"], "book"), r["copies"], r["price"]] for r in stock],
    )

    # A segment's line runs from one shop, through a waypoint, to the other: the spreadsheet
    # keeps the waypoint (the middle of the course's LINESTRING)
    segments = rows(store, """
        PREFIX geo: <http://www.opengis.net/ont/geosparql#>
        SELECT ?from ?to ?km ?waymarked ?wkt WHERE {
          ?segment a bs:TrailSegment ; bs:segmentFrom ?from ; bs:segmentTo ?to ;
                   bs:distanceKm ?km ; bs:waymarked ?waymarked ; geo:hasGeometry/geo:asWKT ?wkt .
        } ORDER BY ?from ?to""")

    def via(wkt: str) -> list[str]:
        points = wkt[wkt.index("(") + 1 : wkt.rindex(")")].split(",")
        return points[1].split()

    write(
        "segments.csv",
        ["from", "to", "km", "waymarked", "via_long", "via_lat"],
        [[slug(r["from"], "shop"), slug(r["to"], "shop"), r["km"], yes_no(r["waymarked"]), *via(r["wkt"])] for r in segments],
    )
    faulty()


# The mistakes a spreadsheet collects, one per row, for the validation chapter to find
MISTAKES = {
    "marginalia": ("town", "wigton"),                              # a typo: Wigtown is the town
    "borderprint": ("founded", "c.1972"),                          # "about 1972" is not a year
    "bookwyrm": ("staff", "0"),                                    # nobody works there?
    "bookbarrow": ("website", "www.example.org/shops/bookbarrow"),  # no https://
    "castle-steps": ("second_hand", "Yes"),                        # Yes, not yes
}


def faulty() -> None:
    with (HERE / "data" / "bookshops.csv").open(newline="", encoding="utf-8") as f:
        table = list(csv.DictReader(f))
    for row in table:
        if row["shop"] in MISTAKES:
            column, value = MISTAKES[row["shop"]]
            row[column] = value
    (HERE / "data" / "faulty").mkdir(exist_ok=True)
    with (HERE / "data" / "faulty" / "bookshops.csv").open("w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=list(table[0]), lineterminator="\n")
        w.writeheader()
        w.writerows(table)
    print(f"faulty/bookshops.csv: {len(MISTAKES)} mistakes in {len(table)} rows")


if __name__ == "__main__":
    sys.exit(main())
