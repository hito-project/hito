# RVT interoperability feasibility

**Status: not yet researched.** Tracked by the RVT and DWG spikes in [E01](../backlog/m0-foundations/e01-research-spikes.md). Once those spikes produce ADRs, this file is replaced by them.

## Question

Can an open-source application read, and ideally write, Revit `.rvt` and `.rfa` files?

## Why it matters

Network effects keep firms on Revit ([research](../research/why-separate-products.md)). If users can't exchange RVT files with collaborators, they can't switch, whatever our features are. This may be the biggest single risk to adoption internationally. In Argentina, cross-company BIM exchange is still rare and DWG dominates ([workflows](../research/civil-engineer-workflows.md)), so DWG feasibility deserves equal priority. The same goes for licence compatibility ([ADR 0009](../decisions/0009-license.md)).

## To investigate

- The state of open-source RVT reverse-engineering efforts
- Commercial SDKs (such as the Open Design Alliance's BimRv) and their licensing compatibility with open source
- Indirect routes: Revit's own IFC export, Speckle connectors, the Revit API run under WinApps
- Legal considerations of reverse-engineering file formats for interoperability
