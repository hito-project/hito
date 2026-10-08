# Industry pain points

**Evidence quality is mixed.** Strong sources include the NIST study, the 2020 open letter and Autodesk's own documentation. Many others are vendor or reseller blogs, and the NIST figure is from 2004. The founding user's own experience overrides this list.

The principles derived from these pain points are in [vision/principles.md](../vision/principles.md).

## 1. Data loss between programs

- NIST estimated that inadequate interoperability cost the US capital facilities industry **$15.8 billion per year** (2002 data, described as conservative). Owners and operators bore $10.6B of that, architects and engineers $1.2B.
- Studies of Revit → IFC → analysis software found missing material properties, loads that wouldn't import, distorted geometry and displaced objects. Revit's internal model doesn't map one-to-one to IFC, and IFC implementations differ between programs and versions.
- Moving a model between Revit and ETABS or Robot requires manual QA: node tolerances, connectivity checks, and locking type names before export.

## 2. Cost and licensing

- In 2020, an open letter (initially 17 UK firms including Zaha Hadid Architects and Grimshaw, later 48 firms) cited a cost increase of **up to 70% over five years** while development stagnated: *"Practices find that they are paying more but using Revit less because of its constraints."*
- The AEC Collection costs about **$3,375 per user per year** (2026 list price), with 8–10% annual increases.
- Licensing models keep changing: named user, Flex tokens, and removed renewal discounts.

## 3. Lock-in and data ownership

- `.rvt` and `.dwg` are closed formats, so a firm's historic project data stays tied to one vendor.
- 96% of construction CIOs report concern over data ownership and control (2025).

## 4. Version lock-in

- RVT files are **not backward compatible**. Upgrades are one-way, and Revit can't save to older formats.
- Whole project teams must upgrade together, and firms keep several Revit versions installed. Autodesk recommends upgrading large models one release at a time.

## 5. Performance and stability

- Revit is **largely single-threaded**, so multi-core CPUs barely help.
- Crashes cluster around syncs, view changes and regenerations, especially with linked models, rebar and dense MEP.
- Users report each release feeling slower than the last.

## 6. Windows only

- There's no macOS or Linux version. Mac studios rely on virtual machines or cloud workstations.

## Sources

- [NIST GCR 04-867: Cost Analysis of Inadequate Interoperability](https://nvlpubs.nist.gov/nistpubs/gcr/2004/NIST.GCR.04-867.pdf)
- [The Architect's Newspaper: Open letter to Autodesk](https://www.archpaper.com/2020/07/leading-architecture-firms-pen-open-letter-to-autodesk/)
- [Dezeen: Architects criticise lack of development of Revit](https://www.dezeen.com/2020/07/28/autodesk-revit-bim-software-criticism/)
- [AEC Magazine: Nordic associations demand better value](https://www.aecmag.com/bim/nordic-architectural-associations-demand-better-value-from-autodesk)
- [AEC Magazine: Prisoner of Vendor](https://aecmag.com/bim/prisoner-of-vendor-has-aec-software-become-a-trap-bim/)
- [MDPI: BIM interoperability via IFC from a structural viewpoint](https://www.mdpi.com/2076-3417/11/23/11430)
- [MDPI: Architectural and structural interoperability in BIM](https://www.mdpi.com/2075-5309/15/24/4540)
- [ResearchGate: Revit–ETABS interoperability analysis](https://www.researchgate.net/publication/398660413_Streamlining_Structural_Synergy_A_Critical_Analysis_of_Cost-Free_Revit-ETABS_BIM_Interoperability_Solutions)
- [Novedge: Optimizing Revit's analytical model](https://novedge.com/blogs/design-news/revit-tip-optimizing-revit-analytical-model-for-reliable-structural-analysis)
- [Autodesk: Revit backward compatibility](https://autodesk.com/support/technical/article/Backwards-compatibility-of-Revit-with-earlier-releases-of-the-software)
- [Autodesk forums: Revit slow but CPU at 30%](https://forums.autodesk.com/t5/revit-architecture-forum/revit-is-very-slow-but-cpu-is-only-at-30/m-p/7046609)
- [Vagon: Why Revit crashes](https://vagon.io/blog/autodesk-revit-crashes)
- [Autodesk price increase history 2019–2026](https://autodesksaudits.com/blog/autodesk-price-increase-history/)
- [AEC Collection licensing guide](https://autodesksaudits.com/blog/autodesk-aec-collection-licensing/)
- [PressReach: Construction CIOs on data ownership](https://pressreach.com/technology/constructions-cios-sound-the-alarm-on-data-ownership-and-ai-readiness/)
- [MyArchitectAI: Running Revit on a Mac](https://www.myarchitectai.com/blog/revit-for-mac)
