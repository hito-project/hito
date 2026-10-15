"""IfcOpenShell as the reference.

    ios.py elements <file.ifc> <compare.jsonl>   per-element results for the ids in a `compare` run
    ios.py bench <file.ifc> [threads]            open + mesh every element, timing and peak memory

Uses IfcOpenShell's default geometry settings plus world coordinates.
"""
import json, multiprocessing, resource, sys, time

import ifcopenshell
import ifcopenshell.geom
import numpy as np


def peak_mb():
    return resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / 1024


def summary(shape):
    v = np.array(shape.geometry.verts, dtype=float).reshape(-1, 3)
    f = np.array(shape.geometry.faces, dtype=np.int64).reshape(-1, 3)
    if len(f) == 0:
        return None
    v = v - v[np.unique(f)].min(0)  # about the bbox corner: georeferenced coordinates would cancel out
    a, b, c = v[f[:, 0]], v[f[:, 1]], v[f[:, 2]]
    vol = abs(np.einsum("ij,ij->i", a, np.cross(b, c)).sum() / 6)
    used = v[np.unique(f)]
    ext = (used.max(0) - used.min(0)).round(4).tolist()
    return {"tris": int(len(f)), "vol": round(float(vol), 6), "ext": ext}


def settings():
    s = ifcopenshell.geom.settings()
    s.set("use-world-coords", True)
    return s


def elements(path, compare):
    ids = []
    with open(compare) as fh:
        next(fh)
        for line in fh:
            ids.append(json.loads(line)["id"])
    f = ifcopenshell.open(path)
    by_id = {}
    if ids:
        it = ifcopenshell.geom.iterator(settings(), f, multiprocessing.cpu_count(), include=[f.by_id(i) for i in ids])
        t = time.time()
        if it.initialize():
            while True:
                sh = it.get()
                by_id[sh.id] = summary(sh)
                if not it.next():
                    break
    for i in ids:
        r = by_id.get(i)
        print(json.dumps({"id": i, "ios": {"ok": True, **r} if r else {"ok": False}}))


def bench(path, threads):
    t0 = time.time()
    f = ifcopenshell.open(path)
    t_open = time.time() - t0
    rss_open = peak_mb()
    n_inst = len(list(f))
    t = time.time()
    it = ifcopenshell.geom.iterator(settings(), f, threads)
    meshed, tris = 0, 0
    if it.initialize():
        while True:
            sh = it.get()
            meshed += 1
            tris += len(sh.geometry.faces) // 3
            if not it.next():
                break
    t_geom = time.time() - t
    print(json.dumps({
        "reader": "ifcopenshell " + ifcopenshell.version, "file": path, "instances": n_inst, "open_s": round(t_open, 3),
        "rss_after_open_mb": round(rss_open), "elements_meshed": meshed, "triangles": tris,
        "geometry_s": round(t_geom, 3), "total_s": round(time.time() - t0, 3), "peak_rss_mb": round(peak_mb()), "threads": threads,
    }))


if __name__ == "__main__":
    if sys.argv[1] == "elements":
        elements(sys.argv[2], sys.argv[3])
    else:
        bench(sys.argv[2], int(sys.argv[3]) if len(sys.argv) > 3 else multiprocessing.cpu_count())
