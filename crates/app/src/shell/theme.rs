//! The colour theme of the window. Light is the default, as in Revit; Revit
//! 2024 and later can also be dark.

use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Theme {
    #[default]
    Light,
    Dark,
    /// Light or dark, as the operating system is set.
    System,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::Light, Theme::Dark, Theme::System];

    /// The message key of its name.
    pub fn label(self) -> &'static str {
        match self {
            Theme::Light => "theme-light",
            Theme::Dark => "theme-dark",
            Theme::System => "theme-system",
        }
    }

    pub(super) fn preference(self) -> egui::ThemePreference {
        match self {
            Theme::Light => egui::ThemePreference::Light,
            Theme::Dark => egui::ThemePreference::Dark,
            Theme::System => egui::ThemePreference::System,
        }
    }
}
