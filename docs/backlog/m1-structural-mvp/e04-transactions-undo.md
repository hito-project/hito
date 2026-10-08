---
title: "E04: Transactions and undo"
labels: type:epic, area:core
milestone: "M1: Structural BIM MVP"
---

Every model change goes through a transaction, so changes are atomic, validated and undoable. Transactions are recorded as change sets, which lays the groundwork for future worksharing ([ADR 0006](docs/decisions/0006-civil-engineering-suite-scope.md)) without implementing it.

## Story: Undo and redo any change
labels: type:story, area:core

As a user, I want to undo and redo any model change, including commands that change many elements, so that I can experiment safely.

### Acceptance criteria
- [ ] One user command produces one undo step, however many elements it touched
- [ ] Undo and redo restore the exact previous state, including dependent elements
- [ ] The undo history is available as a list in the UI, with command names

## Story: Atomic, validated model changes
labels: type:story, area:core

As a developer, I want all mutations to happen inside transactions that validate before committing, so that an invalid model state is never persisted.

### Acceptance criteria
- [ ] The model can't be mutated outside a transaction
- [ ] A failed validation or a panic rolls the transaction back completely
- [ ] Domains can register validation rules

## Story: Changes are recorded as change sets
labels: type:story, area:core

As a future collaborator, I want each committed transaction recorded as a serializable change set, so that worksharing and history can be built later without redesigning the core.

### Acceptance criteria
- [ ] Every commit produces a change set (created, modified and deleted elements, with before and after values)
- [ ] Change sets serialize and deserialize losslessly
- [ ] Replaying change sets on the initial state reproduces the final model (tested)
