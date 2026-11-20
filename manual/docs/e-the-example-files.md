# Appendix E. The example files

Everything in `manual/examples/bookshop-trail/`, and the chapter that uses it.

## reference/

Copied unchanged from the Semantechs courses, which are MIT-licensed by the same author.

| File | What it is | Chapters |
|---|---|---|
| `01-vocabulary.ttl` | the Bookshop Trail ontology (OWL 2 DL) | 3, 12 |
| `02-genres.ttl`, `03-places.ttl`, `05-people.ttl` | reference data: genres, places, people and publishers | 10, 11, 12 |
| `04-bookshops.ttl`, `06-books.ttl`, `07-events.ttl`, `08-trail.ttl`, `09-stock.ttl` | the course's graph, which the spreadsheets were written from and the mappings must reproduce | 3, 11 |
| `s64-the-whole-trail-in-one-shapes-graph.ttl` | the SHACL course's shapes for the whole trail | 10 |
| `q01-every-bookshop-on-the-trail.rq` | the SPARQL course's first query | 11 |

## data/

| File | Rows | Chapters |
|---|---|---|
| `bookshops.csv`, `events.csv`, `works.csv`, `stock.csv`, `segments.csv` | the committee's spreadsheets, written by `tools/make-spreadsheets.py` | 3 to 5 |
| `faulty/bookshops.csv` | `bookshops.csv` with five mistakes | 10, 11 |
| `prepared/bookshops.csv`, `prepared/segments.csv` | tables for the bOTTR maps, written by `python/prepare.py` | 8, 9 |
| `prepared/shop-stock.csv`, `prepared/shop-stock.parquet` | each shop's stock as four lists, for `tt:ShopStock` | 7, 9 |

## Mappings

| File | What it maps | Chapter |
|---|---|---|
| `tarql/bookshops-first.rq` | shops: name and town | 4 |
| `tarql/bookshops.rq` | shops, in full, with their points | 4 |
| `tarql/stocks.rq` | the titles a shop stocks, with `apf:strSplit` | 5 |
| `tarql/stocks-split.rq` | the same, for `--split` | 5 |
| `tarql/stock.rq` | stock records | 5 |
| `tarql/events.rq` | events: UK dates, pound signs | 5 |
| `tarql/works.rq` | works: optional columns, translations, tagged titles | 5 |
| `tarql/functions-tour.rq` | a guide to the trail, through Jena's, XPath's, SPIN's and GeoSPARQL's functions | 5 |
| `ottr/base.stottr` | `tt:Geometry`, `tt:Named` | 6 |
| `ottr/trail.stottr` | `tt:Bookshop`, `tt:Stocks`, `tt:StockRecord`, `tt:Segment`, `tt:Connects` | 6 |
| `ottr/rows.stottr` | `tt:ShopStock`, with `cross` and `zipMin` | 6, 7 |
| `ottr-mistakes/events.stottr` | a draft with five lint errors and a warning | 6 |
| `instances/two-rivers.stottr` | a new shop, written by hand | 7 |
| `bottr/bookshops.bottr.ttl` | shops, through `tt:Bookshop` | 8 |
| `bottr/segments.bottr.ttl` | trail segments, through `tt:Segment` | 8 |
| `bottr/connections.bottr.ttl` | `bs:connectsTo`, from the segments' RDF | 8 |
| `bottr/connections-endpoint.bottr.ttl` | the same, from a store's SPARQL endpoint | 13 |

## Checks

| File | What it checks | Chapter |
|---|---|---|
| `shapes/trail-extra.ttl` | events, stock records, a shop's answers and its year | 10 |
| `sparql/counts.rq` | how many of each class | 11 |
| `sparql/cross-file-checks.rq` | disagreements between files, unknown towns | 11 |
| `sparql/every-shop-answers.rq` | an `ASK` for shops that skip a yes/no question | 11 |

## Scripts

| File | Does | Chapter |
|---|---|---|
| `tools/make-spreadsheets.py` | writes `data/*.csv` from the course's graph | 3 |
| `python/prepare.py` | prepares the tables for OTTR and bOTTR, with Polars | 9 |
| `python/frames.py` | maps a frame and a Parquet file from Python | 9 |
| `python/validate.py` | validates with the mapping's shapes (SHACL_Engine) and with s64 and the reference data (HOLOS) | 10 |
| `tools/sparql.py` | runs a query over files and the reference data; `--expect-none` makes a gate of it | 11 |
| `tools/compare.py` | compares what tarka made with the course's graph, value for value | 11 |
| `pipeline/trail.yml` | the whole transformation as a GitHub Actions workflow | 14 |
| `check.py` | runs every example in the manual, in order, and checks each result | 2 |

`check.py` needs `tarka` on the `PATH` (or `TARKA=path/to/tarka`) and the Python packages
`tarka-rdf`, `polars`, `holosdb` and `shacl`. Set `TARKA_EXAMPLES_OUT` to keep the output on
another drive, and `HOLOS_SERVER=path/to/holos-server` to run Chapter 13's store steps too. On
this edition's machine, it runs 39 checks.

---

| [← Appendix D. Troubleshooting](d-troubleshooting.md) | [The tarka Manual](../README.md) |
|---|---|
