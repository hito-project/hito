# 0008. Argentina first, international by design

- **Status:** Accepted
- **Date:** 2026-10-08

## Context

The founding user works in Argentina, where structural design follows the CIRSOC and INPRES-CIRSOC codes and drawings follow IRAM standards ([workflows](https://github.com/hito-project/hito/discussions/75)). The final product must not be limited to Argentina.

## Decision

Everything region-specific goes behind pluggable adapters, never in the core:

| Concern | First implementation | Mechanism |
|---|---|---|
| Design codes | CIRSOC 201 (concrete), CIRSOC 301 (steel), INPRES-CIRSOC 103 (seismic) | Design-code adapters |
| UI language | Spanish (Argentina) and English | Localisation (i18n) from day one |
| Units | Metric | Unit system in the core; display units are user preferences |
| Drawing standards | IRAM (line weights, title blocks, dimension styles) | Templates and standards packs |

## Consequences

- No hardcoded strings, units or code clauses in the core.
- Adding a country means adding adapters and packs (Eurocode, ACI, NSR-10…), not changing the core.
- Argentine design-code checks are required before the structural workflow is usable locally.
