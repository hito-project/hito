# M1: Structural BIM MVP

A usable structural modeller for reinforced-concrete buildings ([ADR 0011](../../decisions/0011-mvp-structural-bim.md)).

**Done when:** a user can model a small concrete building (levels, grids, columns, beams, slabs, walls, footings) in Spanish or English, get generated plan views with dimensions, export plans to PDF and the model to valid IFC4, and save and reopen the project.

| Epic | Goal |
|---|---|
| [E03: Element store and schemas](e03-element-store.md) | The data foundation every domain builds on |
| [E04: Transactions and undo](e04-transactions-undo.md) | Safe, undoable model changes, ready for future worksharing |
| [E05: Application shell and commands](e05-app-shell-commands.md) | A familiar window, panels and command system |
| [E06: 3D viewport and geometry](e06-viewport-geometry.md) | See and select the model, and generate element geometry |
| [E07: Levels and grids](e07-levels-grids.md) | The datum elements everything attaches to |
| [E08: Structural elements](e08-structural-elements.md) | Columns, beams, slabs, walls and footings |
| [E09: Views and drawings](e09-views-drawings.md) | Generated plans, dimensions and PDF export |
| [E10: Native file format](e10-native-file-format.md) | Open, documented, versioned save and load |
| [E11: IFC export](e11-ifc-export.md) | Exchange with the rest of the industry |
| [E12: Localization and units](e12-localization-units.md) | Spanish and English, metric units, nothing hardcoded |
