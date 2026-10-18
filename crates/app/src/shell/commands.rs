//! The commands the shell registers at startup.
//!
//! Aliases are Revit's default keyboard shortcuts, from Autodesk's Revit
//! Keyboard Shortcuts Guide. A command with no default shortcut in Revit
//! has no alias; users can add their own with #25.

use super::Action;
use super::layout::Tab;
use crate::command::{Command, Registry, Run, Tool};
use crate::tools;

/// Revit's ribbon commands for M1 (#33 to #41, #36, #163), which come with
/// their stories, and the window commands, which work now.
const COMMANDS: &[Command] = &[
    unavailable("BEAM", "command-beam", Some("BM")),
    unavailable("WALL", "command-wall", Some("WA")),
    unavailable("COLUMN", "command-column", Some("CL")),
    unavailable("SLAB", "command-slab", Some("SB")),
    unavailable("ISOLATEDFOOTING", "command-isolated-footing", None),
    tool("LEVEL", "command-level", Some("LL"), tools::level::start),
    tool("GRID", "command-grid", Some("GR"), tools::grid::start),
    unavailable("MATERIALS", "command-materials", None),
    unavailable("UNITS", "command-project-units", Some("UN")),
    window("VIEW3D", "action-new-3d-view", None, Action::New3dView),
    window(
        "CLOSEINACTIVE",
        "action-close-inactive-views",
        None,
        Action::CloseInactiveViews,
    ),
    window("TABVIEWS", "action-tab-views", None, Action::TabViews),
    window(
        "TILEVIEWS",
        "action-tile-views",
        Some("WT"),
        Action::TileViews,
    ),
    window(
        "PROPERTIES",
        "panel-properties",
        Some("PP"),
        Action::TogglePanel(Tab::Properties),
    ),
    window(
        "PROJECTBROWSER",
        "panel-project-browser",
        None,
        Action::TogglePanel(Tab::ProjectBrowser),
    ),
    window(
        "ZOOMFIT",
        "action-zoom-to-fit",
        Some("ZF"),
        Action::ZoomToFit,
    ),
];

const fn unavailable(
    name: &'static str,
    label: &'static str,
    alias: Option<&'static str>,
) -> Command {
    Command {
        name,
        label,
        alias,
        run: Run::Unavailable,
    }
}

const fn tool(
    name: &'static str,
    label: &'static str,
    alias: Option<&'static str>,
    start: fn() -> Box<dyn Tool>,
) -> Command {
    Command {
        name,
        label,
        alias,
        run: Run::Tool(start),
    }
}

const fn window(
    name: &'static str,
    label: &'static str,
    alias: Option<&'static str>,
    action: Action,
) -> Command {
    Command {
        name,
        label,
        alias,
        run: Run::Window(action),
    }
}

/// Every command the shell has.
pub fn registry() -> Registry {
    let mut registry = Registry::new();
    for &command in COMMANDS {
        registry
            .register(command)
            .expect("the shell's names and aliases are unique");
    }
    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{I18n, Locale};

    #[test]
    fn every_command_has_a_label_in_every_language() {
        let registry = registry();
        for locale in Locale::ALL {
            let i18n = I18n::new(locale);
            for command in registry.iter() {
                assert!(!i18n.tr(command.label).starts_with('⟨'), "{}", command.name);
            }
        }
    }

    #[test]
    fn names_are_upper_case_letters_and_digits() {
        for command in registry().iter() {
            let plain = |s: &str| {
                s.chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            };
            assert!(plain(command.name), "{}", command.name);
            assert!(command.alias.is_none_or(plain), "{}", command.name);
        }
    }
}
