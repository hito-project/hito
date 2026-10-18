//! What each dock tab shows: the project browser, the properties and the
//! views.

use std::collections::HashMap;

use eframe::egui;
use eframe::egui_wgpu::RenderState;

use hito_core::ElementStore;

use super::elements::{self, Edit, Elements};
use super::layout::{Tab, ViewId};
use crate::command::InputFormat;
use crate::i18n::I18n;
use crate::viewport::{Navigation, Scene, Viewport};

/// Draws the dock tabs. The dock borrows the layout while it draws, so
/// `Tabs` gets copies of what it needs from it, and records in `open` a view
/// the user asked to open, and in `edits` the changes to the model the user
/// asked for.
pub(super) struct Tabs<'a> {
    pub i18n: &'a I18n,
    pub views: Vec<ViewId>,
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

    /// The picked element's properties, or with nothing picked the active
    /// view's, as in Revit. Editing the view's comes with #26.
    fn properties(&mut self, ui: &mut egui::Ui) {
        let (store, edits) = (self.project, &mut self.edits);
        if elements::properties(ui, self.i18n, self.input, store, self.elements, edits) {
            return;
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
                viewport.ui(ui, scene, navigation, self.render_state, self.i18n);
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
