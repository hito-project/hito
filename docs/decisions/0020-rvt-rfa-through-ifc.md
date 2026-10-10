# 0020. RVT and RFA: through Revit's IFC export, no direct reader for now

- **Status:** Accepted
- **Date:** 2026-10-10

## Context

HITO's first users come from Revit, so the first question they ask is whether they can bring their Revit projects (`.rvt`) and families (`.rfa`) over. RVT is out of scope for M1 ([ADR 0011](0011-mvp-structural-bim.md)), but epic E23 ([#67](https://github.com/hito-project/hito/issues/67)) needs a strategy. Spike [#8](https://github.com/hito-project/hito/issues/8) surveyed the options and the law on 2026-10-10. The evaluations, sources and legal summary are in [Discussion #131](https://github.com/hito-project/hito/discussions/131).

What the survey found:

| Route | What it gets from Revit | Fit for HITO |
|---|---|---|
| **Revit's IFC export** ([revit-ifc](https://github.com/Autodesk/revit-ifc), LGPL-2.1, runs inside Revit) | Elements, `Family:Type` names, types, materials, Revit parameters as property sets, quantities, stable GlobalIds, IFC4 | Free to anyone with Revit. Loses parametric families, constraints, the analytical model and loads. Has open structural bugs. |
| **A Revit add-in** (our own, run in the user's Revit) | Everything the public Revit API exposes, including family parameters and formulas | The only route to parametric families. Needs a Revit licence and a family model in HITO (E16). |
| **rvt-rs** (Apache-2.0, Rust, reads files directly) | Revit 2024 and 2025 projects as typed IFC elements; 2023 partly; older releases, 2027 and family geometry not at all; few parameters | The first credible open-source reader, but six months old, one author, and its legal footing is untested |
| **ODA BimRv** | RVT and RFA from 2011 on | Proprietary and members-only. At least $13,750 the first year, and every distributor must be a member. Incompatible with [ADR 0009](0009-license.md). |
| **Autodesk Platform Services** | RVT to IFC in Autodesk's cloud | Paid, sends models to Autodesk, and its terms forbid building competing products |
| **Speckle** | Meshes and properties, published from inside Revit | Revit receives only non-native shapes, as it does from IFC. No better than IFC for migration. |

No route available to an open-source project gives a native, editable round trip with Revit. Even commercial products built on BimRv import Revit models as solids.

The law, summarised in the Discussion (it isn't legal advice):

- Studying a file format to interoperate is generally lawful in the EU, where contract terms can't forbid it (Directive 2009/24/EC, articles 5, 6 and 8; *SAS v. WPL*). In the US it's fair use (*Sega*, *Connectix*), and RVT files aren't encrypted, so the DMCA anti-circumvention rule probably doesn't apply.
- Argentine law doesn't prohibit reverse engineering, but unlike the EU's it has no explicit exception for interoperability or decompiling, so how Ley 11.723 applies to decompilation is untested. Studying file formats isn't affected.
- Autodesk's terms forbid Revit users from studying Revit's "data structures", and put South American customers under California law. US courts have enforced such clauses (*Bowers v. Baystate*). Most people who would contribute RVT knowledge or test files to HITO are bound by them.
- Autodesk has enforced hard around file compatibility before: it sued the ODA over DWG in 2006 and won a restraining order in nine days.

## Decision

### 1. Revit projects come in through Revit's own IFC export

- **One-time migration:** the user, or a colleague with Revit, exports IFC4 from Revit, and HITO imports it. This depends on IFC import (E22, [#66](https://github.com/hito-project/hito/issues/66)), not on an RVT reader.
- **HITO's importer knows Revit's exporter.** It rebuilds types from `Family:Type`, keeps Revit's parameters and GlobalIds, and lists what it couldn't map ([#135](https://github.com/hito-project/hito/issues/135)).
- **HITO publishes a Revit IFC export setup** (a `.json` file Revit's exporter loads) and a guide in Spanish and English ([#136](https://github.com/hito-project/hito/issues/136)). On Linux, the guide uses Revit in a Windows virtual machine for the one-time export.

### 2. Ongoing exchange with Revit users is IFC both ways

Revit colleagues send IFC exports. HITO's IFC4 exports ([ADR 0016](0016-ifc-export.md)) are linked in Revit as a reference. Neither side edits the other's native elements. This is the usual openBIM exchange between different tools.

### 3. Families wait for a Revit add-in

IFC brings a family over only as fixed geometry per type, and no open reader can do better. Parametric families need code running inside Revit. A spike ([#134](https://github.com/hito-project/hito/issues/134)) scopes an open-source HITO exporter add-in once the families epic (E16, [#60](https://github.com/hito-project/hito/issues/60)) has a family model. The add-in would be .NET, licensed like HITO, and never linked into HITO.

### 4. No direct RVT reader for now, and no reverse engineering by the project

- HITO doesn't link or ship an RVT reader, and doesn't reverse-engineer RVT or RFA files. No ODA or Autodesk SDK code, and no APS.
- rvt-rs is re-evaluated when E23 is scheduled, together with a lawyer's answers to the questions in Discussion #131 ([#133](https://github.com/hito-project/hito/issues/133)). Until then, HITO's documentation doesn't recommend it.

## Consequences

- Revit users can't bring a project into HITO until IFC import (E22) exists. Their model's elements, types and data survive, but its parametric behaviour doesn't.
- HITO carries no legal risk from the RVT format and no proprietary dependency.
- Migration needs Revit once, either the user's licence or a colleague's. Running it in Revit's Viewer mode without a licence may work, but it's unclear whether Revit's terms allow it, so the guide won't suggest it until that's clarified.
- The quality of a migration depends on Revit's exporter, including its bugs. HITO's importer has to be tested against real Revit exports from several versions.
- Exchange with Revit is one-way per element, like any IFC exchange. HITO's IFC export quality matters to Revit users, so checking HITO's exports in Revit is suggested for [#52](https://github.com/hito-project/hito/issues/52).
- If the legal review is favourable and rvt-rs matures, a direct reader could later let users without Revit migrate. That would be a new ADR.
- New work: [#135](https://github.com/hito-project/hito/issues/135), [#136](https://github.com/hito-project/hito/issues/136), [#134](https://github.com/hito-project/hito/issues/134) and [#133](https://github.com/hito-project/hito/issues/133), all under E23.
