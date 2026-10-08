# Civil engineer workflows

The founding user's specific workflow is unknown, so this document describes **common** workflows. It stands in for the founding user's answers ([questions](../open-questions/founding-user-questions.md)).

## Typical Revit day for a structural or civil engineer

1. **Set up or refine the model:** levels, grids, and linked architectural models.
2. **Model the structure:** beams, columns, walls, foundations, rebar and connections, using families with the right parameters.
3. **Run analysis:** export to a solver (Robot, ETABS, SAP2000, or CYPECAD in Argentina), bring results back, and iterate.
4. **Coordinate:** check for clashes with other disciplines (Revit interference checks or Navisworks), and exchange IFC, DWG or RVT with consultants.
5. **Document:** foundation, floor and roof plans, sections and details, schedules (including rebar bending schedules), and sheets for permit and construction.
6. **Revise:** respond to red-line comments and work through revision cycles.

## Argentina

| Finding | Implication for us |
|---|---|
| **CYPECAD (CYPE) dominates structural calculation.** Job postings require it, and universities (UTN) teach it. It implements CIRSOC 201-2005 (reinforced concrete) and INPRES-CIRSOC 103 (seismic). | CYPE is a key program to replace or interoperate with. Argentine design-code checks are a must for local adoption. |
| **AutoCAD is the main drafting tool.** Revit is used, usually at an intermediate level. | DWG interop and 2D drafting matter as much as BIM, possibly more. |
| **BIM adoption is early.** In BIM Forum Argentina's 2023 survey (1,311 respondents), 65% were in training or early adoption and 20% had practical experience. Fewer than 5% of all respondents had exchanged BIM data across organisations. | RVT exchange is less critical in Argentina than DWG exchange. This lowers the urgency of RVT relative to DWG for the founding user's market. |
| **Public-sector push:** Plan Córdoba BIM 2030 is piloting BIM on 29 public works. | IFC export matters for public works, where open standards are typically required. |

## Assumed answers for the founding user

These are assumptions, to be replaced if the founding user answers directly.

| Question | Assumed answer |
|---|---|
| Daily Revit use | Structural modelling, documentation (plans, sections, sheets, schedules), coordination |
| Exchange `.rvt` with others? | Occasionally. DWG exchange is far more common. |
| Families | Mostly built-in and company families, occasionally from manufacturers |
| Other programs | AutoCAD and CYPECAD. Possibly ETABS/SAP2000 and Excel. |
| Most valued Revit property | Drawings generated from the model (no manual sync between plan and section) |
| Biggest pain points | Cost, Windows-only, data loss between Revit and analysis programs |
| Civil and infrastructure work | Mostly buildings. Infrastructure is occasional. |
| Blender | Not used. Blender was only mentioned as an alternative. |

## Sources

- [Autodesk: Revit for structural engineering](https://www.autodesk.com/campaigns/revit-for-structural-engineering)
- [Freelancer: Ongoing structural Revit support (typical tasks)](https://www.freelancer.de/projects/revit/ongoing-structural-revit-support)
- [UTN FRBA: CYPECAD courses](https://www.frba.utn.edu.ar/wp-content/uploads/2019/06/cypecad_2012.pdf)
- [Bumeran: Civil engineering job listings, Argentina](https://www.bumeran.com.ar/empleos-area-ingenieria-civil-y-construccion-subarea-ingenieria-civil-full-time-seniority-semi-sr.html)
- [CYPE: CIRSOC 201-2005 implementation](https://info.cype.com/en/new-feature/improved-code-application-cirsoc-201-2005-argentina/)
- [INTI: INPRES-CIRSOC 103 Part II (2021)](https://www.inti.gob.ar/assets/uploads/files/cirsoc/en%20tramite/2021/1%20-%20Reglamento%20IC-103-II%202021.pdf)
- [Hexagon: The state of BIM adoption in key LATAM markets](https://multivistaservices.hexagon.com/the-state-of-bim-adoption-in-key-latam-construction-markets/)
