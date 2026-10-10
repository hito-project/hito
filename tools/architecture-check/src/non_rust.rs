//! Flags dependencies that aren't pure Rust
//! ([ADR 0005](https://github.com/hito-project/hito/blob/main/docs/decisions/0005-rust-first-stack.md)).
//!
//! A crate counts as non-Rust when it links a native library (its manifest has
//! a `links` key) or compiles or looks up native code in its build script (it
//! has a build-dependency on one of [`NATIVE_BUILD_TOOLS`]). Each one must be
//! listed in [`JUSTIFIED`] with the reason we accept it. The check covers
//! every platform, not only the one it runs on.

use std::collections::BTreeSet;

use serde_json::Value;

/// Build-dependencies that mean a crate builds or links non-Rust code.
pub const NATIVE_BUILD_TOOLS: &[&str] = &[
    "cc",
    "cmake",
    "bindgen",
    "cxx-build",
    "pkg-config",
    "system-deps",
    "vcpkg",
];

/// Why the toolkit's platform bindings are accepted.
const WINDOW_SYSTEM: &str = "binds a library the operating system provides only through a C \
     interface, to open windows and draw; comes with the UI toolkit (ADR 0015)";

/// Non-Rust dependencies we accept, with the justification ADR 0005 asks for.
/// A new entry needs its reason here, or a link to the ADR that records it.
pub const JUSTIFIED: &[(&str, &str)] = &[
    ("android-activity", WINDOW_SYSTEM),
    ("khronos-egl", WINDOW_SYSTEM),
    ("objc-sys", WINDOW_SYSTEM),
    ("smithay-client-toolkit", WINDOW_SYSTEM),
    ("wayland-backend", WINDOW_SYSTEM),
    ("wayland-sys", WINDOW_SYSTEM),
    ("x11-dl", WINDOW_SYSTEM),
    (
        "wasm-bindgen-shared",
        "pure Rust: its `links` key only stops two versions being linked together",
    ),
];

/// The outcome of [`check`].
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Non-Rust crates in the dependency graph with no justification.
    pub unjustified: Vec<String>,
    /// Justified crates that are no longer non-Rust dependencies, so their
    /// entry in [`JUSTIFIED`] should go.
    pub stale: Vec<String>,
}

/// Finds the non-Rust crates in the JSON printed by
/// `cargo metadata --format-version 1` and compares them with `justified`.
pub fn check(metadata: &Value, justified: &[(&str, &str)]) -> Result<Report, String> {
    let mut found = BTreeSet::new();
    for package in metadata["packages"].as_array().ok_or("no packages")? {
        let name = package["name"].as_str().ok_or("package without name")?;
        let links = package["links"].is_string();
        let builds_native = package["dependencies"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|dep| {
                dep["kind"] == "build"
                    && dep["name"]
                        .as_str()
                        .is_some_and(|n| NATIVE_BUILD_TOOLS.contains(&n))
            });
        if links || builds_native {
            found.insert(name);
        }
    }

    let justified: BTreeSet<&str> = justified.iter().map(|(name, _)| *name).collect();
    Ok(Report {
        unjustified: found
            .difference(&justified)
            .map(ToString::to_string)
            .collect(),
        stale: justified
            .difference(&found)
            .map(ToString::to_string)
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Builds minimal `cargo metadata` output. `crates` lists
    /// (name, links, build-dependencies).
    fn metadata(crates: &[(&str, Option<&str>, &[&str])]) -> Value {
        json!({
            "packages": crates.iter().map(|(name, links, build_deps)| json!({
                "name": name,
                "links": links,
                "dependencies": build_deps.iter()
                    .map(|d| json!({ "name": d, "kind": "build" }))
                    .chain([json!({ "name": "cc", "kind": null })])
                    .collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })
    }

    #[test]
    fn pure_rust_crates_pass() {
        // A normal (not build) dependency on `cc` doesn't count.
        let m = metadata(&[("serde", None, &["autocfg"])]);
        assert_eq!(check(&m, &[]).unwrap(), Report::default());
    }

    #[test]
    fn links_and_native_build_tools_are_flagged() {
        let m = metadata(&[
            ("libsqlite3-sys", Some("sqlite3"), &[]),
            ("manifold-sys", None, &["cmake"]),
        ]);
        assert_eq!(
            check(&m, &[]).unwrap().unjustified,
            ["libsqlite3-sys", "manifold-sys"]
        );
    }

    #[test]
    fn justified_crates_pass_and_unused_entries_are_stale() {
        let m = metadata(&[("x11-dl", None, &["pkg-config"])]);
        let report = check(&m, &[("x11-dl", "why"), ("gone-sys", "why")]).unwrap();
        assert_eq!(report.unjustified, Vec::<String>::new());
        assert_eq!(report.stale, ["gone-sys"]);
    }
}
