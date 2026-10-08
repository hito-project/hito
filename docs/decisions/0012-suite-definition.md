# 0012. Suite definition

- **Status:** Accepted
- **Date:** 2026-10-08

## Context

[ADR 0006](0006-civil-engineering-suite-scope.md) makes HITO a complete civil engineering suite, with parity as the minimum, but doesn't say which programs the suite replaces. Spike [#82](https://github.com/hito-project/hito/issues/82) inventoried the candidates, one Research discussion per area:

- [2D drafting](https://github.com/hito-project/hito/discussions/84)
- [civil and infrastructure](https://github.com/hito-project/hito/discussions/85)
- [structural analysis and design](https://github.com/hito-project/hito/discussions/86)
- [detailing](https://github.com/hito-project/hito/discussions/87)
- [geotechnical and water](https://github.com/hito-project/hito/discussions/88)
- [coordination and reality capture](https://github.com/hito-project/hito/discussions/89)
- [visualization](https://github.com/hito-project/hito/discussions/90)
- [project management](https://github.com/hito-project/hito/discussions/91)

It also collected the [core requirements](https://github.com/hito-project/hito/discussions/92) they imply.

Two findings shape the choice:

1. Most of these programs are modelling, geometry and drawings, which is core work. A few are built around a **specialist solver** that has decades of validation behind it, and engineers rely on it for liability.
2. **Solver licenses vary.** OpenSees needs a commercial license for use on engineering projects (since 2022). Code_Aster and CalculiX are GPL. MYSTRAN (MIT), Kratos (BSD), EPANET (MIT) and SWMM (public domain) can be wrapped freely. HEC-RAS is free to use, but its source isn't published.

## Decision

The suite replaces these programs. Parity with them is the minimum, as in ADR 0006:

| Area | Programs replaced | Solvers |
|---|---|---|
| BIM | Revit | — |
| 2D drafting | AutoCAD | — |
| Civil and infrastructure | Civil 3D, OpenRoads; InfraWorks's conceptual design becomes part of the civil workspace | Hydraulics wrapped (see below) |
| Structural analysis and design | CYPECAD, ETABS, SAP2000, SAFE, STAAD.Pro, Robot | All analysis runs through the **solver port** (see below) |
| Detailing | Tekla Structures, Advance Steel | — |
| Geotechnical and water | PLAXIS, HEC-RAS, HEC-HMS, EPANET, SWMM, WaterGEMS: the modelling and results side | **All solvers wrapped**, never rewritten |
| Coordination | Navisworks | Clash detection built in |
| Reality capture | ReCap: display and use of registered point clouds | Scan registration later, possibly wrapped |
| Visualization | 3ds Max, rendering use only | Offline rendering wrapped (for example Cycles) |
| Quantities, cost and scheduling | Quantity takeoff and *cómputo y presupuesto*; **MS Project and Primavera P6**; 4D playback | — |

**Not replaced:** general business software such as ERP and accounting systems. HITO links to them through import and export.

**Structural analysis through a swappable solver port.** Linear analysis (static, modal, response spectrum) runs through the solver port:

1. The first analysis release ships with a wrapped solver (MYSTRAN, MIT).
2. A built-in Rust solver can replace it later, behind the same port.
3. Both stay available, so results can be cross-checked.

The port is designed for what design checks need (forces at sections along members, modal results, spectrum combinations), not for one solver's output. Nonlinear and time-history analysis also go through wrapped solvers.

**Solver rules:**

- A wrapped solver must have a license that lets any user run it on commercial work at no extra cost: MIT, BSD, Apache, or public domain.
- GPL solvers run only as separate programs, never linked.
- Solvers that need a license from the user (OpenSees for commercial work, PLAXIS, CSI and others) are optional adapters, never the default.

## Consequences

- Every area now has a capability inventory, so "parity" can be planned and checked. MS Project and Primavera P6 are the exception. They still need an inventory of their own, because [#91](https://github.com/hito-project/hito/discussions/91) covered only quantities, cost and 4D linking.
- The core must meet requirements SR-1 to SR-17 from the [core requirements](https://github.com/hito-project/hito/discussions/92). SR-1 to SR-7, SR-9, SR-10 and SR-15 shape the M0 spikes and M1 now. They're linked from the affected issues.
- [unified-core.md](../architecture/unified-core.md) no longer lists OpenSees as a default solver.
- A future epic covers quantities, cost, scheduling and 4D, since no current epic did.
- Scheduling parity with Primavera P6 is a large scope: resource levelling, baselines, earned value, multi-project and multi-user work. It's also what links the model to the work plan (*plan de trabajos*) and investment curve (*curva de inversiones*) without data loss.
- Analysis can ship early with a validated wrapped solver, and a built-in solver doesn't need to be trusted blindly, because the two can be compared. The solver port must be designed before the first solver is wrapped.
- Discussion [#80](https://github.com/hito-project/hito/discussions/80) is answered by this ADR.
