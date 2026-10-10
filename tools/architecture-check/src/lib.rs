//! Checks that HITO's crates follow the dependency rules of the architecture
//! ([ADR 0004](https://github.com/hito-project/hito/blob/main/docs/decisions/0004-unified-core-ports-and-adapters.md)):
//!
//! - The core depends on no other HITO crate.
//! - A domain depends on the core and other domains only.
//! - An adapter depends on the core, domains and other adapters only.
//! - Nothing depends on the app or on the tools.
//! - Only the app may use a UI toolkit, directly or through any dependency.
//!
//! A crate's layer comes from where it lives in the repository, so a new crate
//! is checked as soon as it's added under `crates/`. The check reads the output
//! of `cargo metadata` and runs in `cargo test --workspace`
//! (see `tests/dependency_rules.rs`).

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::path::Path;

use serde_json::Value;

/// Crates that belong to a UI toolkit. Only the app may depend on them.
pub const UI_CRATES: &[&str] = &[
    "eframe",
    "egui",
    "egui-winit",
    "egui-wgpu",
    "egui_glow",
    "winit",
    "iced",
    "slint",
    "gtk",
    "gtk4",
    "tao",
    "tauri",
    "dioxus",
    "druid",
    "xilem",
    "floem",
    "vizia",
    "freya",
    "makepad-widgets",
];

/// The architecture layer a workspace crate belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Core,
    Domain,
    Adapter,
    App,
    Tool,
}

impl Layer {
    /// The layer of a crate, from its manifest path relative to the workspace root.
    pub fn of(relative_manifest: &Path) -> Option<Layer> {
        let parts: Vec<_> = relative_manifest
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
        match parts.as_slice() {
            ["crates", "core", ..] => Some(Layer::Core),
            ["crates", "domains", ..] => Some(Layer::Domain),
            ["crates", "adapters", ..] => Some(Layer::Adapter),
            ["crates", "app", ..] => Some(Layer::App),
            ["tools", ..] => Some(Layer::Tool),
            _ => None,
        }
    }

    /// Whether a crate in this layer may depend on a crate in `other`.
    fn may_depend_on(self, other: Layer) -> bool {
        use Layer::*;
        match self {
            Core => false,
            Domain => matches!(other, Core | Domain),
            Adapter => matches!(other, Core | Domain | Adapter),
            App => matches!(other, Core | Domain | Adapter),
            Tool => true,
        }
    }

    fn may_use_ui(self) -> bool {
        matches!(self, Layer::App | Layer::Tool)
    }
}

/// A broken dependency rule, with the chain of crates that breaks it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub rule: String,
    pub chain: Vec<String>,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.rule, self.chain.join(" -> "))
    }
}

/// Checks the dependency rules against the JSON printed by
/// `cargo metadata --format-version 1`. Returns every violation found, or an
/// error if the metadata can't be read or a crate has no layer.
pub fn check(metadata: &Value) -> Result<Vec<Violation>, String> {
    let root = metadata["workspace_root"]
        .as_str()
        .ok_or("cargo metadata has no workspace_root")?;
    let members: HashSet<&str> = metadata["workspace_members"]
        .as_array()
        .ok_or("cargo metadata has no workspace_members")?
        .iter()
        .filter_map(Value::as_str)
        .collect();

    let mut names = HashMap::new();
    let mut layers = HashMap::new();
    for package in metadata["packages"].as_array().ok_or("no packages")? {
        let id = package["id"].as_str().ok_or("package without id")?;
        let name = package["name"].as_str().ok_or("package without name")?;
        names.insert(id, name);
        if members.contains(id) {
            let manifest = Path::new(package["manifest_path"].as_str().unwrap_or_default());
            let relative = manifest.strip_prefix(root).unwrap_or(manifest);
            let layer = Layer::of(relative).ok_or_else(|| {
                format!(
                    "crate `{name}` at {} is outside every layer. Put it under \
                     crates/core, crates/domains, crates/adapters, crates/app or tools.",
                    relative.display()
                )
            })?;
            layers.insert(id, layer);
        }
    }

    let mut edges: HashMap<&str, Vec<&str>> = HashMap::new();
    for node in metadata["resolve"]["nodes"]
        .as_array()
        .ok_or("cargo metadata has no resolve graph")?
    {
        let id = node["id"].as_str().ok_or("node without id")?;
        let deps = node["deps"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|dep| dep["pkg"].as_str())
            .collect();
        edges.insert(id, deps);
    }

    let mut violations = Vec::new();
    let mut member_ids: Vec<&str> = layers.keys().copied().collect();
    member_ids.sort_by_key(|id| names[id]);
    for start in member_ids {
        let layer = layers[start];
        // Breadth-first search, remembering how each crate was reached.
        let mut reached_from: HashMap<&str, &str> = HashMap::new();
        let mut queue = VecDeque::from([start]);
        while let Some(current) = queue.pop_front() {
            for &dep in edges.get(current).into_iter().flatten() {
                if dep == start || reached_from.contains_key(dep) {
                    continue;
                }
                reached_from.insert(dep, current);
                queue.push_back(dep);

                let rule = if let Some(&dep_layer) = layers.get(dep) {
                    (!layer.may_depend_on(dep_layer))
                        .then(|| format!("{layer:?} must not depend on {dep_layer:?}"))
                } else {
                    (UI_CRATES.contains(&names[dep]) && !layer.may_use_ui())
                        .then(|| format!("{layer:?} must not depend on a UI toolkit"))
                };
                if let Some(rule) = rule {
                    let mut chain = vec![names[dep].to_string()];
                    let mut at = dep;
                    while at != start {
                        at = reached_from[at];
                        chain.push(names[at].to_string());
                    }
                    chain.reverse();
                    violations.push(Violation { rule, chain });
                }
            }
        }
    }
    Ok(violations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Builds minimal `cargo metadata` output. `crates` lists
    /// (name, path relative to the root or None for a registry crate, deps).
    fn metadata(crates: &[(&str, Option<&str>, &[&str])]) -> Value {
        let id = |name: &str| format!("{name} 0.0.0");
        json!({
            "workspace_root": "/ws",
            "workspace_members": crates.iter()
                .filter(|(_, path, _)| path.is_some())
                .map(|(name, _, _)| id(name)).collect::<Vec<_>>(),
            "packages": crates.iter().map(|(name, path, _)| json!({
                "id": id(name),
                "name": name,
                "manifest_path": format!("/ws/{}/Cargo.toml", path.unwrap_or("registry")),
            })).collect::<Vec<_>>(),
            "resolve": { "nodes": crates.iter().map(|(name, _, deps)| json!({
                "id": id(name),
                "deps": deps.iter().map(|d| json!({ "pkg": id(d) })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>() },
        })
    }

    fn rules(metadata: &Value) -> Vec<String> {
        check(metadata)
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn the_intended_structure_passes() {
        let m = metadata(&[
            ("hito-core", Some("crates/core"), &[]),
            ("dom", Some("crates/domains/dom"), &["hito-core"]),
            ("ad", Some("crates/adapters/ad"), &["hito-core", "dom"]),
            (
                "hito",
                Some("crates/app"),
                &["hito-core", "dom", "ad", "eframe"],
            ),
            ("eframe", None, &["winit"]),
            ("winit", None, &[]),
        ]);
        assert_eq!(rules(&m), Vec::<String>::new());
    }

    #[test]
    fn core_depending_on_a_domain_fails() {
        let m = metadata(&[
            ("hito-core", Some("crates/core"), &["dom"]),
            ("dom", Some("crates/domains/dom"), &[]),
        ]);
        assert_eq!(
            rules(&m),
            ["Core must not depend on Domain: hito-core -> dom"]
        );
    }

    #[test]
    fn a_ui_toolkit_reached_transitively_fails_with_its_chain() {
        let m = metadata(&[
            ("hito-core", Some("crates/core"), &["helper"]),
            ("helper", None, &["winit"]),
            ("winit", None, &[]),
        ]);
        assert_eq!(
            rules(&m),
            ["Core must not depend on a UI toolkit: hito-core -> helper -> winit"]
        );
    }

    #[test]
    fn adapters_must_not_depend_on_the_app() {
        let m = metadata(&[
            ("ad", Some("crates/adapters/ad"), &["hito"]),
            ("hito", Some("crates/app"), &[]),
        ]);
        assert_eq!(rules(&m), ["Adapter must not depend on App: ad -> hito"]);
    }

    #[test]
    fn a_crate_outside_every_layer_is_an_error() {
        let m = metadata(&[("stray", Some("stray"), &[])]);
        assert!(check(&m).unwrap_err().contains("outside every layer"));
    }
}
