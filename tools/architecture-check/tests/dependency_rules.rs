//! Runs the architecture's dependency rules against the real workspace.

use std::path::Path;
use std::process::Command;

#[test]
fn workspace_follows_the_dependency_rules() {
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
    let metadata = serde_json::from_slice(&output.stdout).expect("cargo metadata printed bad JSON");

    let violations = hito_architecture_check::check(&metadata).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        violations.is_empty(),
        "\n\nThe crate dependencies break the architecture's rules \
         (see Cargo.toml and docs/architecture/unified-core.md):\n\n{}\n",
        violations
            .iter()
            .map(|v| format!("  - {v}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
