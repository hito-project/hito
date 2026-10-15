"""Summarise a run_corpus.py output directory.

    aggregate.py <out dir>

Prints Markdown tables: per file, per geometry kind, and the most common failures.
An element "matches" IfcOpenShell when both meshed it and the volumes agree within
1% (or 1e-6 m3) and every bounding-box extent within 1% (or 5 mm).
"""
import collections, glob, json, os, sys

out = sys.argv[1]
READERS = ["own", "lite", "ios"]


def close(a, b, rel, abs_):
    return abs(a - b) <= max(abs_, rel * max(abs(a), abs(b)))


def matches(r, ref):
    if not (r.get("ok") and ref.get("ok")):
        return False
    if not close(r["vol"], ref["vol"], 0.01, 1e-6):
        return False
    return all(close(x, y, 0.01, 0.005) for x, y in zip(r["ext"], ref["ext"]))


def origin_family(o):
    o = o.lower()
    for k, v in [("revit", "Revit"), ("exporter 1", "Revit"), ("exporter 2", "Revit"), ("archicad", "ArchiCAD"), ("tekla", "Tekla"),
                 ("ifcopenshell", "IfcOpenShell"), ("blenderbim", "IfcOpenShell"), ("bonsai", "IfcOpenShell")]:
        if k in o:
            return v
    return "Other"


files = []
cats = collections.defaultdict(lambda: collections.Counter())
fails = {r: collections.Counter() for r in READERS}
fam = collections.defaultdict(lambda: collections.Counter())
for path in sorted(glob.glob(f"{out}/*.jsonl")):
    if path.endswith(".ios.jsonl"):
        continue
    name = os.path.basename(path)[:-6]
    lines = open(path).read().splitlines()
    if not lines:
        files.append((name, None))
        continue
    head = json.loads(lines[0])
    rows = {}
    for l in lines[1:]:
        try:
            r = json.loads(l)
            rows[r["id"]] = r
        except json.JSONDecodeError:
            pass
    ios_path = f"{out}/{name}.ios.jsonl"
    if os.path.exists(ios_path):
        for l in open(ios_path).read().splitlines():
            try:
                r = json.loads(l)
            except json.JSONDecodeError:
                continue
            if r["id"] in rows:
                rows[r["id"]]["ios"] = r["ios"]
    st = json.load(open(f"{out}/{name}.status.json")) if os.path.exists(f"{out}/{name}.status.json") else {}
    c = collections.Counter()
    for r in rows.values():
        r.setdefault("ios", {"ok": False, "missing": True})
        c["n"] += 1
        for k in READERS:
            if r[k].get("ok"):
                c[k] += 1
            else:
                fails[k][r[k].get("err", "not meshed")] += 1
            if r[k].get("err") == "PANIC":
                c[k + "_panic"] += 1
        for k in ["own", "lite"]:
            if matches(r[k], r["ios"]):
                c[k + "_match"] += 1
        cat = r["cat"]
        cats[cat]["n"] += 1
        for k in READERS:
            cats[cat][k] += bool(r[k].get("ok"))
        for k in ["own", "lite"]:
            cats[cat][k + "_match"] += matches(r[k], r["ios"])
        cats[cat]["ios_ok_for_match"] += bool(r["ios"].get("ok"))
    f = origin_family(head.get("origin", ""))
    for k, v in c.items():
        fam[f][k] += v
    fam[f]["files"] += 1
    files.append((name, head, c, st))


def pct(a, b):
    return f"{100 * a / b:.1f}%" if b else "–"


print("## Per file\n")
print("| File | Schema | Origin | Elements | Own | ifc-lite | IfcOpenShell | Own = IOS | ifc-lite = IOS |")
print("|---|---|---|---|---|---|---|---|---|")
for item in files:
    if item[1] is None:
        print(f"| {item[0]} | | | compare produced nothing | | | | | |")
        continue
    name, head, c, st = item
    n = c["n"]
    extra = "" if all(v.get("exit") == 0 for v in st.values()) else " ⚠ " + ", ".join(f"{k}: {v.get('exit')}" for k, v in st.items() if v.get("exit") != 0)
    print(f"| {name}{extra} | {head['schema']} | {head['origin'][:40]} | {n} | {pct(c['own'], n)} | {pct(c['lite'], n)} | {pct(c['ios'], n)} | {pct(c['own_match'], c['ios'])} | {pct(c['lite_match'], c['ios'])} |")

print("\n## By originating system\n")
print("| Family | Files | Elements | Own | ifc-lite | IfcOpenShell | Own = IOS | ifc-lite = IOS |")
print("|---|---|---|---|---|---|---|---|")
for f, c in sorted(fam.items()):
    n = c["n"]
    print(f"| {f} | {c['files']} | {n} | {pct(c['own'], n)} | {pct(c['lite'], n)} | {pct(c['ios'], n)} | {pct(c['own_match'], c['ios'])} | {pct(c['lite_match'], c['ios'])} |")
tot = collections.Counter()
for c in fam.values():
    tot.update(c)
print(f"| **All** | {tot['files']} | {tot['n']} | {pct(tot['own'], tot['n'])} | {pct(tot['lite'], tot['n'])} | {pct(tot['ios'], tot['n'])} | {pct(tot['own_match'], tot['ios'])} | {pct(tot['lite_match'], tot['ios'])} |")
print(f"\nPanics caught: own {tot['own_panic']}, ifc-lite {tot['lite_panic']}")

print("\n## By geometry kind (share of elements meshed)\n")
print("| Kind | Elements | Own | ifc-lite | IfcOpenShell | Own = IOS | ifc-lite = IOS |")
print("|---|---|---|---|---|---|---|")
for cat, c in sorted(cats.items(), key=lambda kv: -kv[1]["n"]):
    n = c["n"]
    print(f"| {cat} | {n} | {pct(c['own'], n)} | {pct(c['lite'], n)} | {pct(c['ios'], n)} | {pct(c['own_match'], c['ios_ok_for_match'])} | {pct(c['lite_match'], c['ios_ok_for_match'])} |")

for k in READERS:
    print(f"\n## Top failures: {k}\n")
    for err, n in fails[k].most_common(15):
        print(f"- {n} × {err}")
