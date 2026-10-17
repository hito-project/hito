//! The shell: HITO's main window, laid out like Revit's
//! ([ADR 0007](https://github.com/hito-project/hito/blob/main/docs/decisions/0007-ui-familiar-to-current-users.md)).
//! The ribbon is at the top; the properties, the project browser and the
//! views are dock tabs, which the user can drag, stack, split and float.
//!
//! The shell is built on egui and egui_dock, and is the only part of HITO
//! that uses them
//! ([ADR 0015](https://github.com/hito-project/hito/blob/main/docs/decisions/0015-ui-toolkit.md)).
//! Its layout and theme are saved through eframe's storage when the window
//! closes.

mod layout;
mod panels;
mod ribbon;
mod theme;

use std::collections::HashMap;

use eframe::egui;
use egui_dock::DockArea;

pub use layout::{Layout, Tab, ViewId};
pub use ribbon::Action;
pub use theme::Theme;

use crate::i18n::{I18n, Locale};
use crate::viewport::Viewport;

/// The key the layout is saved under in eframe's storage.
const LAYOUT_KEY: &str = "layout";
/// The key the theme is saved under, next to the layout.
const THEME_KEY: &str = "theme";

/// The main window.
pub struct Shell {
    i18n: I18n,
    ribbon: ribbon::Ribbon,
    layout: Layout,
    theme: Theme,
    viewports: HashMap<ViewId, Viewport>,
}

impl Shell {
    /// The window as the user left it last time, or Revit's default layout
    /// the first time.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::restore(cc.storage, Locale::default())
    }

    /// The window with the layout saved in `storage`, if there is one it can
    /// read, and its text in `locale`.
    pub fn restore(storage: Option<&dyn eframe::Storage>, locale: Locale) -> Self {
        let layout = storage
            .and_then(|s| eframe::get_value::<Layout>(s, LAYOUT_KEY))
            .map(Layout::repair)
            .unwrap_or_default();
        Self {
            i18n: I18n::new(locale),
            ribbon: Default::default(),
            layout,
            theme: storage
                .and_then(|s| eframe::get_value(s, THEME_KEY))
                .unwrap_or_default(),
            viewports: HashMap::new(),
        }
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn theme(&self) -> Theme {
        self.theme
    }

    pub fn i18n(&self) -> &I18n {
        &self.i18n
    }

    /// The renderer of an open 3D view, once it has been shown.
    pub fn viewport(&self, view: ViewId) -> Option<&Viewport> {
        self.viewports.get(&view)
    }

    /// Does what a ribbon button asks.
    pub fn apply(&mut self, action: Action) {
        let layout = &mut self.layout;
        match action {
            Action::New3dView => {
                layout.new_3d_view();
            }
            Action::CloseInactiveViews => layout.close_inactive_views(),
            Action::TabViews => layout.tab_views(),
            Action::TileViews => layout.tile_views(),
            Action::TogglePanel(panel) => layout.toggle_panel(panel),
            Action::SetTheme(theme) => self.theme = theme,
        }
    }
}

impl eframe::App for Shell {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // egui keeps a theme of its own, which it also saves; ours wins.
        let preference = self.theme.preference();
        if ui.ctx().options(|o| o.theme_preference) != preference {
            ui.ctx().set_theme(preference);
        }

        let (layout, theme) = (&self.layout, self.theme);
        let action = egui::Panel::top("ribbon")
            .show(ui, |ui| {
                self.ribbon
                    .ui(ui, &self.i18n, |panel| layout.is_open(panel), theme)
            })
            .inner;
        if let Some(action) = action {
            self.apply(action);
        }

        let render_state = frame.wgpu_render_state();
        let mut tabs = panels::Tabs {
            i18n: &self.i18n,
            views: self.layout.views().to_vec(),
            active_view: self.layout.active_view(),
            viewports: &mut self.viewports,
            render_state,
            open: None,
        };
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                DockArea::new(&mut self.layout.dock).show_inside(ui, &mut tabs);
            });
        if let Some(view) = tabs.open {
            self.layout.open_view(view);
        }
        self.layout.track_focus();

        // Free the textures of views that were closed.
        let open = self.layout.open_views();
        self.viewports.retain(|view, viewport| {
            let keep = open.contains(view);
            if !keep {
                viewport.free(render_state);
            }
            keep
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, LAYOUT_KEY, &self.layout);
        eframe::set_value(storage, THEME_KEY, &self.theme);
    }
}
