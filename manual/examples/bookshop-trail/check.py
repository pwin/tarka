"""Runs every example in the tarka manual, in the order the chapters give them, and checks
what each one does: the exit status, and what it made against the course's own graph.

    python check.py              # needs tarka on PATH (or TARKA=path/to/tarka), and the
                                 # Python packages tarka-rdf, polars, holosdb and shacl

Output goes to out/ (written afresh). With TARKA_EXAMPLES_OUT=E:/somewhere, out/ is made a
directory junction to that folder, so large or temporary output lives on another drive while
the commands keep their paths. The script stops at the first check that fails.
"""

import os
import shutil
import subprocess
import sys
from pathlib import Path

import holosdb

HERE = Path(__file__).resolve().parent
OUT = HERE / "out"
TARKA = os.environ.get("TARKA", "tarka")
PY = sys.executable
LISTS = ["--list", "records", ";", "--list", "books", ";", "--list", "copies", ";", "--list", "prices", ";"]
passed = 0


def step(title: str, command: list[str], status: int = 0, needle: str | None = None) -> str:
    """Runs a command from the example's directory; checks its exit status, and that its
    output (standard output and error together) contains `needle`."""
    global passed
    done = subprocess.run(command, cwd=HERE, capture_output=True, text=True, encoding="utf-8")
    said = done.stdout + done.stderr
    if done.returncode != status or (needle and needle not in said):
        print(f"FAILED: {title}\n  {' '.join(command)}\n  exit {done.returncode}, expected {status}\n{said}")
        sys.exit(1)
    passed += 1
    print(f"ok  {title}")
    return said


def tarka(title: str, *args: str, status: int = 0, needle: str | None = None) -> str:
    return step(title, [TARKA, *args], status, needle)


def python(title: str, *args: str, status: int = 0, needle: str | None = None) -> str:
    return step(title, [PY, *args], status, needle)


def same_triples(a: str, b: list[str]) -> None:
    """The file a holds exactly the triples of the files b."""
    global passed
    def triples(files):
        store = holosdb.Store()
        for f in files:
            store.load(str(HERE / f))
        return set(store.query("CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }"))
    left, right = triples([a]), triples(b)
    if left != right:
        print(f"FAILED: {a} and {' + '.join(b)} differ: {len(left - right)} and {len(right - left)} triples")
        sys.exit(1)
    passed += 1
    print(f"ok  {a} = {' + '.join(b)} ({len(left)} triples)")


def same_values(a: str, b: list[str]) -> None:
    """The file a holds the triples of the files b, literals compared by value: bOTTR writes a
    typed literal as the cell has it ("80.0"), where a TARQL cast writes its canonical form ("80")."""
    global passed
    store = holosdb.Store()
    store.load(str(HERE / a), graph="urn:a")
    for f in b:
        store.load(str(HERE / f), graph="urn:b")
    same = "(sameTerm(?o, ?o2) || (isLiteral(?o) && isLiteral(?o2) && datatype(?o) = datatype(?o2) && ?o = ?o2))"
    count = lambda x, y: len(store.query(
        f"SELECT * WHERE {{ GRAPH <{x}> {{ ?s ?p ?o }} FILTER NOT EXISTS {{ GRAPH <{y}> {{ ?s ?p ?o2 }} FILTER {same} }} }}"))
    left, right = count("urn:a", "urn:b"), count("urn:b", "urn:a")
    if left or right:
        print(f"FAILED: {a} and {' + '.join(b)} differ in value: {left} and {right} triples")
        sys.exit(1)
    passed += 1
    print(f"ok  {a} = {' + '.join(b)}, value for value")


def fresh_out() -> None:
    """Empties out/. With TARKA_EXAMPLES_OUT set (a folder on another drive, say), out/ is a
    directory junction to it, so the commands keep their paths and the files go there."""
    elsewhere = os.environ.get("TARKA_EXAMPLES_OUT")
    if os.path.isjunction(OUT) or OUT.is_symlink():
        os.rmdir(OUT)  # removes the link, not what it points to
    else:
        shutil.rmtree(OUT, ignore_errors=True)
    if not elsewhere:
        OUT.mkdir()
        return
    target = Path(elsewhere).resolve()
    shutil.rmtree(target, ignore_errors=True)
    target.mkdir(parents=True)
    if os.name == "nt":
        subprocess.run(["cmd", "/c", "mklink", "/J", str(OUT), str(target)], check=True, capture_output=True)
    else:
        OUT.symlink_to(target, target_is_directory=True)


def main() -> None:
    fresh_out()

    # Chapter 3: the spreadsheets, and the tables prepared for OTTR
    python("the spreadsheets", "tools/make-spreadsheets.py")
    python("the prepared tables (Polars)", "python/prepare.py")

    # Chapters 4 and 5: TARQL
    tarka("first mapping", "run", "-q", "tarql/bookshops-first.rq", "-i", "data/bookshops.csv", "-o", "out/bookshops-first.ttl")
    tarka("a sample of five rows", "run", "-q", "tarql/bookshops.rq", "-i", "data/bookshops.csv", "--test", needle="bt:shop-bookbarrow")
    tarka("bookshops", "run", "-q", "tarql/bookshops.rq", "-i", "data/bookshops.csv", "-o", "out/bookshops.ttl")
    tarka("titles stocked, apf:strSplit", "run", "-q", "tarql/stocks.rq", "-i", "data/bookshops.csv", "-o", "out/stocks.ttl")
    tarka("titles stocked, --split", "run", "-q", "tarql/stocks-split.rq", "-i", "data/bookshops.csv",
          "--split", "stocks", "book", ";", "--ntriples", "-o", "out/stocks-split.nt")
    same_triples("out/stocks-split.nt", ["out/stocks.ttl"])
    tarka("stock records", "run", "-q", "tarql/stock.rq", "-i", "data/stock.csv", "-o", "out/stock.ttl")
    tarka("events", "run", "-q", "tarql/events.rq", "-i", "data/events.csv", "-o", "out/events.ttl")
    tarka("works", "run", "-q", "tarql/works.rq", "-i", "data/works.csv", "-o", "out/works.ttl")
    tarka("functions tour", "run", "-q", "tarql/functions-tour.rq", "-i", "data/bookshops.csv", "--test", "3",
          needle="BOOKBARROW")
    python("TARQL output = the course's graph", "tools/compare.py", "out/bookshops.ttl", "out/stocks.ttl",
           "out/stock.ttl", "out/events.ttl", "out/works.ttl", needle="base direction (RDF 1.2), which tarka does not write: 6")

    # Chapters 6 to 8: OTTR, lint, expand, bOTTR
    tarka("the library is clean", "lint", "-l", "ottr/", needle="8 templates: 0 errors, 0 warnings")
    tarka("lint finds the draft's mistakes", "lint", "-l", "ottr/", "-l", "ottr-mistakes/", status=1,
          needle="10 templates: 5 errors, 1 warning")
    tarka("stock from CSV, zipMin and cross", "run", "-l", "ottr/", "-T", "tt:ShopStock", "-i", "data/prepared/shop-stock.csv",
          *LISTS, "-o", "out/shop-stock-csv.ttl")
    tarka("stock from Parquet", "run", "-l", "ottr/", "-T", "tt:ShopStock", "-i", "data/prepared/shop-stock.parquet",
          "-o", "out/shop-stock-parquet.ttl")
    same_triples("out/shop-stock-csv.ttl", ["out/stocks.ttl", "out/stock.ttl"])
    same_triples("out/shop-stock-parquet.ttl", ["out/shop-stock-csv.ttl"])
    tarka("two rivers, by hand", "expand", "-l", "ottr/", "instances/two-rivers.stottr", "-o", "out/two-rivers.ttl")
    tarka("bookshops through bOTTR", "bottr", "-l", "ottr/", "bottr/bookshops.bottr.ttl", "-o", "out/bookshops-bottr.ttl")
    same_values("out/bookshops-bottr.ttl", ["out/bookshops.ttl", "out/stocks.ttl"])
    tarka("segments through bOTTR", "bottr", "-l", "ottr/", "bottr/segments.bottr.ttl", "-o", "out/segments.ttl")
    tarka("connections from the segments' graph", "bottr", "-l", "ottr/", "bottr/connections.bottr.ttl", "-o", "out/connections.ttl")
    python("OTTR and bOTTR output = the course's graph", "tools/compare.py", "out/bookshops-bottr.ttl",
           "out/shop-stock-parquet.ttl", "out/segments.ttl", "out/connections.ttl", needle="subjects missed: 0")

    # Chapter 9: Python and Polars
    python("frames and Parquet from Python", "python/frames.py")
    same_triples("out/bookshops-frame.ttl", ["out/bookshops.ttl"])
    same_triples("out/shop-stock-frame.nt", ["out/shop-stock-csv.ttl"])

    # Chapters 10 to 12: validation
    tarka("shapes from the mapping", "shapes", "-q", "tarql/bookshops.rq", "--base", "https://example.org/bookshop-trail/shapes#",
          "-o", "out/bookshops.shapes.ttl")
    tarka("good data conforms", "run", "-q", "tarql/bookshops.rq", "-i", "data/bookshops.csv", "--validate",
          "--shapes", "shapes/trail-extra.ttl", "-o", "out/bookshops-validated.ttl")
    tarka("faulty data does not", "run", "-q", "tarql/bookshops.rq", "-i", "data/faulty/bookshops.csv", "--validate",
          "--shapes", "shapes/trail-extra.ttl", "--report", "out/faulty-report.ttl", "-o", "out/faulty.ttl",
          status=3, needle="does not conform")
    rest = ["out/stocks.ttl", "out/stock.ttl", "out/events.ttl", "out/works.ttl", "out/segments.ttl", "out/connections.ttl"]
    python("no violations of s64", "python/validate.py", "out/bookshops.ttl", *rest, needle="0 violations, 6 warnings")
    python("the faults, found by s64", "python/validate.py", "out/faulty.ttl", *rest, status=1, needle="4 violations, 6 warnings")
    python("cross-file checks find nothing", "tools/sparql.py", "sparql/cross-file-checks.rq", "out/bookshops.ttl",
           *rest, "--expect-none", needle="0 rows")
    python("cross-file checks find the unknown town", "tools/sparql.py", "sparql/cross-file-checks.rq", "out/faulty.ttl",
           *rest, "--expect-none", status=1, needle="bt:place-wigton")
    python("every shop answers", "tools/sparql.py", "sparql/every-shop-answers.rq", "out/bookshops.ttl",
           "--no-reference", "--expect-none", needle="ASK: no")

    # Chapter 13: a store, when there is one to start (HOLOS_SERVER=path/to/holos-server)
    if os.environ.get("HOLOS_SERVER"):
        store_steps()

    print(f"\n{passed} checks passed")


def store_steps() -> None:
    """Loads the segments into a HOLOS server, and reads the connections back from it."""
    import time
    import urllib.request
    server = subprocess.Popen([os.environ["HOLOS_SERVER"], "--listen", "127.0.0.1:7878"],
                              stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        for _ in range(100):
            try:
                urllib.request.urlopen("http://127.0.0.1:7878/health", timeout=1)
                break
            except OSError:
                time.sleep(0.1)
        tarka("segments into the store", "bottr", "-l", "ottr/", "bottr/segments.bottr.ttl", "--post",
              "http://127.0.0.1:7878/graph", "--graph", "https://example.org/bookshop-trail/graph/trail", "--stats",
              needle="answered 201")
        tarka("connections from the store", "bottr", "-l", "ottr/", "bottr/connections-endpoint.bottr.ttl",
              "-o", "out/connections-endpoint.ttl")
        same_triples("out/connections-endpoint.ttl", ["out/connections.ttl"])
    finally:
        server.terminate()


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    main()
