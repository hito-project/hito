---
title: "E10: Native file format"
labels: type:epic, area:file-format
milestone: "M1: Structural BIM MVP"
---

Save and open projects in HITO's own format. It must be open, documented and versioned ([principles 3 and 4](docs/vision/principles.md)). The storage engine is chosen in E01.

## Story: Save and open projects
labels: type:story, area:file-format

As a user, I want to save my project and open it later exactly as I left it, so that I can work across sessions.

### Acceptance criteria
- [ ] Save, Save As and Open, with the file extension decided in this story
- [ ] Every element, type, material, view setting and relationship round-trips losslessly (tested)
- [ ] Autosave and crash recovery

## Story: Older files open in newer versions
labels: type:story, area:file-format

As a user, I want files from older HITO versions to open in newer ones, so that upgrading never strands my projects.

### Acceptance criteria
- [ ] Files record their schema versions
- [ ] Schema migrations upgrade old data on open
- [ ] A test corpus of files from every released version opens in CI

## Story: Documented file format
labels: type:story, area:file-format

As a third-party developer, I want the native format documented, so that I can read HITO files without HITO and never be locked in.

### Acceptance criteria
- [ ] Format specification published in the repo docs
- [ ] A minimal reader example in the docs
