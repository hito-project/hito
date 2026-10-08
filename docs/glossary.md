# Glossary

## AEC terms

| Term | Meaning |
|---|---|
| **AEC** | Architecture, Engineering and Construction: the industry this software serves. |
| **BIM** | Building Information Modeling. A 3D model made of semantic elements (walls, pipes, beams) that carry data and relate to one another, instead of plain lines and shapes. |
| **CAD** | Computer-Aided Design. In AEC it usually means drafting with geometric primitives (lines, arcs, blocks), as in AutoCAD. |
| **MEP** | Mechanical, Electrical and Plumbing: ducts, pipes, cables and equipment. |
| **IFC** | Industry Foundation Classes. The open, ISO-standardised (ISO 16739) exchange format for BIM data. |
| **RVT / RFA** | Revit's closed project format (`.rvt`) and family format (`.rfa`). |
| **DWG** | AutoCAD's closed drawing format and the de facto standard for exchanging 2D drawings. |
| **Family** | A parametric component definition in Revit, such as a door whose width and height are parameters. |
| **Type / Instance** | A family has *types* (e.g. "Door 90×210") and placed *instances*. Type parameters are shared by all instances of a type; instance parameters are set per placed element. |
| **Hosted element** | An element that lives inside another element, such as a window in a wall. It moves with its host and cuts it. |
| **Level / Grid** | Horizontal datum planes (floors) and reference axes that other elements attach to. |
| **View** | A plan, section, elevation, 3D view or schedule *generated* from the model, never drawn separately. |
| **Sheet** | A printable drawing page that arranges views with a title block. |
| **Schedule** | A table generated from model data, such as a door list or a pipe bill of quantities. |
| **Analytical model** | A simplified structural representation of the physical model (beams become lines, slabs become planes) that analysis solvers consume. |
| **Clash detection** | Finding elements that physically conflict, such as a pipe running through a beam. |
| **Worksharing** | Revit's multi-user editing of a central model, with element borrowing. |
| **Phasing / Design options** | Modelling a building over time (existing, demolished, new) and comparing alternative designs inside one model. |
| **CIRSOC / INPRES-CIRSOC** | Argentina's structural design codes: 201 for reinforced concrete, 301 for steel, 103 for seismic design. |
| **IRAM** | Argentina's standards body. Its standards include technical drawing conventions. |
| **CYPE / CYPECAD** | Spanish structural analysis and design software, dominant in Argentina and Spain. |
| **TIN** | Triangulated Irregular Network. A triangle-mesh representation of terrain. |
| **Alignment / Corridor** | In civil design, a road or channel centreline, and the 3D solid swept along it. |

## Project terms

| Term | Meaning |
|---|---|
| **Core** | The minimal shared platform: element store, schemas, relationships, transactions, coordinates, views. See [architecture/unified-core.md](architecture/unified-core.md). |
| **Domain** | A plug-in schema that adds element types for one discipline (architecture, structure, MEP, civil and so on). |
| **Port / Adapter** | A core interface (port) and an interchangeable implementation of it (adapter), such as a solver, an importer or a renderer. |
| **Workspace** | A discipline-specific UI layout over the shared core, similar to Revit's discipline tabs (Architecture, Structure, Systems). |
| **Model** | A container of elements with one kind: physical, definition, drawing, analytical or linked. Every element lives in exactly one model. See [architecture/element-model.md](architecture/element-model.md). |
| **Aspect** | A group of properties that one domain attaches to another domain's element, such as structural data on a wall. |
| **Code** | An optional human-readable name, unique within a scope, such as the structural mark C12. It is not the element's identity. |
| **Representation** | One named geometry of an element (`Body`, `Axis`, `FootPrint`), all generated from the same parameters. |
| **Change set** | The record of one committed transaction: what was created, modified and deleted, with before and after values. It drives undo, saving and collaboration. |
| **Founding user** | The civil engineer whose day-to-day Revit use drives the build order. See [vision/target-user.md](vision/target-user.md). |
