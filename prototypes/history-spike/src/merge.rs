//! Three-way merge of two branches, at field level.
//!
//! Each branch's change sets since the common ancestor fold into one net
//! delta per element. Conflicts are the same field of the same element set
//! to different values, or an element modified on one side and deleted on
//! the other. The merge applies their net delta to ours as an ordinary
//! transaction, so recompute and the domains' rules run on the result:
//! a merge can be valid data but wrong engineering, and the rules are what
//! catch it. Spike #156: properties, creations, deletions and relationships
//! only.

use std::collections::BTreeMap;

use hito_core::{
    ChangeSet, Command, Element, ElementChange, ElementId, ElementStore, FieldChange, Relationship,
    RelationshipChange, StoreError, Value,
};

/// One branch's net effect on an element since the common ancestor.
#[derive(Debug, Clone)]
pub enum Net {
    Created(Element),
    Deleted,
    /// Property name → value after (`None` = cleared).
    Modified(BTreeMap<String, Option<Value>>),
}

pub fn net(changes: &[ChangeSet]) -> (BTreeMap<ElementId, Net>, BTreeMap<Relationship, bool>) {
    let mut elements: BTreeMap<ElementId, Net> = BTreeMap::new();
    let mut relationships = BTreeMap::new();
    for changes in changes {
        for change in changes.elements() {
            let id = change.id();
            match change {
                ElementChange::Created(e) => {
                    elements.insert(id, Net::Created(e.clone()));
                }
                ElementChange::Deleted(_) => {
                    // Created and deleted on the same branch: nothing.
                    if matches!(elements.remove(&id), Some(Net::Created(_))) {
                        continue;
                    }
                    elements.insert(id, Net::Deleted);
                }
                ElementChange::Modified { fields, .. } => {
                    let entry = elements.entry(id).or_insert(Net::Modified(BTreeMap::new()));
                    for field in fields {
                        let FieldChange::Property { name, after, .. } = field else {
                            continue; // spike: properties only
                        };
                        match entry {
                            Net::Created(e) => {
                                let mut e2 = e.clone();
                                e2 = match after {
                                    Some(v) => e2.with_property(name.clone(), v.clone()),
                                    None => e2,
                                };
                                *e = e2;
                            }
                            Net::Modified(props) => {
                                props.insert(name.clone(), after.clone());
                            }
                            Net::Deleted => {}
                        }
                    }
                }
            }
        }
        for change in changes.relationships() {
            match change {
                RelationshipChange::Added(r) => relationships.insert(r.clone(), true),
                RelationshipChange::Removed(r) => relationships.insert(r.clone(), false),
            };
        }
    }
    (elements, relationships)
}

#[derive(Debug)]
pub enum Conflict {
    /// Both changed a property to different values.
    Property {
        element: ElementId,
        name: String,
        ours: Option<Value>,
        theirs: Option<Value>,
    },
    /// One side modified the element, the other deleted it.
    ModifyDelete {
        element: ElementId,
        deleted_by_us: bool,
    },
}

#[derive(Debug)]
pub struct Merged {
    pub changes: ChangeSet,
    pub conflicts: Vec<Conflict>,
    /// Properties that recompute changed after the merge: derived values
    /// that a plain replay of their change sets would have left stale.
    pub recomputed: usize,
}

/// Merges `theirs` (change sets since the common ancestor) into the store,
/// which is at our head, given ours since the same ancestor. Conflicts keep
/// our value; the UI would let the user choose.
pub fn merge(
    store: &mut ElementStore,
    ours: &[ChangeSet],
    theirs: &[ChangeSet],
) -> Result<Merged, StoreError> {
    let (our_net, _) = net(ours);
    let (their_net, their_relationships) = net(theirs);
    let mut conflicts = Vec::new();
    let mut tx = store.transaction_for(Command::new("Merge"));
    let mut expected = 0;
    // Creations first, then relationships, then edits, then deletions.
    for change in their_net.values() {
        if let Net::Created(e) = change {
            tx.insert(e.clone())?;
        }
    }
    for (r, added) in &their_relationships {
        let exists = tx.view().relationships_from(r.source()).any(|x| x == r);
        let ends_exist = tx.view().get(r.source()).is_some() && tx.view().get(r.target()).is_some();
        match (added, exists) {
            (true, false) if ends_exist => tx.relate(r.clone())?,
            (false, true) => tx.unrelate(r)?,
            _ => {}
        }
    }
    for (&id, change) in &their_net {
        match (change, our_net.get(&id)) {
            (Net::Modified(_), Some(Net::Deleted)) => conflicts.push(Conflict::ModifyDelete {
                element: id,
                deleted_by_us: true,
            }),
            (Net::Deleted, Some(Net::Modified(_))) => conflicts.push(Conflict::ModifyDelete {
                element: id,
                deleted_by_us: false,
            }),
            (Net::Modified(props), ours) => {
                let our_props = match ours {
                    Some(Net::Modified(p)) => Some(p),
                    _ => None,
                };
                for (name, after) in props {
                    if let Some(our_after) = our_props.and_then(|p| p.get(name)) {
                        if our_after != after {
                            conflicts.push(Conflict::Property {
                                element: id,
                                name: name.clone(),
                                ours: our_after.clone(),
                                theirs: after.clone(),
                            });
                        }
                        continue;
                    }
                    expected += 1;
                    match after {
                        Some(v) => tx.set_property(id, name, v.clone())?,
                        None => {
                            tx.clear_property(id, name)?;
                        }
                    }
                }
            }
            (Net::Deleted, _) => {
                if tx.view().get(id).is_some() {
                    tx.delete(id)?;
                }
            }
            (Net::Created(_), _) => {}
        }
    }
    let changes = tx.commit()?;
    let created = changes
        .elements()
        .iter()
        .filter(|c| matches!(c, ElementChange::Created(_)))
        .count();
    let modified_fields: usize = changes
        .elements()
        .iter()
        .map(|c| match c {
            ElementChange::Modified { fields, .. } => fields.len(),
            _ => 0,
        })
        .sum();
    let _ = created;
    Ok(Merged {
        recomputed: modified_fields.saturating_sub(expected),
        changes,
        conflicts,
    })
}
