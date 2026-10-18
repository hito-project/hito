//! What each dock tab shows: the project browser, the properties and the
//! views.

use std::collections::HashMap;

use eframe::egui;
use eframe::egui_wgpu::RenderState;
use hito_core::{ElementStore, Value};

use super::layout::{Tab, ViewId};
use crate::i18n::I18n;
use crate::selection::Selection;
use crate::viewport::{Navigation, Picked, Scene, Viewport};

/// Draws the dock tabs. The dock borrows the layout while it draws, so
/// `Tabs` gets copies of what it needs from it, and records in `open` a view
/// the user asked to open.
pub(super) struct Tabs<'a> {
    pub i18n: &'a I18n,
    pub views: Vec<ViewId>,
    pub active_view: Option<ViewId>,
    pub viewports: &'a mut HashMap<ViewId, Viewport>,
    pub scene: &'a mut Scene,
    pub navigation: Navigation,
    pub project: &'a ElementStore,
    pub selection: &'a Selection,
    /// What a click or box in a view picked this frame.
    pub picked: Option<Picked>,
    pub render_state: Option<&'a RenderState>,
    pub open: Option<ViewId>,
}

/// A view's name, as the project browser and its tab show it.
pub fn view_name(i18n: &I18n, view: ViewId) -> String {
    if view == ViewId::DEFAULT_3D {
        i18n.tr("view-default-3d")
    } else {
        i18n.tr_with("view-3d-numbered", "number", view.0)
    }
}

impl Tabs<'_> {
    /// Revit's project browser: the views, then the levels and element
    /// types. Double-clicking a view opens it. Levels come with #33 and
    /// element types with the structural elements (#36 to #41).
    fn project_browser(&mut self, ui: &mut egui::Ui) {
        let tr = |key| self.i18n.tr(key);
        egui::CollapsingHeader::new(tr("browser-views"))
            .id_salt("browser-views")
            .default_open(true)
            .show(ui, |ui| {
                egui::CollapsingHeader::new(tr("browser-3d-views"))
                    .id_salt("browser-3d-views")
                    .default_open(true)
                    .show(ui, |ui| {
                        for &view in &self.views {
                            let name = view_name(self.i18n, view);
                            let active = self.active_view == Some(view);
                            if ui.selectable_label(active, name).double_clicked() {
                                self.open = Some(view);
                            }
                        }
                    });
            });
        for key in ["browser-levels", "browser-element-types"] {
            egui::CollapsingHeader::new(tr(key))
                .id_salt(key)
                .show(ui, |ui| ui.weak(tr("browser-none")));
        }
    }

    /// Revit's properties: the selected elements', or with nothing selected
    /// the active view's. Editing them comes with #26.
    fn properties(&mut self, ui: &mut egui::Ui) {
        if !self.selection.is_empty() {
            return self.selected_properties(ui);
        }
        let Some(view) = self.active_view else {
            ui.weak(self.i18n.tr("properties-no-view"));
            return;
        };
        ui.strong(self.i18n.tr("view-kind-3d"));
        egui::Grid::new("properties")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                let label = ui.label(self.i18n.tr("properties-view-name"));
                ui.label(view_name(self.i18n, view)).labelled_by(label.id);
                ui.end_row();
            });
    }

    /// The selected elements' class and properties, as Revit shows them:
    /// one element's name, or with several their class and how many, or
    /// "Common" if they're of different classes. With several, a property
    /// they all have shows its value if it's the same in all of them and
    /// is blank if it varies.
    fn selected_properties(&mut self, ui: &mut egui::Ui) {
        let elements: Vec<_> = self
            .selection
            .ids()
            .iter()
            .filter_map(|&id| self.project.get(id))
            .collect();
        let Some(first) = elements.first() else {
            return;
        };
        let class = first.class();
        let same_class = elements.iter().all(|e| e.class() == class);
        // Class names aren't translated yet: domains will give their
        // classes labels (#26).
        let title = match (elements.len(), same_class) {
            (1, _) => class.name().to_owned(),
            (n, true) => format!("{} ({n})", class.name()),
            (n, false) => self.i18n.tr_with("properties-common", "count", n),
        };
        ui.strong(title);
        egui::Grid::new("selected-properties")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                for (name, value) in first.properties() {
                    let values: Option<Vec<_>> =
                        elements.iter().map(|e| e.properties().get(name)).collect();
                    let Some(values) = values else {
                        continue;
                    };
                    let shown = if values.iter().all(|v| *v == value) {
                        show(value)
                    } else {
                        String::new()
                    };
                    let label = ui.label(name);
                    ui.label(shown).labelled_by(label.id);
                    ui.end_row();
                }
            });
    }
}

/// A property's value as text. Lengths show in metres, as the model stores
/// them, until project units (#55) format them.
fn show(value: &Value) -> String {
    match value {
        Value::Boolean(b) => b.to_string(),
        Value::Integer(i) => i.to_string(),
        Value::Real(x) => {
            let text = format!("{x:.4}");
            text.trim_end_matches('0').trim_end_matches('.').to_owned()
        }
        Value::Text(text) => text.clone(),
    }
}

impl egui_dock::TabViewer for Tabs<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(("dock-tab", *tab))
    }

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        match tab {
            Tab::ProjectBrowser => self.i18n.tr("panel-project-browser"),
            Tab::Properties => self.i18n.tr("panel-properties"),
            Tab::View(view) => view_name(self.i18n, *view),
        }
        .into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        match *tab {
            Tab::ProjectBrowser => self.project_browser(ui),
            Tab::Properties => self.properties(ui),
            Tab::View(view) => {
                let viewport = self.viewports.entry(view).or_default();
                let (scene, navigation) = (&mut *self.scene, self.navigation);
                if let Some(picked) =
                    viewport.ui(ui, scene, navigation, self.render_state, self.i18n)
                {
                    self.picked = Some(picked);
                }
            }
        }
    }

    fn scroll_bars(&self, tab: &Tab) -> [bool; 2] {
        match tab {
            Tab::View(_) => [false, false],
            _ => [true, true],
        }
    }
}
