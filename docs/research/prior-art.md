# Prior art

| Project | What it is | Lesson for us |
|---|---|---|
| **Bentley iTwin / iModel / BIS** | An iModel is a SQLite database with a schema defined by BIS (Base Infrastructure Schemas): a small BisCore plus modular per-discipline domain schemas. Open source (iTwin.js, BIS on GitHub). | The closest model for our core and domain design. Study it before designing the element store. It unifies data, not authoring tools. |
| **IFC** (buildingSMART) | Open ISO schema covering architecture, structure, MEP and infrastructure | Shows that one schema can span disciplines. Our main exchange format. |
| **IfcOpenShell** | Open-source IFC toolkit (C++/Python) | Reference for IFC semantics and test files |
| **Bonsai** (formerly BlenderBIM) | IFC-native BIM inside Blender | Users must learn Blender first. That's why we build a standalone app ([ADR 0007](../decisions/0007-ui-familiar-to-current-users.md)). |
| **FreeCAD BIM** | BIM workbench on FreeCAD (C++/Qt/OpenCascade) | Mature geometry and IFC, but a UX we want to avoid |
| **Speckle** | Open-source AEC data exchange with connectors for Revit, Rhino, ETABS and others | Potential adapter and ally for interop |
| **Blender** | One core with very different toolsets exposed as workspaces | Shows that one core can serve very different toolsets. It is **not** our UI model: its patterns are adopted only via ADR ([ADR 0007](../decisions/0007-ui-familiar-to-current-users.md)). |
| **WinApps** | Runs Windows apps such as Revit from a VM, integrated into the Linux desktop | A stopgap so users can move to Linux before parity |

## Sources

- [iTwin.js: iModel overview](https://www.itwinjs.org/learning/imodels/)
- [iTwin.js: BIS](https://www.itwinjs.org/bis)
- [iTwin.js: Organization of BIS](https://www.itwinjs.org/bis/guide/intro/bis-organization)
- [iTwin.js: BIS core domains](https://www.itwinjs.org/bis/domains/core-domains)
- [Speckle](https://speckle.systems)
- [WinApps](https://github.com/winapps-org/winapps)
