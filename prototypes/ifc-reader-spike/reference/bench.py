"""Large-model benchmark: each reader in its own process under /usr/bin/time -v, one after the other.

    bench.py <spike binary> <out file> <ifc>...

Records the reader's own JSON line plus wall time and peak RSS from time(1).
"""
import json, os, re, subprocess, sys

binary, out = sys.argv[1], sys.argv[2]
here = os.path.dirname(os.path.abspath(__file__))
TIMEOUT = 5400

with open(out, "a") as fh:
    for f in sys.argv[3:]:
        runs = [
            ("own, 1 thread", [binary, "bench-own", f], {}),
            ("ifc-lite pipeline, 6 threads", [binary, "bench-lite", f], {}),
            ("ifc-lite pipeline, 1 thread", [binary, "bench-lite", f], {"RAYON_NUM_THREADS": "1"}),
            ("IfcOpenShell, 6 threads", [sys.executable, f"{here}/ios.py", "bench", f, "6"], {}),
            ("IfcOpenShell, 1 thread", [sys.executable, f"{here}/ios.py", "bench", f, "1"], {}),
        ]
        for label, cmd, env in runs:
            try:
                r = subprocess.run(["time", "-v"] + cmd, capture_output=True, text=True, timeout=TIMEOUT, env={**os.environ, **env})
                rss = re.search(r"Maximum resident set size \(kbytes\): (\d+)", r.stderr)
                wall = re.search(r"Elapsed \(wall clock\) time.*: (.*)", r.stderr)
                rec = {"run": label, "file": os.path.basename(f), "exit": r.returncode,
                       "wall": wall.group(1) if wall else None, "max_rss_mb": int(rss.group(1)) // 1024 if rss else None,
                       "reader": r.stdout.strip()[-1500:], "stderr_tail": r.stderr[-300:] if r.returncode else ""}
            except subprocess.TimeoutExpired:
                rec = {"run": label, "file": os.path.basename(f), "exit": "timeout", "wall": f">{TIMEOUT}s"}
            fh.write(json.dumps(rec) + "\n")
            fh.flush()
            print(json.dumps({k: rec.get(k) for k in ("run", "file", "exit", "wall", "max_rss_mb")}), flush=True)
