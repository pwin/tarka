"""How tarka, maplib and Lutra treat three kinds of cell: a prefixed name, a value that is not
an IRI, and a null in a mandatory parameter of the root template. Lutra (when LUTRA_JAR is
the path of its jar) reads the same rows as CSV through a bOTTR map that declares `ex:`.

    python comparisons/ottr/edge_cases.py
"""

import os
import subprocess
import sys
import tempfile
from pathlib import Path

import polars as pl
import holosdb
import tarka_rdf
from maplib import Model

sys.stdout.reconfigure(encoding="utf-8")
library = """@prefix ex: <http://example.com/> . @prefix foaf: <http://xmlns.com/foaf/0.1/> . @prefix ottr: <http://ns.ottr.xyz/0.4/> .
ex:Person [ ottr:IRI ?person, ? ottr:IRI ?mbox, ?name ] :: {
    ottr:Triple(?person, foaf:name, ?name), ottr:Triple(?person, foaf:mbox, ?mbox) } .
"""
work = Path(tempfile.mkdtemp())
(work / "lib").mkdir()
(work / "lib" / "person.stottr").write_text(library, encoding="utf-8")
bottr = """@prefix ottr: <http://ns.ottr.xyz/0.4/> . @prefix ex: <http://example.com/> .
[] a ottr:InstanceMap ; ottr:template ex:Person ; ottr:source [ a ottr:H2Source ] ;
   ottr:query "SELECT * FROM CSVREAD('@@THIS_DIR@@/rows.csv')" ;
   ottr:argumentMaps ( [ ottr:type ottr:IRI ] [ ottr:type ottr:IRI ] [ ] ) .
"""


def lutra(df: pl.DataFrame) -> str:
    df.write_csv(work / "rows.csv")
    (work / "map.bottr.ttl").write_text(bottr, encoding="utf-8")
    out = work / "out.ttl"
    out.unlink(missing_ok=True)
    command = ["java", "-jar", os.environ["LUTRA_JAR"], "-l", str(work / "lib"), "-L", "stottr", "-I", "bottr", "-m", "expand"]
    done = subprocess.run([*command, "-o", str(out), str(work / "map.bottr.ttl")], capture_output=True, text=True, errors="replace")
    errors = [line for line in (done.stdout + done.stderr).splitlines() if line.startswith("[ERROR]")]
    store = holosdb.Store()
    if out.exists():
        store.load(str(out))
    subjects = sorted(f"<{row['s']}>" for row in store.query("SELECT DISTINCT ?s WHERE { ?s ?p ?o }"))
    said = f", and says: {errors[0]}" if errors else ""
    return f"{len(store)} triples, subjects {subjects} (exit {done.returncode}{said})"


def show(what: str, df: pl.DataFrame) -> None:
    print(f"{what}:")
    m = Model()
    m.add_template(library)
    try:
        m.map("http://example.com/Person", df)
        triples = m.query("SELECT ?s ?p ?o WHERE { ?s ?p ?o }")
        print(f"  maplib: {triples.height} triples, subjects {sorted(set(triples['s']))}")
    except Exception as e:
        print(f"  maplib: {type(e).__name__}: {str(e).splitlines()[0]}")
    try:
        triples = tarka_rdf.Mapping.ottr([str(work / "lib" / "person.stottr")], ["ex:Person"]).triplify(df)
        print(f"  tarka:  {triples.height} triples, subjects {sorted(set(triples['subject']))}")
    except Exception as e:
        print(f"  tarka:  {type(e).__name__}: {str(e).splitlines()[0]}")
    if os.environ.get("LUTRA_JAR"):
        print(f"  Lutra:  {lutra(df)}")


if not os.environ.get("LUTRA_JAR"):
    print("LUTRA_JAR is not set: leaving Lutra out\n")
show("a prefixed name in a cell (ex: is declared)", pl.DataFrame({"person": ["ex:ada"], "mbox": [None], "name": ["Ada"]}))
show(
    "a mailto: cell, with no mailto: prefix declared",
    pl.DataFrame({"person": ["http://example.com/ada"], "mbox": ["mailto:ada@example.com"], "name": ["Ada"]}),
)
show(
    "a cell that is not an IRI, for an optional parameter",
    pl.DataFrame({"person": ["http://example.com/ada", "http://example.com/bo"], "mbox": [None, "n/a"], "name": ["Ada", "Bo"]}),
)
show(
    "a null in the mandatory ?person",
    pl.DataFrame({"person": ["http://example.com/ada", None], "mbox": [None, None], "name": ["Ada", "Bo"]}),
)
