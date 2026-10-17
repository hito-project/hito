//! What each dock tab shows: the project browser, the properties and the
//! views.

use std::collections::HashMap;

use eframe::egui;
use eframe::egui_wgpu::RenderState;

use super::layout::{Tab, ViewId};
use crate::i18n::I18n;
use crate::viewport::Viewport;

/// Draws the dock tabs. The dock borrows the layout while it draws, so
/// `Tabs` gets copies of what it needs from it, and records in `open` a view
/// the user asked to open.
pub(super) struct Tabs<'a> {
    pub i18n: &'a I18n,
    pub views: Vec<ViewId>,
    pub active_view: Option<ViewId>,
    pub viewports: &'a mut HashMap<ViewId, Viewport>,
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

    /// With nothing selected, Revit's properties show the active view's.
    /// Editing them comes with #26.
    fn properties(&mut self, ui: &mut egui::Ui) {
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
                let no_gpu = self.i18n.tr("viewport-no-gpu");
                let viewport = self.viewports.entry(view).or_default();
                viewport.ui(ui, self.render_state, &no_gpu);
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
