//! The shell: HITO's main window, laid out like Revit's
//! ([ADR 0007](https://github.com/hito-project/hito/blob/main/docs/decisions/0007-ui-familiar-to-current-users.md)).
//! The ribbon is at the top and the command line at the bottom; the
//! properties, the project browser and the views are dock tabs, which the
//! user can drag, stack, split and float.
//!
//! Every button, typed command and shortcut runs a registered
//! [command](crate::command).
//!
//! The shell is built on egui and egui_dock, and is the only part of HITO
//! that uses them
//! ([ADR 0015](https://github.com/hito-project/hito/blob/main/docs/decisions/0015-ui-toolkit.md)).
//! Its layout and theme are saved through eframe's storage when the window
//! closes.

mod command_line;
mod commands;
mod layout;
mod panels;
mod ribbon;
mod theme;

use std::collections::HashMap;

use eframe::egui;
use egui_dock::DockArea;
use hito_core::{ElementStore, SchemaRegistry};

pub use commands::registry;
pub use layout::{Layout, Tab, ViewId};
pub use ribbon::Action;
pub use theme::Theme;

use crate::command::{InputFormat, Outcome, Session};
use crate::i18n::{I18n, Locale};
use crate::viewport::Viewport;
use command_line::{CommandLine, Request};
use ribbon::Pressed;

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
    commands: Session,
    command_line: CommandLine,
    input: InputFormat,
    /// The model being edited. Opening and saving files comes with E10
    /// (#46).
    project: ElementStore,
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
        let mut schemas = SchemaRegistry::new();
        hito_structural::register(&mut schemas).expect("the structural schema registers");
        Self {
            i18n: I18n::new(locale),
            ribbon: Default::default(),
            layout,
            theme: storage
                .and_then(|s| eframe::get_value(s, THEME_KEY))
                .unwrap_or_default(),
            viewports: HashMap::new(),
            commands: Session::new(registry()),
            command_line: Default::default(),
            input: InputFormat::new(locale),
            project: ElementStore::new(schemas),
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

    /// The registered commands, the one running and what the command line
    /// last said.
    pub fn commands(&self) -> &Session {
        &self.commands
    }

    /// The commands, to register more.
    pub fn commands_mut(&mut self) -> &mut Session {
        &mut self.commands
    }

    /// The model being edited.
    pub fn project(&self) -> &ElementStore {
        &self.project
    }

    /// The renderer of an open 3D view, once it has been shown.
    pub fn viewport(&self, view: ViewId) -> Option<&Viewport> {
        self.viewports.get(&view)
    }

    /// Runs the command called `name`, as its ribbon button does.
    pub fn run(&mut self, name: &str) {
        let outcome = self.commands.start(name, &self.i18n);
        self.follow(outcome);
    }

    /// Cancels the running command, as Esc does.
    pub fn cancel(&mut self) {
        self.commands.cancel(&self.i18n);
        self.command_line.clear();
    }

    /// Changes the window.
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

    fn enter(&mut self, text: &str) {
        let outcome = self
            .commands
            .enter(text, &self.i18n, &self.input, &mut self.project);
        self.follow(outcome);
    }

    fn follow(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Nothing => {}
            Outcome::Window(action) => self.apply(action),
            // Undo and redo (#19) will keep the change set as one step.
            Outcome::Committed(_changes) => {}
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

        // Esc cancels the running command wherever the focus is.
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.cancel();
        }
        if let Some(name) = self
            .command_line
            .shortcuts(ui.ctx(), self.commands.registry())
        {
            self.run(name);
        }

        let (layout, theme) = (&self.layout, self.theme);
        let state = ribbon::State {
            i18n: &self.i18n,
            commands: self.commands.registry(),
            is_open: |panel| layout.is_open(panel),
            theme,
        };
        let pressed = egui::Panel::top("ribbon")
            .show(ui, |ui| self.ribbon.ui(ui, &state))
            .inner;
        match pressed {
            Some(Pressed::Command(name)) => self.run(name),
            Some(Pressed::Action(action)) => self.apply(action),
            None => {}
        }

        let request = egui::Panel::bottom("command-line")
            .show(ui, |ui| {
                self.command_line.ui(ui, &self.i18n, &self.commands)
            })
            .inner;
        match request {
            Some(Request::Enter(text)) => self.enter(&text),
            Some(Request::Run(name)) => self.run(name),
            None => {}
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
