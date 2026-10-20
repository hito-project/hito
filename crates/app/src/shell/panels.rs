//! What each dock tab shows: the project browser, the properties and the
//! views.

use std::collections::HashMap;

use eframe::egui;
use eframe::egui_wgpu::RenderState;

use hito_core::{ElementId, ElementStore, Value};

use super::elements::{self, Edit, Elements};
use super::layout::{Tab, ViewId};
use super::plans;
use crate::command::{InputFormat, Wants};
use crate::i18n::I18n;
use crate::viewport::{Navigation, Picked, Scene, Viewport};

/// Draws the dock tabs. The dock borrows the layout while it draws, so
/// `Tabs` gets copies of what it needs from it, and records in `open` a view
/// the user asked to open, and in `edits` the changes to the model the user
/// asked for.
pub(super) struct Tabs<'a> {
    pub i18n: &'a I18n,
    pub views: Vec<ViewId>,
    /// Every plan view and its level.
    pub plans: Vec<(ViewId, ElementId)>,
    pub active_view: Option<ViewId>,
    pub viewports: &'a mut HashMap<ViewId, Viewport>,
    pub scene: &'a mut Scene,
    pub navigation: Navigation,
    pub render_state: Option<&'a RenderState>,
    pub open: Option<ViewId>,
    pub project: &'a ElementStore,
    pub input: &'a InputFormat,
    pub elements: &'a mut Elements,
    pub edits: Vec<Edit>,
    /// What a click or a box in a view picked this frame.
    pub picked: Option<Picked>,
    /// What the running command asks for, so a plan view can take a click
    /// as a point.
    pub wants: Option<Wants>,
    /// A point clicked in a plan view this frame, and the plan's level.
    pub point: Option<(ElementId, [f64; 3])>,
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
    /// A view's name. A plan view's is its level's, as in Revit.
    fn name_of(&self, view: ViewId) -> String {
        match self.plan_level(view) {
            Some(level) => plans::name(self.project, level),
            None => view_name(self.i18n, view),
        }
    }

    fn plan_level(&self, view: ViewId) -> Option<ElementId> {
        let plan = self.plans.iter().find(|(v, _)| *v == view);
        plan.map(|(_, level)| *level)
    }

    /// A plan view (#183). A click in it answers the running command's
    /// prompt with a point on its level.
    fn plan(&mut self, ui: &mut egui::Ui, view: ViewId) {
        let Some(level) = self.plan_level(view) else {
            return;
        };
        let viewport = self.viewports.entry(view).or_insert_with(Viewport::plan);
        let plan = plans::content(self.project, level);
        let output = viewport.plan_ui(ui, self.scene, &plan, self.wants);
        if let Some(point) = output.point {
            self.point = Some((level, point));
        }
        if let Some(picked) = output.picked {
            self.picked = Some(picked);
        }
    }

    /// Revit's project browser: the views, then the levels,
    /// grids and element types. Double-clicking a view opens it; clicking a
    /// level or a grid picks it.
    /// Element types come with the structural elements (#36 to #41).
    fn project_browser(&mut self, ui: &mut egui::Ui) {
        let tr = |key| self.i18n.tr(key);
        egui::CollapsingHeader::new(tr("browser-views"))
            .id_salt("browser-views")
            .default_open(true)
            .show(ui, |ui| {
                // Closed at first: a plan is named after its level, which
                // the levels below list too, until elevations (#197) show
                // the levels instead.
                egui::CollapsingHeader::new(tr("browser-floor-plans"))
                    .id_salt("browser-floor-plans")
                    .show(ui, |ui| {
                        for &(view, level) in &self.plans {
                            let name = plans::name(self.project, level);
                            let active = self.active_view == Some(view);
                            if ui.selectable_label(active, name).double_clicked() {
                                self.open = Some(view);
                            }
                        }
                    });
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
        egui::CollapsingHeader::new(tr("browser-levels"))
            .id_salt("browser-levels")
            .default_open(true)
            .show(ui, |ui| {
                let (store, edits) = (self.project, &mut self.edits);
                elements::browser_levels(ui, self.i18n, store, self.elements, edits);
            });
        egui::CollapsingHeader::new(tr("browser-grids"))
            .id_salt("browser-grids")
            .default_open(true)
            .show(ui, |ui| {
                let (store, edits) = (self.project, &mut self.edits);
                elements::browser_grids(ui, self.i18n, store, self.elements, edits);
            });
        egui::CollapsingHeader::new(tr("browser-element-types"))
            .id_salt("browser-element-types")
            .show(ui, |ui| ui.weak(tr("browser-none")));
    }

    /// The selection's properties, editable for a single level or grid, or
    /// with nothing selected the active view's, as in Revit. Editing the
    /// others comes with #26.
    fn properties(&mut self, ui: &mut egui::Ui) {
        let (store, edits) = (self.project, &mut self.edits);
        if elements::properties(ui, self.i18n, self.input, store, self.elements, edits) {
            return;
        }
        if !self.elements.selection().is_empty() {
            return self.selected_properties(ui);
        }
        let Some(view) = self.active_view else {
            ui.weak(self.i18n.tr("properties-no-view"));
            return;
        };
        let kind = match self.plan_level(view) {
            Some(_) => "view-kind-floor-plan",
            None => "view-kind-3d",
        };
        ui.strong(self.i18n.tr(kind));
        egui::Grid::new("properties")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                let label = ui.label(self.i18n.tr("properties-view-name"));
                ui.label(self.name_of(view)).labelled_by(label.id);
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
            .elements
            .selection()
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
            Tab::View(view) => self.name_of(*view),
        }
        .into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        match *tab {
            Tab::ProjectBrowser => self.project_browser(ui),
            Tab::Properties => self.properties(ui),
            Tab::View(view) if self.plan_level(view).is_some() => self.plan(ui, view),
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
