"""Run ifc-lite's whole native pipeline (`lite-pipeline`) on every file that has a `compare` output.

    run_pipeline.py <corpus dir> <out dir> <spike binary>

Writes <out>/<name>.pipe.jsonl. aggregate.py then uses these as ifc-lite's results:
the per-element router calls in `compare` gave wrong volumes on some files
(Snowdon Towers, in feet) that the pipeline gets right.
"""
import os, subprocess, sys

corpus, out, binary = sys.argv[1:4]
for dp, _, fns in os.walk(corpus):
    for fn in fns:
        p = os.path.join(dp, fn)
        name = os.path.relpath(p, corpus).replace("/", "__")
        if not os.path.exists(f"{out}/{name}.jsonl"):
            continue
        with open(f"{out}/{name}.pipe.jsonl", "w") as fh:
            r = subprocess.run([binary, "lite-pipeline", p], stdout=fh, stderr=subprocess.PIPE, timeout=1200)
        print(name, r.returncode, flush=True)
