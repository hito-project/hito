"""List the entities an exported file uses whose attributes differ between IFC4 and IFC4X3_ADD2.

    python schema_diff.py FILE.ifc
"""

import sys

import ifcopenshell
import ifcopenshell.ifcopenshell_wrapper as w

f = ifcopenshell.open(sys.argv[1])
names = sorted({e.is_a() for e in f})
a, b = w.schema_by_name("IFC4"), w.schema_by_name("IFC4X3_ADD2")
same = 0
for n in names:
    try:
        x = [at.name() for at in a.declaration_by_name(n).all_attributes()]
        y = [at.name() for at in b.declaration_by_name(n).all_attributes()]
    except Exception as ex:
        print(f"{n}: missing in one schema ({ex})")
        continue
    if x == y:
        same += 1
    else:
        print(f"{n}: IFC4 {x}\n    IFC4X3 {y}")
print(f"{len(names)} entity types used, {same} identical in both schemas")
