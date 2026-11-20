# Appendix E. The example files

Everything in `manual/examples/bookshop-trail/`, and the chapter that uses it. In the web edition,
a file name opens the file in a viewer, highlighted (a spreadsheet as a table), with buttons to copy
it, download it, and open it in the Turtle Editor Viewer where that is useful. In the EPUB and the
PDF, it links to the file as it is on tarka's main branch, which opens in the browser and saves. To
have them all, [browse the folder on GitHub](https://github.com/pwin/tarka/tree/main/manual/examples/bookshop-trail), or [download tarka as a ZIP](https://github.com/pwin/tarka/archive/refs/heads/main.zip) and look
in `manual/examples/bookshop-trail/`.

The **open** links load a file into the [Turtle Editor Viewer](https://semantechs.co.uk/turtle-editor-viewer/), as the Semantechs courses
load theirs, so it can be read as a diagram, queried and validated in the browser, with nothing to
install. Shapes open in a tab of their own, beside the courses' whole trail in one file, and
already chosen for validation: press **Validate**, and the editor reports what is written beside the
link. A query opens the editor with the trail loaded: paste the query into the SPARQL panel and
press **Execute Query**. The trail is the course's graph, which tarka's mappings reproduce, so a
check finds in it what it finds in their output.

## reference/

Copied unchanged from the Semantechs courses, which are MIT-licensed by the same author. Files 04 and
06 to 09 are the course's graph, which the spreadsheets were written from and the mappings must
reproduce.

| File | What it is | Chapters | In the editor |
|---|---|---|---|
| [`01-vocabulary.ttl`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/reference/01-vocabulary.ttl) | the Bookshop Trail ontology (OWL 2 DL) | 3, 12 | [open](https://semantechs.co.uk/turtle-editor-viewer/?dot=https%3A%2F%2Fraw.githubusercontent.com%2Fpwin%2Ftarka%2Fmain%2Fmanual%2Fexamples%2Fbookshop-trail%2Freference%2F01-vocabulary.ttl) |
| [`02-genres.ttl`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/reference/02-genres.ttl) | reference data: the genres | 10, 11, 12 | [open](https://semantechs.co.uk/turtle-editor-viewer/?dot=https%3A%2F%2Fraw.githubusercontent.com%2Fpwin%2Ftarka%2Fmain%2Fmanual%2Fexamples%2Fbookshop-trail%2Freference%2F02-genres.ttl) |
| [`03-places.ttl`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/reference/03-places.ttl) | reference data: the places | 10, 11, 12 | [open](https://semantechs.co.uk/turtle-editor-viewer/?dot=https%3A%2F%2Fraw.githubusercontent.com%2Fpwin%2Ftarka%2Fmain%2Fmanual%2Fexamples%2Fbookshop-trail%2Freference%2F03-places.ttl) |
| [`04-bookshops.ttl`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/reference/04-bookshops.ttl) | the course's graph: the bookshops | 3, 11 | [open](https://semantechs.co.uk/turtle-editor-viewer/?dot=https%3A%2F%2Fraw.githubusercontent.com%2Fpwin%2Ftarka%2Fmain%2Fmanual%2Fexamples%2Fbookshop-trail%2Freference%2F04-bookshops.ttl) |
| [`05-people.ttl`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/reference/05-people.ttl) | reference data: the authors and publishers | 10, 11, 12 | [open](https://semantechs.co.uk/turtle-editor-viewer/?dot=https%3A%2F%2Fraw.githubusercontent.com%2Fpwin%2Ftarka%2Fmain%2Fmanual%2Fexamples%2Fbookshop-trail%2Freference%2F05-people.ttl) |
| [`06-books.ttl`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/reference/06-books.ttl) | the course's graph: works, translations and series | 3, 11 | [open](https://semantechs.co.uk/turtle-editor-viewer/?dot=https%3A%2F%2Fraw.githubusercontent.com%2Fpwin%2Ftarka%2Fmain%2Fmanual%2Fexamples%2Fbookshop-trail%2Freference%2F06-books.ttl) |
| [`07-events.ttl`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/reference/07-events.ttl) | the course's graph: the events | 3, 11 | [open](https://semantechs.co.uk/turtle-editor-viewer/?dot=https%3A%2F%2Fraw.githubusercontent.com%2Fpwin%2Ftarka%2Fmain%2Fmanual%2Fexamples%2Fbookshop-trail%2Freference%2F07-events.ttl) |
| [`08-trail.ttl`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/reference/08-trail.ttl) | the course's graph: the trail's segments | 3, 11 | [open](https://semantechs.co.uk/turtle-editor-viewer/?dot=https%3A%2F%2Fraw.githubusercontent.com%2Fpwin%2Ftarka%2Fmain%2Fmanual%2Fexamples%2Fbookshop-trail%2Freference%2F08-trail.ttl) |
| [`09-stock.ttl`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/reference/09-stock.ttl) | the course's graph: the stock records | 3, 11 | [open](https://semantechs.co.uk/turtle-editor-viewer/?dot=https%3A%2F%2Fraw.githubusercontent.com%2Fpwin%2Ftarka%2Fmain%2Fmanual%2Fexamples%2Fbookshop-trail%2Freference%2F09-stock.ttl) |
| [`s64-the-whole-trail-in-one-shapes-graph.ttl`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/reference/s64-the-whole-trail-in-one-shapes-graph.ttl) | the SHACL course's shapes for the whole trail | 10 | [open](https://semantechs.co.uk/turtle-editor-viewer/?dot=https%3A%2F%2Fraw.githubusercontent.com%2Fpwin%2FSHACL_Course%2Fmain%2Fdata%2Fbookshop-trail-1.1.ttl&shapes=https%3A%2F%2Fraw.githubusercontent.com%2Fpwin%2Ftarka%2Fmain%2Fmanual%2Fexamples%2Fbookshop-trail%2Freference%2Fs64-the-whole-trail-in-one-shapes-graph.ttl): six warnings, no violations |
| [`q01-every-bookshop-on-the-trail.rq`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/reference/q01-every-bookshop-on-the-trail.rq) | the SPARQL course's first query | 11 | [open the trail](https://semantechs.co.uk/turtle-editor-viewer/?dot=https%3A%2F%2Fraw.githubusercontent.com%2Fpwin%2FSPARQL_Course%2Fmain%2Fdata%2Fbookshop-trail-1.1.ttl) |
| [`NOTICE.md`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/reference/NOTICE.md) | where these files come from, and their licence | 3 |  |

## data/

The spreadsheets and the prepared tables. The Parquet file downloads; the rest are text.

| File | Rows | Chapters |
|---|---|---|
| [`bookshops.csv`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/data/bookshops.csv), [`events.csv`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/data/events.csv), [`works.csv`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/data/works.csv), [`stock.csv`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/data/stock.csv), [`segments.csv`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/data/segments.csv) | the committee's spreadsheets, written by `tools/make-spreadsheets.py` | 3 to 5 |
| [`faulty/bookshops.csv`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/data/faulty/bookshops.csv) | `bookshops.csv` with five mistakes | 10, 11 |
| [`prepared/bookshops.csv`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/data/prepared/bookshops.csv), [`prepared/segments.csv`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/data/prepared/segments.csv) | tables for the bOTTR maps, written by `python/prepare.py` | 8, 9 |
| [`prepared/shop-stock.csv`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/data/prepared/shop-stock.csv), [`prepared/shop-stock.parquet`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/data/prepared/shop-stock.parquet) | each shop's stock as four lists, for `tt:ShopStock` | 7, 9 |

## Mappings

The TARQL queries run over a table's rows, not over a graph, so they are for reading here; a bOTTR
map is Turtle, and opens as one.

| File | What it maps | Chapter |
|---|---|---|
| [`tarql/bookshops-first.rq`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/tarql/bookshops-first.rq) | shops: name and town | 4 |
| [`tarql/bookshops.rq`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/tarql/bookshops.rq) | shops, in full, with their points | 4 |
| [`tarql/stocks.rq`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/tarql/stocks.rq) | the titles a shop stocks, with `apf:strSplit` | 5 |
| [`tarql/stocks-split.rq`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/tarql/stocks-split.rq) | the same, for `--split` | 5 |
| [`tarql/stock.rq`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/tarql/stock.rq) | stock records | 5 |
| [`tarql/events.rq`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/tarql/events.rq) | events: UK dates, pound signs | 5 |
| [`tarql/works.rq`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/tarql/works.rq) | works: optional columns, translations, tagged titles | 5 |
| [`tarql/functions-tour.rq`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/tarql/functions-tour.rq) | a guide to the trail, through Jena's, XPath's, SPIN's and GeoSPARQL's functions | 5 |
| [`ottr/base.stottr`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/ottr/base.stottr) | `tt:Geometry`, `tt:Named` | 6 |
| [`ottr/trail.stottr`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/ottr/trail.stottr) | `tt:Bookshop`, `tt:Stocks`, `tt:StockRecord`, `tt:Segment`, `tt:Connects` | 6 |
| [`ottr/rows.stottr`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/ottr/rows.stottr) | `tt:ShopStock`, with `cross` and `zipMin` | 6, 7 |
| [`ottr-mistakes/events.stottr`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/ottr-mistakes/events.stottr) | a draft with five lint errors and a warning | 6 |
| [`instances/two-rivers.stottr`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/instances/two-rivers.stottr) | a new shop, written by hand | 7 |
| [`bottr/bookshops.bottr.ttl`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/bottr/bookshops.bottr.ttl) | shops, through `tt:Bookshop` ([open](https://semantechs.co.uk/turtle-editor-viewer/?dot=https%3A%2F%2Fraw.githubusercontent.com%2Fpwin%2Ftarka%2Fmain%2Fmanual%2Fexamples%2Fbookshop-trail%2Fbottr%2Fbookshops.bottr.ttl)) | 8 |
| [`bottr/segments.bottr.ttl`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/bottr/segments.bottr.ttl) | trail segments, through `tt:Segment` ([open](https://semantechs.co.uk/turtle-editor-viewer/?dot=https%3A%2F%2Fraw.githubusercontent.com%2Fpwin%2Ftarka%2Fmain%2Fmanual%2Fexamples%2Fbookshop-trail%2Fbottr%2Fsegments.bottr.ttl)) | 8 |
| [`bottr/connections.bottr.ttl`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/bottr/connections.bottr.ttl) | `bs:connectsTo`, from the segments' RDF ([open](https://semantechs.co.uk/turtle-editor-viewer/?dot=https%3A%2F%2Fraw.githubusercontent.com%2Fpwin%2Ftarka%2Fmain%2Fmanual%2Fexamples%2Fbookshop-trail%2Fbottr%2Fconnections.bottr.ttl)) | 8 |
| [`bottr/connections-endpoint.bottr.ttl`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/bottr/connections-endpoint.bottr.ttl) | the same, from a store's SPARQL endpoint ([open](https://semantechs.co.uk/turtle-editor-viewer/?dot=https%3A%2F%2Fraw.githubusercontent.com%2Fpwin%2Ftarka%2Fmain%2Fmanual%2Fexamples%2Fbookshop-trail%2Fbottr%2Fconnections-endpoint.bottr.ttl)) | 13 |

## Checks

| File | What it checks | Chapter | In the editor |
|---|---|---|---|
| [`shapes/trail-extra.ttl`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/shapes/trail-extra.ttl) | events, stock records, a shop's answers and its year | 10 | [open](https://semantechs.co.uk/turtle-editor-viewer/?dot=https%3A%2F%2Fraw.githubusercontent.com%2Fpwin%2FSHACL_Course%2Fmain%2Fdata%2Fbookshop-trail-1.1.ttl&shapes=https%3A%2F%2Fraw.githubusercontent.com%2Fpwin%2Ftarka%2Fmain%2Fmanual%2Fexamples%2Fbookshop-trail%2Fshapes%2Ftrail-extra.ttl): conforms |
| [`sparql/counts.rq`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/sparql/counts.rq) | how many of each class | 11 | [open the trail](https://semantechs.co.uk/turtle-editor-viewer/?dot=https%3A%2F%2Fraw.githubusercontent.com%2Fpwin%2FSPARQL_Course%2Fmain%2Fdata%2Fbookshop-trail-1.1.ttl) |
| [`sparql/cross-file-checks.rq`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/sparql/cross-file-checks.rq) | disagreements between files, unknown towns | 11 | [open the trail](https://semantechs.co.uk/turtle-editor-viewer/?dot=https%3A%2F%2Fraw.githubusercontent.com%2Fpwin%2FSPARQL_Course%2Fmain%2Fdata%2Fbookshop-trail-1.1.ttl) |
| [`sparql/every-shop-answers.rq`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/sparql/every-shop-answers.rq) | an `ASK` for shops that skip a yes/no question | 11 | [open the trail](https://semantechs.co.uk/turtle-editor-viewer/?dot=https%3A%2F%2Fraw.githubusercontent.com%2Fpwin%2FSPARQL_Course%2Fmain%2Fdata%2Fbookshop-trail-1.1.ttl) |

## Scripts

| File | Does | Chapter |
|---|---|---|
| [`tools/make-spreadsheets.py`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/tools/make-spreadsheets.py) | writes `data/*.csv` from the course's graph | 3 |
| [`python/prepare.py`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/python/prepare.py) | prepares the tables for OTTR and bOTTR, with Polars | 9 |
| [`python/frames.py`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/python/frames.py) | maps a frame and a Parquet file from Python | 9 |
| [`python/validate.py`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/python/validate.py) | validates with the mapping's shapes (SHACL_Engine) and with s64 and the reference data (HOLOS) | 10 |
| [`tools/sparql.py`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/tools/sparql.py) | runs a query over files and the reference data; `--expect-none` makes a gate of it | 11 |
| [`tools/compare.py`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/tools/compare.py) | compares what tarka made with the course's graph, value for value | 11 |
| [`pipeline/trail.yml`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/pipeline/trail.yml) | the whole transformation as a GitHub Actions workflow | 14 |
| [`check.py`](https://raw.githubusercontent.com/pwin/tarka/main/manual/examples/bookshop-trail/check.py) | runs every example in the manual, in order, and checks each result | 2 |

`check.py` needs `tarka` on the `PATH` (or `TARKA=path/to/tarka`) and the Python packages
`tarka-rdf`, `polars`, `holosdb` and `shacl`. Set `TARKA_EXAMPLES_OUT` to keep the output on
another drive, and `HOLOS_SERVER=path/to/holos-server` to run Chapter 13's store steps too. On
this edition's machine, it runs 39 checks.

---

| [← Appendix D. Troubleshooting](d-troubleshooting.md) | [The tarka Manual](../README.md) |
|---|---|
