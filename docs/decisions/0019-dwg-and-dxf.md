# 0019. DWG and DXF: our own pure-Rust codec, DXF first

- **Status:** Accepted
- **Date:** 2026-10-10

## Context

In Argentina, DWG is the exchange format between firms, and DWG round-trip fidelity is the adoption gate for 2D drafting ([E21](https://github.com/hito-project/hito/issues/65), [Discussion #84](https://github.com/hito-project/hito/discussions/84)). DWG is out of scope for M1 ([ADR 0011](0011-mvp-structural-bim.md)), but E21 needs a strategy. HITO is MIT OR Apache-2.0, so GPL libraries can't be linked ([ADR 0009](0009-license.md)), and dependencies should be Rust ([ADR 0005](0005-rust-first-stack.md)).

Spike [#7](https://github.com/hito-project/hito/issues/7) surveyed the options and the legal questions on 2026-10-10. The findings and sources are in [Discussion #123](https://github.com/hito-project/hito/discussions/123). No code was written.

**DXF and DWG hold the same thing.** Both store the AutoCAD drawing database: header, tables, blocks, entities and objects. DXF writes it as tagged text, DWG as compressed binary. Autodesk publishes the DXF reference. It doesn't publish DWG, but the Open Design Alliance (ODA) publishes a free 270-page specification of it (version 5.4.1, R13 to 2018). AutoCAD 2018 to 2026 all use the same DWG version, AC1032.

What the survey found:

| Option | License | DWG | DXF | Verdict |
|---|---|---|---|---|
| **LibreDWG** | GPL-3.0 | Reads all versions. Writing is unreliable after 2004. | via its tools | Can't be linked. Usable only as a separate program the user installs. C, with a history of memory-safety CVEs. |
| **ODA SDKs** | proprietary, confidential | everything | everything | Binary only, $3,000 the first year and $2,250 after, 100 copies a year on the cheapest commercial tier, and the right to distribute ends with the membership. Doesn't fit an open-source project. |
| **ODA File Converter** | free, but "non-commercial applications only" for non-members | converts | converts | Our users do paid work. Can't be shipped or relied on. |
| **ACadSharp** (C#) | MIT | Reads 2000 to 2018, writes most | Reads and writes | The best permissively licensed DWG code. Five years old, 30 contributors. |
| **`dxf` crate** | MIT | no | Reads and writes R10 to 2018 | Mature, but no HATCH and no MLEADER. |
| **acadrust** | MPL-2.0 | Reads and writes R12 to 2018 | Reads and writes | The most complete Rust crate. Nine months old, open panic and out-of-memory issues on bad files. Its DWG code mirrors ACadSharp's layout without crediting it, and cites LibreDWG's GPL sources. |
| **cadkit** | MIT OR Apache-2.0 | Reads AC1014 to AC1032 | Reads, writes ASCII | Clean-room with a provenance ledger, but nine days old. |

Legal points from the spike (not legal advice; open questions are in [#130](https://github.com/hito-project/hito/issues/130)):

- **GPL.** The FSF treats programs that talk through pipes, sockets and command-line arguments as separate programs. Converting a DWG to a DXF file with LibreDWG's command-line tool is that case.
- **Trademark.** The USPTO refused "DWG" as a trademark for Autodesk as merely descriptive, and the application was abandoned in 2016. Autodesk still treats DWG as its mark, sued the ODA in 2006 over writing the TrustedDWG watermark, and made the ODA drop DWG from its product names in 2010. TrustedDWG is a registered Autodesk trademark.
- **Reverse engineering.** The EU allows decompiling for interoperability and holds that data file formats aren't protected by copyright. In the US it can be fair use, but a license can forbid it. Argentine law doesn't prohibit reverse engineering, but unlike the EU's it has no explicit exception for decompiling, so how Ley 11.723 applies to decompilation is untested. Studying file formats isn't affected. HITO doesn't need to decompile anything: the published specification, the DXF reference, permissively licensed code and sample files are enough.

## Decision

### 1. One drawing-database crate, two codecs

HITO reads and writes DXF and DWG through **its own pure-Rust crate** for the AutoCAD drawing database, with a DXF codec and a DWG codec on one document model. The crate knows nothing about HITO elements. Import and export adapters behind the importer/exporter port ([ADR 0004](0004-unified-core-ports-and-adapters.md)) map between it and the element store. Objects HITO doesn't understand are kept and written back.

### 2. Order and versions

| | Read | Write |
|---|---|---|
| **DXF** | First. R12 to 2018, ASCII and binary. | First. AC1032 (2018) by default, AC1015 (2000) as an option. |
| **DWG** | Second, and the adoption gate. AC1015 to AC1032. | Third, after reading passes the corpus. AC1032 first, then AC1015. |

### 3. Sources: published specifications and permissive code only

- The DWG codec is written from the ODA specification and from ACadSharp (MIT). Ported parts keep ACadSharp's copyright notice.
- The DXF codec is written from Autodesk's DXF reference, with dxf-rs and ezdxf (both MIT) as references.
- No code from GPL projects (LibreDWG, libdxfrw) or vendor SDKs is read or ported. Nobody decompiles Autodesk or ODA software. Contributors bound by an ODA or Autodesk developer agreement don't write codec code.
- Every source of format knowledge is recorded in a provenance file in the crate.

### 4. Check existing crates first

Before the DWG reader is written, spike [#126](https://github.com/hito-project/hito/issues/126) runs acadrust and cadkit against the test corpus, a fuzzer and a provenance review. If one passes, a new ADR can adopt it as the codec behind the same port. acadrust would also need MPL-2.0 added to `deny.toml`.

### 5. An optional converter until then

Until HITO reads DWG itself, an optional importer can run a converter **the user installed**: LibreDWG's `dwg2dxf` or the ODA File Converter. HITO never ships, downloads or links them, talks to them only through files and command-line arguments, and tells the user each one's license.

### 6. Validation in CI

As with IFC ([ADR 0016](0016-ifc-export.md)), written files are checked by independent readers in CI: ezdxf's audit, ACadSharp, and LibreDWG's tools. They run as tools, aren't shipped or linked, and so don't affect the license. Readers are fuzzed, because DWG and DXF files from other firms are untrusted input. Before a release, files are opened in at least two other CAD programs.

### 7. Trademark rules

"DWG" and ".dwg" are used only to describe the files HITO opens and saves. They're never part of a product, crate or feature name. HITO never writes the TrustedDWG watermark or the word "AutoCAD" into files, and never claims AutoCAD certification. The about box and documentation say that DWG and AutoCAD are trademarks of Autodesk.

## Consequences

- HITO has no DWG or DXF dependency, stays pure Rust and permissively licensed, and controls fidelity. The ODA's cost, confidentiality and distribution limits are avoided.
- DXF arrives early and cheaply, and gives E09's generated drawings an export format before DWG exists.
- DWG is a large job: several person-months for a reader of the 2D content civil drawings use, more for a writer that AutoCAD opens cleanly. Porting ACadSharp's proven structure reduces the risk. Spike #126 may remove most of it.
- We must maintain a provenance file and review it in pull requests that touch the codecs.
- CI needs Python (ezdxf), .NET (ACadSharp) and LibreDWG's tools for validation, and a corpus of real drawings from Argentine practice.
- A lawyer should answer the questions in [#130](https://github.com/hito-project/hito/issues/130) before a release reads or writes DWG.
- New work: the drawing-database crate with the DXF codec ([#124](https://github.com/hito-project/hito/issues/124)), CI validation and the corpus ([#125](https://github.com/hito-project/hito/issues/125)), the crate evaluation spike ([#126](https://github.com/hito-project/hito/issues/126)), the DWG reader ([#127](https://github.com/hito-project/hito/issues/127)), the DWG writer ([#128](https://github.com/hito-project/hito/issues/128)), the optional converter ([#129](https://github.com/hito-project/hito/issues/129)) and the legal review ([#130](https://github.com/hito-project/hito/issues/130)).
