//! Runs the architecture's dependency rules against the real workspace.

use std::path::Path;
use std::process::Command;

use hito_architecture_check::non_rust;
use serde_json::Value;

/// The workspace's `cargo metadata`, for every platform.
fn metadata() -> Value {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--manifest-path"])
        .arg(&manifest)
        .output()
        .expect("failed to run cargo metadata");
    assert!(
        output.status.success(),
        "cargo metadata failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("cargo metadata printed bad JSON")
}

fn bullets(items: impl IntoIterator<Item = impl std::fmt::Display>) -> String {
    items
        .into_iter()
        .map(|item| format!("  - {item}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn workspace_follows_the_dependency_rules() {
    let violations = hito_architecture_check::check(&metadata()).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        violations.is_empty(),
        "\n\nThe crate dependencies break the architecture's rules \
         (see Cargo.toml and docs/architecture/unified-core.md):\n\n{}\n",
        bullets(violations)
    );
}

#[test]
fn every_non_rust_dependency_is_justified() {
    let report =
        non_rust::check(&metadata(), non_rust::JUSTIFIED).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        report.unjustified.is_empty(),
        "\n\nThese dependencies build or link non-Rust code. ADR 0005 asks for a \
         recorded justification: why no Rust option is good enough, and whether it \
         can be replaced later. Add each one, with its reason, to `JUSTIFIED` in \
         tools/architecture-check/src/non_rust.rs, or drop the dependency \
         (`cargo tree -i <crate>` shows what pulls it in):\n\n{}\n",
        bullets(&report.unjustified)
    );
    assert!(
        report.stale.is_empty(),
        "\n\nThese entries in `JUSTIFIED` (tools/architecture-check/src/non_rust.rs) \
         no longer match a non-Rust dependency. Remove them:\n\n{}\n",
        bullets(&report.stale)
    );
}
