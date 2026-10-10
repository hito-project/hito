"""Check an exported IFC file against what the exporter says it wrote.

    python check.py FILE.ifc FILE.expected.json

Three layers, each one the spike proposes for CI:
  1. schema: IfcOpenShell's validator, with EXPRESS where-rules and functions
  2. geometry: IfcOpenShell's own geometry engine builds every element; its
     volume and world bounding box must match the parameters
  3. semantics: every element is found again with its GUID, class, name,
     storey, type, material and properties
Exit status is non-zero if anything fails.
"""

import json
import sys
import time

import ifcopenshell
import ifcopenshell.geom
import ifcopenshell.util.element as eu
import ifcopenshell.util.shape as su
import ifcopenshell.validate

VOLUME_TOL = 1e-6  # relative
BBOX_TOL = 1e-6  # metres


def main(path, expected_path):
    expected = json.load(open(expected_path))
    failures = []

    # 1. Schema and where-rules
    t = time.perf_counter()
    f = ifcopenshell.open(path)
    logger = ifcopenshell.validate.json_logger()
    ifcopenshell.validate.validate(f, logger, express_rules=True)
    for s in logger.statements:
        failures.append(f"schema: {s.get('message')} on {s.get('instance')}")
    print(f"schema {f.schema_identifier}: {len(logger.statements)} issue(s), {time.perf_counter() - t:.2f} s")

    # 2. Geometry
    settings = ifcopenshell.geom.settings()
    settings.set("use-world-coords", True)
    rows = []
    for e in expected["elements"]:
        el = f.by_guid(e["guid"])
        body = next(r for r in el.Representation.Representations if r.RepresentationIdentifier == "Body")
        try:
            shape = ifcopenshell.geom.create_shape(settings, el, body)
        except Exception as ex:
            failures.append(f"geometry: {e['name']} could not be built: {ex}")
            continue
        g = shape.geometry
        vol = su.get_volume(g)
        verts = su.get_vertices(g)
        lo = verts.min(axis=0).tolist()
        hi = verts.max(axis=0).tolist()
        rel = abs(vol - e["volume"]) / e["volume"]
        # Circles are faceted by the reader, so their extents and volume are
        # only close; planar solids must match exactly.
        curved = body.Items[0].SweptArea.is_a("IfcCircleProfileDef")
        tol = 1e-3 if curved else BBOX_TOL
        ok_bbox = all(abs(a - b) <= tol for a, b in zip(lo + hi, e["bbox_min"] + e["bbox_max"]))
        rows.append((e["class"], e["name"], e["volume"], vol, rel, ok_bbox))
        if not ok_bbox:
            failures.append(f"geometry: {e['name']} bbox {lo} {hi} != {e['bbox_min']} {e['bbox_max']}")
        if rel > (1e-2 if curved else VOLUME_TOL):
            failures.append(f"geometry: {e['name']} volume {vol} != {e['volume']}")
    print("geometry: class, name, exact volume (parameters), mesh volume (IfcOpenShell), relative difference, bbox ok")
    for r in rows:
        print(f"  {r[0]:<10} {r[1]:<4} {r[2]:.6f} {r[3]:.6f} {r[4]:.2e} {r[5]}")

    # 3. Semantics
    for e in expected["elements"]:
        el = f.by_guid(e["guid"])
        where = e["name"]
        if not el.is_a(e["class"]):
            failures.append(f"semantics: {where} is {el.is_a()}, expected {e['class']}")
        if el.Name != e["name"]:
            failures.append(f"semantics: {where} name {el.Name!r}")
        container = eu.get_container(el)
        if container is None or container.Name != e["storey"]:
            failures.append(f"semantics: {where} storey {container and container.Name!r}")
        t_ = eu.get_type(el)
        if (t_.Name if t_ else None) != e["type"]:
            failures.append(f"semantics: {where} type {t_ and t_.Name!r}")
        mats = eu.get_materials(el)
        if [m.Name for m in mats] != [e["material"]]:
            failures.append(f"semantics: {where} materials {[m.Name for m in mats]}")
        psets = eu.get_psets(el)
        for m in mats:
            psets.update(eu.get_psets(m))
        for pset, prop, value in e["props"]:
            got = psets.get(pset, {}).get(prop)
            ok = str(got) == value or (
                isinstance(got, float) and abs(got - float(value)) <= 1e-9 * max(1.0, abs(float(value)))
            )
            if not ok:
                failures.append(f"semantics: {where} {pset}.{prop} = {got!r}, expected {value}")
    print(f"semantics: {len(expected['elements'])} elements checked")

    for x in failures:
        print("FAIL", x)
    print("PASS" if not failures else f"{len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1], sys.argv[2]))
