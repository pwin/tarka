"""One timed run, in its own process: TOOL (tarka, maplib or lutra) maps the retail rows
through the retail templates, to a file (TARGET file) or into memory (TARGET memory:
tarka's triplify, a frame of distinct triples; maplib's map, into its store). Prints one
line of JSON: the time, the triples made and the peak memory.

    python comparisons/ottr/run.py TOOL TARGET LIB_DIR FRAME.parquet OUT [keep]

tarka and maplib map the frame in FRAME.parquet, and write N-Triples; the time and memory
are this process's, with the frame loaded first. Lutra (java -jar $LUTRA_JAR, files only)
reads the same rows from the CSV file beside the frame, through the bOTTR map beside it,
and writes Turtle; the time is its process's from start to exit, and the memory its peak.
The triples counted are the lines of an N-Triples file, the distinct triples of tarka's
frame and maplib's store, and the distinct triples of Lutra's file (loaded into HOLOS).
Lutra is stopped after LUTRA_TIMEOUT seconds (3600 by default). The file is removed
afterwards unless `keep` is given.
"""

import json
import os
import sys
import tempfile
import time

import polars as pl
import psutil

tool, target, library, frame, out = sys.argv[1:6]
keep = sys.argv[6:] == ["keep"]
root = "http://example.com/retail/template#OrderRow"
files = ["base.stottr", "domain.stottr", "rows.stottr"]
df = pl.read_parquet(frame)
if tool == "tarka":
    import tarka_rdf
elif tool == "maplib":
    from maplib import Model
elif tool == "lutra":
    if target != "file":
        raise SystemExit("Lutra writes files only")
    jar = os.environ.get("LUTRA_JAR") or sys.exit("set LUTRA_JAR to the path of Lutra's jar")
    timeout = float(os.environ.get("LUTRA_TIMEOUT", 3600))
else:
    raise SystemExit(f"unknown tool {tool}")
proc = psutil.Process()
before = proc.memory_info().rss


def peak_of(p: psutil.Process) -> int:
    mem = p.memory_info()
    return getattr(mem, "peak_wset", None) or getattr(mem, "peak_rss", None) or mem.rss


start = time.perf_counter()
if tool == "tarka":
    m = tarka_rdf.Mapping.ottr([f"{library}/{f}" for f in files], ["rt:OrderRow"])
    if target == "file":
        m.write(df, out, format="ntriples")
    else:
        triples = m.triplify(df)
elif tool == "maplib":
    m = Model()
    for f in files:
        with open(f"{library}/{f}", encoding="utf-8") as text:
            m.add_template(text.read())
    m.map(root, df)
    if target == "file":
        m.write(out, format="ntriples")
else:
    bottr = frame.removesuffix(".parquet") + ".bottr.ttl"
    command = ["java", "-jar", jar, "-l", library, "-L", "stottr", "-I", "bottr", "-m", "expand", "-o", out, bottr]
    with tempfile.TemporaryFile("w+", encoding="utf-8", errors="replace") as log:
        child = psutil.Popen(command, stdout=log, stderr=log)
        child_peak = 0
        while child.poll() is None:
            try:
                child_peak = max(child_peak, peak_of(child))
            except psutil.NoSuchProcess:
                pass
            if time.perf_counter() - start > timeout:
                child.kill()
                sys.exit(f"Lutra stopped after {timeout:.0f} s, at {child_peak / 2**20:.0f} MB")
            time.sleep(0.02)
        log.seek(0)
        messages = log.read()
    if child.returncode != 0 or "[ERROR]" in messages:
        sys.exit(f"Lutra failed (exit {child.returncode}, {time.perf_counter() - start:.0f} s, {child_peak / 2**20:.0f} MB): {messages.strip()[-1500:]}")
seconds = time.perf_counter() - start

if tool == "lutra":
    import holosdb

    # loaded into a HOLOS store, which holds each triple once
    store = holosdb.Store()
    store.load(out)
    count = len(store)
    peak, frame_mb = child_peak, None
else:
    peak, frame_mb = peak_of(proc), round(before / 2**20)
    if target == "file":
        with open(out, "rb") as f:
            count = sum(1 for _ in f)
    elif tool == "tarka":
        count = triples.height
    else:
        count = m.query("SELECT (COUNT(*) AS ?n) WHERE { ?s ?p ?o }")["n"][0]
if target == "file" and not keep:
    os.remove(out)
print(json.dumps({"tool": tool, "target": target, "rows": df.height, "seconds": round(seconds, 3),
                  "triples": int(count), "peak_mb": round(peak / 2**20), "frame_mb": frame_mb}))
