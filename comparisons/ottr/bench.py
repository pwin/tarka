"""tarka, maplib and Lutra on the same OTTR templates and rows: the inputs, a comparison of
the RDF each makes from the 8-row fixture, then timed runs (each in its own process) and a
table of medians. Lutra is included when LUTRA_JAR is the path of its jar; it is a hundred
times slower here, so it runs --lutra-runs times (1 by default) rather than --runs. A run
that fails (Lutra out of memory, or stopped after LUTRA_TIMEOUT seconds) is recorded as
such, and the others go on.

    python comparisons/ottr/bench.py [--rows 200000 1000000] [--runs 3] [--lutra-runs 1] [--out DIR]
    python comparisons/ottr/bench.py --summarize comparisons/ottr/results/2026-10-23-afternoon.jsonl

DIR (target/comparisons/ottr by default) gets the inputs and results.jsonl.
"""

import argparse
import json
import os
import statistics
import subprocess
import sys
from pathlib import Path

here = Path(__file__).resolve().parent
# what each tool writes, and what it is timed on
suffix = {"tarka": "nt", "maplib": "nt", "lutra": "ttl"}
targets = {"tarka": ["file", "memory"], "maplib": ["file", "memory"], "lutra": ["file"]}


def summarize(path: Path) -> None:
    rows = [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
    print(f"{'rows':>9}  {'target':6}  {'tool':6}  {'median s':>8}  {'runs (s)':24}  {'triples':>10}  {'peak MB':>7}")
    for key in sorted({(r["rows"], r["target"], r["tool"]) for r in rows}):
        runs = [r for r in rows if (r["rows"], r["target"], r["tool"]) == key and "error" not in r]
        for r in rows:
            if (r["rows"], r["target"], r["tool"]) == key and "error" in r:
                print(f"{key[0]:>9,}  {key[1]:6}  {key[2]:6}  failed: {r['error'][:200]}")
        if not runs:
            continue
        seconds = sorted(r["seconds"] for r in runs)
        print(
            f"{key[0]:>9,}  {key[1]:6}  {key[2]:6}  {statistics.median(seconds):>8.2f}  {', '.join(map(str, seconds)):24}"
            f"  {runs[0]['triples']:>10,}  {max(r['peak_mb'] for r in runs):>7,}"
        )


def run(*args: str) -> str:
    """One run's line of JSON; a failed run's says why."""
    done = subprocess.run([sys.executable, str(here / "run.py"), *args], capture_output=True, text=True)
    if done.returncode != 0:
        tool, target, _, frame = args[:4]
        rows = int(Path(frame).stem.removeprefix("orders_"))
        why = " ".join(done.stderr.split())
        return json.dumps({"tool": tool, "target": target, "rows": rows, "error": why}) + "\n"
    return done.stdout


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--rows", type=int, nargs="+", default=[200_000, 1_000_000])
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--lutra-runs", type=int, default=1)
    parser.add_argument("--out", type=Path, default=here.parents[1] / "target" / "comparisons" / "ottr")
    parser.add_argument("--summarize", type=Path)
    args = parser.parse_args()
    if args.summarize:
        return summarize(args.summarize)

    tools = ["tarka", "maplib"] + (["lutra"] if os.environ.get("LUTRA_JAR") else [])
    if "lutra" not in tools:
        print("LUTRA_JAR is not set: leaving Lutra out")
    out = args.out
    out.mkdir(parents=True, exist_ok=True)
    subprocess.run([sys.executable, str(here / "data.py"), str(out), *map(str, args.rows)], check=True)
    lib = str(out / "lib")

    print("The RDF each makes from the 8-row fixture:", flush=True)
    made = {}
    for tool in tools:
        made[tool] = out / f"{tool}.{suffix[tool]}"
        line = run(tool, "file", lib, str(out / "orders_8.parquet"), str(made[tool]), "keep")
        if "error" in line:
            sys.exit(f"{tool} could not map the 8-row fixture: {json.loads(line)['error']}")
    # Lutra, the reference implementation, against each of the others
    reference = "lutra" if "lutra" in tools else "tarka"
    for tool in tools:
        if tool != reference:
            subprocess.run([sys.executable, str(here / "compare.py"), str(made[reference]), str(made[tool])], check=True)

    results = out / "results.jsonl"
    results.write_text("")
    for rows in args.rows:
        for i in range(max(args.runs, args.lutra_runs if "lutra" in tools else 0)):
            # interleaved, so that a slow spell on the machine does not land on one tool
            for tool in tools:
                if i >= (args.lutra_runs if tool == "lutra" else args.runs):
                    continue
                for target in targets[tool]:
                    line = run(tool, target, lib, str(out / f"orders_{rows}.parquet"), str(out / f"out_{tool}.{suffix[tool]}"))
                    with results.open("a") as f:
                        f.write(line)
                    print(f"{rows:,} rows, run {i + 1}: {line.strip()}", flush=True)
    print()
    summarize(results)


main()
