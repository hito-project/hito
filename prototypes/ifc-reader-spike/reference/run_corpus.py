"""Run `compare` (both Rust readers) and IfcOpenShell on every file of the corpus.

    run_corpus.py <corpus dir> <out dir> <spike binary> [max MB]

Writes <out>/<name>.jsonl (both readers) and <out>/<name>.ios.jsonl (IfcOpenShell).
A reader that crashes or exceeds the timeout is recorded in <out>/<name>.status.json.
"""
import json, os, subprocess, sys, time

corpus, out, binary = sys.argv[1:4]
max_mb = float(sys.argv[4]) if len(sys.argv) > 4 else 80
here = os.path.dirname(os.path.abspath(__file__))
os.makedirs(out, exist_ok=True)
TIMEOUT = 1200

files = []
for dp, _, fns in os.walk(corpus):
    for fn in fns:
        p = os.path.join(dp, fn)
        if fn.lower().endswith(".ifc") and os.path.getsize(p) <= max_mb * 1e6:
            files.append(p)

for p in sorted(files, key=os.path.getsize):
    name = os.path.relpath(p, corpus).replace("/", "__")
    status = {}
    for label, cmd, dest in [
        ("compare", [binary, "compare", p], f"{out}/{name}.jsonl"),
        ("ios", [sys.executable, f"{here}/ios.py", "elements", p, f"{out}/{name}.jsonl"], f"{out}/{name}.ios.jsonl"),
    ]:
        t = time.time()
        try:
            with open(dest, "w") as fh:
                r = subprocess.run(cmd, stdout=fh, stderr=subprocess.PIPE, timeout=TIMEOUT)
            status[label] = {"exit": r.returncode, "s": round(time.time() - t, 2), "stderr": r.stderr.decode(errors="replace")[-400:]}
        except subprocess.TimeoutExpired:
            status[label] = {"exit": "timeout", "s": TIMEOUT}
    json.dump(status, open(f"{out}/{name}.status.json", "w"))
    print(name, {k: v["exit"] for k, v in status.items()}, flush=True)
