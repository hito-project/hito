---
title: "E12: Localization and units"
labels: type:epic, area:i18n
milestone: "M1: Structural BIM MVP"
---

Spanish (Argentina) and English UI, metric units, and no hardcoded strings or units anywhere ([ADR 0008](docs/decisions/0008-argentina-first-international-by-design.md)).

## Story: Switch UI language
labels: type:story, area:i18n

As a user, I want to use HITO in Spanish or English, so that I can work in my language.

### Acceptance criteria
- [ ] All user-facing text comes from translation catalogues (es-AR and en)
- [ ] The language can be switched in settings
- [ ] Contributors can add a language without code changes

## Story: Units with typed input
labels: type:story, area:i18n

As a user, I want to type values with units and see values in my preferred display units, so that I never convert by hand.

### Acceptance criteria
- [ ] The core stores values in SI. Display units and precision are user preferences.
- [ ] Input parsing accepts units (`3.5m`, `350cm`, `350`) and expressions (`3.5m + 20cm`)
- [ ] Number formatting follows the locale (for example a decimal comma in es-AR)

## Story: No hardcoded strings or units
labels: type:story, area:i18n

As a maintainer, I want CI to catch hardcoded user-facing strings and units, so that localisation never regresses.

### Acceptance criteria
- [ ] A lint or test fails when user-facing text bypasses the translation system
- [ ] A check fails when translation catalogues are missing keys
