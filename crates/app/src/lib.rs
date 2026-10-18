//! The HITO desktop application: the [`Shell`] (the main window), its text in
//! each language ([`i18n`]), the 3D [`viewport`], the [`project`] being edited
//! and the placement [`tools`].
//!
//! This is the only crate that uses a UI toolkit. The core, domains and
//! adapters never see egui types
//! ([ADR 0015](https://github.com/hito-project/hito/blob/main/docs/decisions/0015-ui-toolkit.md)).

pub mod command;
pub mod i18n;
pub mod project;
pub mod shell;
pub mod tools;
pub mod viewport;

pub use shell::Shell;
