# Principles

Each principle answers a documented industry pain point. See [research/industry-pain-points.md](../research/industry-pain-points.md).

| # | Principle | Pain point it answers |
|---|---|---|
| 1 | **One model, no lossy hand-offs.** Physical and analytical data live in the same model, and IFC export is covered by automated round-trip tests. | Data loss between programs |
| 2 | **Free and open source.** | Rising subscription costs and changing licence models |
| 3 | **Open, documented native format.** | Closed RVT/DWG formats and vendor lock-in |
| 4 | **Versioned schema with backward and forward compatibility.** Newer versions open old files, and files can be saved in older formats. | RVT upgrades are one-way |
| 5 | **Multithreaded core from day one.** | Revit is largely single-threaded and crashes on large models |
| 6 | **Cross-platform, Linux first.** | Revit is Windows-only |
| 7 | **Open roadmap.** | Stagnant, opaque development |
| 8 | **Interoperability is core, not an add-on.** | Users cannot switch unless they can still exchange files with others |
| 9 | **Familiar to current users.** Follow Revit, AutoCAD and CYPE conventions. Departures need a recorded decision ([ADR 0007](../decisions/0007-ui-familiar-to-current-users.md)). | Learning a new UI is a switching cost |
| 10 | **One suite, many workspaces.** Disciplines are domains on a shared core, not separate programs. | Separate products duplicate infrastructure and lose data between them |
| 11 | **Local first, global by design.** Codes, units, language and drawing standards are pluggable ([ADR 0008](../decisions/0008-argentina-first-international-by-design.md)). | Most tools are built around one market's codes |
