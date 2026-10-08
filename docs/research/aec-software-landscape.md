# AEC software landscape

Programs civil engineers commonly use, by purpose. Free or open-source equivalents already run on Linux.

## Drafting and BIM

| Program | Vendor | Purpose |
|---|---|---|
| AutoCAD | Autodesk | 2D/3D drafting. DWG is the industry exchange format. |
| Revit | Autodesk | BIM for buildings: architecture, structure, MEP |
| ArchiCAD | Graphisoft | Revit's main competitor |
| Tekla Structures | Trimble | Detailed steel and concrete models, fabrication drawings, connections |
| Advance Steel | Autodesk | Steel detailing and shop drawings |
| *Open source* | | FreeCAD BIM, Bonsai (Blender), LibreCAD / QCAD (2D) |

## Civil and infrastructure design

| Program | Vendor | Purpose |
|---|---|---|
| Civil 3D | Autodesk | Land development, roads, grading, pipe networks |
| InfraWorks | Autodesk | Early-stage infrastructure design in real-world context |
| OpenRoads | Bentley | Road and transportation design |
| *Open source* | | QGIS (GIS) |

## Structural analysis and design

| Program | Vendor | Purpose |
|---|---|---|
| ETABS | CSI | Buildings, especially seismic analysis |
| SAP2000 | CSI | General structures: frames, shells, solids, cables |
| SAFE | CSI | Concrete slabs and foundations |
| STAAD.Pro | Bentley | General structural analysis and design |
| Robot Structural Analysis | Autodesk | Analysis that round-trips with Revit |
| CYPECAD / CYPE 3D | CYPE | Concrete and steel building design. Dominant in Argentina and Spain, and implements the CIRSOC codes ([workflows](civil-engineer-workflows.md)). |
| *Open source* | | OpenSees, Code_Aster |

## Geotechnical and water

| Program | Vendor | Purpose |
|---|---|---|
| PLAXIS | Bentley | Geotechnical analysis |
| HEC-RAS / HEC-HMS | US Army Corps of Engineers (free) | River hydraulics, hydrology |
| EPANET | US EPA (free, open source) | Water distribution networks |
| EPA SWMM | US EPA (free, open source) | Stormwater and drainage |
| WaterGEMS | Bentley | Water distribution |

## Coordination, visualization and site data

| Program | Purpose |
|---|---|
| Navisworks (Autodesk) | Model federation and clash detection |
| ReCap Pro (Autodesk), Trimble software | Point clouds and survey data |
| 3ds Max (Autodesk) | Rendering |
| Autodesk Construction Cloud / BIM 360, Procore | Shared project data, construction management |
| MS Project, Excel | Scheduling, cost estimating |

## Relevance to this project

- Revit users typically also use **AutoCAD, Navisworks and one structural solver**. That's why parity includes an analytical model and clash detection.
- Several specialist tools are already free (EPANET, SWMM, HEC-RAS, OpenSees, QGIS). They are candidates for adapters, not rewrites. See [architecture/unified-core.md](../architecture/unified-core.md).

## Sources

- [Autodesk AEC Collection: included software](https://www.autodesk.com/collections/architecture-engineering-construction/included-software)
- [Wikipedia: List of civil engineering software](https://en.wikipedia.org/wiki/List_of_civil_engineering_software)
- [Novatr: Best structural engineering software](https://www.novatr.com/blog/best-structural-engineering-software)
- [Construction Placements: Essential computer skills for civil engineers](https://www.constructionplacements.com/essential-computer-skills-for-civil-engineers/)
- [Gitnux: Best structural analysis software](https://gitnux.org/best/structural-analysis-software/)
- [WGI: 10 software programs to know in civil engineering](https://wginc.com/10-software-programs-to-know-in-civil-engineering/)
- [awesome-civil-engineering](https://github.com/QuantumNovice/awesome-civil-engineering)
