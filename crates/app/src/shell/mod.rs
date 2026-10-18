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
mod elements;
mod layout;
mod panels;
mod ribbon;
mod theme;

use std::collections::HashMap;
use std::sync::Arc;

use eframe::egui;
use egui_dock::DockArea;
use hito_core::{ChangeSet, Command, ElementId, ElementStore, SchemaRegistry};
use hito_geometry::GeometryCache;
use hito_manifold::Manifold;

pub use commands::registry;
pub use layout::{Layout, Tab, ViewId};
pub use ribbon::Action;
pub use theme::Theme;

use crate::command::{InputFormat, Outcome, Session};
use crate::i18n::{I18n, Locale};
use crate::project;
use crate::viewport::{Navigation, Scene, Viewport};
use command_line::{CommandLine, Request};
use elements::{Edit, Elements};
use ribbon::Pressed;

/// The key the layout is saved under in eframe's storage.
const LAYOUT_KEY: &str = "layout";
/// The key the theme is saved under, next to the layout.
const THEME_KEY: &str = "theme";
/// The key the 3D navigation setting is saved under.
const NAVIGATION_KEY: &str = "navigation";

/// The main window.
pub struct Shell {
    i18n: I18n,
    ribbon: ribbon::Ribbon,
    layout: Layout,
    theme: Theme,
    navigation: Navigation,
    viewports: HashMap<ViewId, Viewport>,
    /// Every element's mesh, and what the 3D views show of them.
    geometry: GeometryCache,
    scene: Scene,
    commands: Session,
    command_line: CommandLine,
    input: InputFormat,
    /// The model being edited. Opening and saving files comes with E10
    /// (#46).
    project: ElementStore,
    /// The element picked in the project browser, and edits in progress.
    elements: Elements,
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
        let i18n = I18n::new(locale);
        let project = project::new_project(schemas, &i18n);
        Self {
            i18n,
            ribbon: Default::default(),
            layout,
            theme: storage
                .and_then(|s| eframe::get_value(s, THEME_KEY))
                .unwrap_or_default(),
            navigation: storage
                .and_then(|s| eframe::get_value(s, NAVIGATION_KEY))
                .unwrap_or_default(),
            viewports: HashMap::new(),
            // The domains' generators come with their elements (#193, #37
            // to #41).
            geometry: GeometryCache::new(Arc::new(Manifold), Vec::new()),
            scene: Scene::default(),
            commands: Session::new(registry()),
            command_line: Default::default(),
            input: InputFormat::new(),
            project,
            elements: Elements::default(),
        }
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn theme(&self) -> Theme {
        self.theme
    }

    /// What dragging with the mouse wheel does in 3D views.
    pub fn navigation(&self) -> Navigation {
        self.navigation
    }

    /// What the 3D views show.
    pub fn scene(&self) -> &Scene {
        &self.scene
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
            Action::SetNavigation(navigation) => self.navigation = navigation,
            Action::ZoomToFit => {
                let viewport = layout
                    .active_view()
                    .and_then(|v| self.viewports.get_mut(&v));
                if let Some(viewport) = viewport {
                    viewport.zoom_to_fit(&self.scene);
                }
            }
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
            Outcome::Committed(changes) => self.committed(&changes),
        }
    }

    /// Brings the geometry and the views up to date with a committed change.
    fn committed(&mut self, changes: &ChangeSet) {
        let regenerated = self.geometry.update(&self.project, changes);
        self.scene.update(&self.geometry, &regenerated);
    }

    /// Makes a change the user asked for in the browser or the properties,
    /// in one transaction.
    fn edit(&mut self, edit: Edit) {
        self.elements.clear_error();
        match edit {
            Edit::Set {
                element,
                property,
                value,
            } => {
                // Undo (#19) lists it by this name, with the property.
                let command = Command::new("SETPROPERTY").with_parameter("Property", property);
                let mut tx = self.project.transaction_for(command);
                match tx
                    .set_property(element, property, value)
                    .and_then(|()| tx.commit())
                {
                    Ok(changes) => self.committed(&changes),
                    Err(_) => self.elements.fail(self.i18n.tr("properties-edit-failed")),
                }
            }
            Edit::Delete(element) => match self.project.view().preview_delete(element) {
                Ok(deleted) if deleted.len() > 1 => self.elements.ask_to_delete(deleted),
                Ok(_) => self.delete(element),
                Err(_) => self.elements.fail(self.i18n.tr("properties-edit-failed")),
            },
        }
    }

    /// Deletes an element and what goes with it, as Revit's Delete does.
    fn delete(&mut self, element: ElementId) {
        let mut tx = self.project.transaction_for(Command::new("DELETE"));
        match tx.delete(element).and_then(|_| tx.commit()) {
            Ok(changes) => self.committed(&changes),
            Err(_) => self.elements.fail(self.i18n.tr("properties-edit-failed")),
        }
        self.elements.forget_deleted(&self.project);
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
        // Delete removes the element picked in the browser, unless the user
        // is typing.
        let delete = ui.input(|i| i.key_pressed(egui::Key::Delete))
            && !ui.ctx().egui_wants_keyboard_input()
            && !self.elements.is_asking();
        if let Some(element) = self.elements.selected().filter(|_| delete) {
            self.edit(Edit::Delete(element));
        }
        self.elements.forget_deleted(&self.project);

        let (layout, theme) = (&self.layout, self.theme);
        let state = ribbon::State {
            i18n: &self.i18n,
            commands: self.commands.registry(),
            is_open: |panel| layout.is_open(panel),
            theme,
            navigation: self.navigation,
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
            scene: &mut self.scene,
            navigation: self.navigation,
            render_state,
            open: None,
            project: &self.project,
            input: &self.input,
            elements: &mut self.elements,
            edits: Vec::new(),
        };
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                DockArea::new(&mut self.layout.dock).show_inside(ui, &mut tabs);
            });
        let (open, edits) = (tabs.open, tabs.edits);
        if let Some(view) = open {
            self.layout.open_view(view);
        }
        for edit in edits {
            self.edit(edit);
        }
        let (i18n, project) = (&self.i18n, &self.project);
        if let Some(element) = elements::confirm_delete(ui.ctx(), i18n, project, &mut self.elements)
        {
            self.delete(element);
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
        eframe::set_value(storage, NAVIGATION_KEY, &self.navigation);
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui::accesskit::Role;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use hito_core::{ClassDef, ClassId, Element, Relationship, Schema, SchemaVersion};
    use hito_structural::{class, level};

    use super::*;

    /// The window in English, with a column attached to Level 1. Columns
    /// come with #37, so this one is of a test class.
    fn window_with_a_column() -> (Harness<'static, Shell>, ElementId, ElementId) {
        let mut shell = Shell::restore(None, Locale::En);
        let mut schemas = SchemaRegistry::new();
        hito_structural::register(&mut schemas).unwrap();
        let framing = Schema::new("TestFraming", SchemaVersion::new(1, 0, 0))
            .class(ClassDef::new("Column", class(level::LEVEL_BASED_ELEMENT)));
        schemas.register(framing).unwrap();
        shell.project = project::new_project(schemas, &shell.i18n);
        let first = level::levels(&shell.project.view())[0].0;
        let mut tx = shell.project.transaction();
        let model = project::physical_model(&mut tx).unwrap();
        let column = Element::new(ClassId::new("TestFraming", "Column"), model);
        let column = tx.insert(column).unwrap();
        let attached = Relationship::new(class(level::ATTACHED_TO_LEVEL), column, first);
        tx.relate(attached).unwrap();
        tx.commit().unwrap();

        let mut h = Harness::builder()
            .with_size(egui::vec2(1280.0, 800.0))
            .build_eframe(move |_cc| shell);
        h.run_steps(2);
        (h, first, column)
    }

    #[test]
    fn deleting_a_level_asks_first_when_elements_go_with_it() {
        let (mut h, level, column) = window_with_a_column();
        h.get_by_label("Level 1").click();
        h.run_steps(2);
        h.key_press(egui::Key::Delete);
        h.run_steps(2);
        let warning = "Deleting \u{2068}Level 1\u{2069} also deletes 1 element attached to it.";
        assert!(h.query_by_label(warning).is_some(), "asks");

        h.get_by_role_and_label(Role::Button, "Cancel").click();
        h.run_steps(2);
        assert!(h.state().project.get(level).is_some(), "cancelled");

        h.key_press(egui::Key::Delete);
        h.run_steps(2);
        h.get_by_role_and_label(Role::Button, "Delete").click();
        h.run_steps(2);
        let project = &h.state().project;
        assert!(project.get(level).is_none() && project.get(column).is_none());
        assert_eq!(h.state().elements.selected(), None);
    }
}
