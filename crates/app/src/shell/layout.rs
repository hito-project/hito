//! The window layout: where the dockable panels are, and which views are
//! open, as tabs or tiles. It's saved when the program closes and restored
//! the next time it starts.

use egui_dock::{DockState, Node, NodeIndex, NodePath, Split, SurfaceIndex};
use hito_core::ElementId;
use serde::{Deserialize, Serialize};

/// A view. A placeholder until views are elements in the core (#43): a 3D
/// view, numbered in the order it was made, or a level's plan view (#183),
/// numbered from [`ViewId::FIRST_PLAN`]. Number 0 is the default 3D view,
/// `{3D}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ViewId(pub u32);

impl ViewId {
    pub const DEFAULT_3D: ViewId = ViewId(0);
    /// The first plan view's number. Plan views are numbered apart from 3D
    /// views, so 3D views keep their numbers, and their names, whatever
    /// levels there are.
    pub const FIRST_PLAN: u32 = 1 << 20;

    pub fn is_plan(self) -> bool {
        self.0 >= Self::FIRST_PLAN
    }
}

/// What a dock tab shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tab {
    ProjectBrowser,
    Properties,
    View(ViewId),
}

/// The panels, laid out as Revit lays them out by default: the properties
/// above the project browser on the left, and the views in the rest of the
/// window ([ADR 0007](https://github.com/hito-project/hito/blob/main/docs/decisions/0007-ui-familiar-to-current-users.md)).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layout {
    pub(super) dock: DockState<Tab>,
    /// Every 3D view, open or not, in the order they were made.
    views: Vec<ViewId>,
    /// Every level's plan view, in the order they were made. Levels belong
    /// to the project, which isn't saved yet (#46), so plan views aren't
    /// saved with the layout: they're made again for the levels there are.
    #[serde(skip)]
    plans: Vec<(ViewId, ElementId)>,
    /// The view the user last worked in.
    active: Option<ViewId>,
}

/// The share of the window's width the panels on the left take.
const PANEL_WIDTH: f32 = 0.2;

impl Default for Layout {
    fn default() -> Self {
        let mut dock = DockState::new(vec![Tab::View(ViewId::DEFAULT_3D)]);
        let surface = dock.main_surface_mut();
        let [_views, left] =
            surface.split_left(NodeIndex::root(), PANEL_WIDTH, vec![Tab::Properties]);
        surface.split_below(left, 0.5, vec![Tab::ProjectBrowser]);
        Self {
            dock,
            views: vec![ViewId::DEFAULT_3D],
            plans: Vec::new(),
            active: Some(ViewId::DEFAULT_3D),
        }
    }
}

impl Layout {
    /// Every 3D view, open or not, in the order they were made.
    pub fn views(&self) -> &[ViewId] {
        &self.views
    }

    /// Every plan view and its level, in the order they were made.
    pub fn plans(&self) -> &[(ViewId, ElementId)] {
        &self.plans
    }

    /// The level a plan view shows, `None` for a 3D view.
    pub fn plan_level(&self, view: ViewId) -> Option<ElementId> {
        self.plans
            .iter()
            .find(|(v, _)| *v == view)
            .map(|(_, level)| *level)
    }

    /// The plan view of a level, if it has one.
    pub fn plan_of(&self, level: ElementId) -> Option<ViewId> {
        self.plans
            .iter()
            .find(|(_, l)| *l == level)
            .map(|(view, _)| *view)
    }

    /// Gives each of `levels` a plan view, as Revit makes one for each new
    /// level, and deletes the plan views of levels that are gone, closing
    /// their tabs.
    pub fn sync_plans(&mut self, levels: &[ElementId]) {
        let gone: Vec<ViewId> = self
            .plans
            .iter()
            .filter(|(_, level)| !levels.contains(level))
            .map(|(view, _)| *view)
            .collect();
        for view in gone {
            self.close(Tab::View(view));
            self.plans.retain(|(v, _)| *v != view);
        }
        for &level in levels {
            if self.plan_of(level).is_none() {
                let next = self.plans.iter().map(|(v, _)| v.0 + 1).max();
                let view = ViewId(next.unwrap_or(ViewId::FIRST_PLAN));
                self.plans.push((view, level));
            }
        }
    }

    /// Whether the view exists, open or not.
    fn exists(&self, view: ViewId) -> bool {
        self.views.contains(&view) || self.plan_level(view).is_some()
    }

    /// The open views, in the order their tabs are laid out.
    pub fn open_views(&self) -> Vec<ViewId> {
        self.dock
            .iter_all_tabs()
            .filter_map(|(_, tab)| match tab {
                Tab::View(view) => Some(*view),
                _ => None,
            })
            .collect()
    }

    pub fn is_open(&self, tab: Tab) -> bool {
        self.dock.find_tab(&tab).is_some()
    }

    /// The view the user last worked in, if it's still open, otherwise the
    /// first open view.
    pub fn active_view(&self) -> Option<ViewId> {
        let open = self.open_views();
        self.active
            .filter(|view| open.contains(view))
            .or_else(|| open.first().copied())
    }

    /// Notes which view has the focus, if a view has it.
    pub(super) fn track_focus(&mut self) {
        if let Some((_, Tab::View(view))) = self.dock.find_active_focused() {
            self.active = Some(*view);
        }
    }

    /// The number of separate areas (dock leaves) views are shown in: one
    /// when they're tabbed, one per view when they're tiled.
    pub fn view_areas(&self) -> usize {
        self.dock
            .iter_leaves()
            .filter(|(_, leaf)| leaf.tabs().iter().any(|tab| matches!(tab, Tab::View(_))))
            .count()
    }

    /// Makes a new 3D view and opens it. A placeholder for the view commands
    /// of #24 and #43.
    pub fn new_3d_view(&mut self) -> ViewId {
        let view = ViewId(self.views.iter().map(|v| v.0 + 1).max().unwrap_or(0));
        self.views.push(view);
        self.open_view(view);
        view
    }

    /// Opens a view next to the other open views, or brings it to the front
    /// if it's already open.
    pub fn open_view(&mut self, view: ViewId) {
        if !self.exists(view) {
            return;
        }
        let tab = Tab::View(view);
        if self.dock.find_tab(&tab).is_none() {
            let beside = self
                .active_view()
                .and_then(|v| self.dock.find_tab(&Tab::View(v)));
            match beside {
                Some(path) => self.dock[path.surface][path.node].append_tab(tab),
                None => self.add_beside_panels(tab),
            }
        }
        self.focus(tab);
        self.active = Some(view);
    }

    /// Closes every view but the active one, like Revit's Close Inactive.
    pub fn close_inactive_views(&mut self) {
        let active = self.active_view();
        for view in self.open_views() {
            if Some(view) != active {
                self.close(Tab::View(view));
            }
        }
    }

    /// Puts every open view in one area, as tabs.
    pub fn tab_views(&mut self) {
        let Some((anchor, others)) = self.gather_views() else {
            return;
        };
        let path = self
            .dock
            .find_tab(&Tab::View(anchor))
            .expect("the anchor view is open");
        for view in others {
            self.dock[path.surface][path.node].append_tab(Tab::View(view));
        }
        if let Some(active) = self.active_view() {
            self.focus(Tab::View(active));
        }
    }

    /// Splits the views' area into a grid with one view in each tile, as
    /// close to square as it can, in the order the views were made. The
    /// active view stays active.
    pub fn tile_views(&mut self) {
        let Some((anchor, others)) = self.gather_views() else {
            return;
        };
        let mut views: Vec<ViewId> = [anchor].into_iter().chain(others).collect();
        views.sort();
        let columns = views.len().isqrt() + usize::from(views.len().isqrt().pow(2) < views.len());
        let rows: Vec<&[ViewId]> = views.chunks(columns).collect();

        // The first tile is the anchor's area, now showing the first view.
        let path = self
            .dock
            .find_tab(&Tab::View(anchor))
            .expect("the anchor view is open");
        let leaf = self
            .dock
            .leaf_mut(path.node_path())
            .expect("a tab is in a leaf");
        leaf.tabs_mut()[path.tab.0] = Tab::View(views[0]);
        let surface = path.surface;
        // Splitting a leaf moves it into a new child node and leaves every
        // other node where it is.
        let mut row_nodes = Vec::with_capacity(rows.len());
        let mut rest = path.node;
        for (i, row) in rows.iter().enumerate().skip(1) {
            let share = 1.0 / (rows.len() - i + 1) as f32;
            let [above, below] = self.split(surface, rest, Split::Below, share, row[0]);
            row_nodes.push(above);
            rest = below;
        }
        row_nodes.push(rest);
        for (node, row) in row_nodes.into_iter().zip(&rows) {
            let mut rest = node;
            for (j, &view) in row.iter().enumerate().skip(1) {
                let share = 1.0 / (row.len() - j + 1) as f32;
                rest = self.split(surface, rest, Split::Right, share, view)[1];
            }
        }
        // Splitting focuses the new tile; give the focus back.
        self.focus(Tab::View(anchor));
    }

    /// Opens a closed panel, or closes an open one. Panels reopen where
    /// Revit puts them: the properties above the project browser.
    pub fn toggle_panel(&mut self, panel: Tab) {
        if self.is_open(panel) {
            self.close(panel);
            return;
        }
        let (sibling, split, share) = match panel {
            Tab::Properties => (Tab::ProjectBrowser, Split::Above, 0.5),
            Tab::ProjectBrowser => (Tab::Properties, Split::Below, 0.5),
            Tab::View(view) => return self.open_view(view),
        };
        match self.dock.find_tab(&sibling) {
            Some(path) => {
                self.dock
                    .split(path.node_path(), split, share, Node::leaf(panel));
            }
            None => self.add_beside_panels(panel),
        }
    }

    /// Drops what a saved layout can't show: tabs of views that don't exist
    /// any more, and a second tab of the same view or panel.
    pub(super) fn repair(mut self) -> Self {
        if !self.views.contains(&ViewId::DEFAULT_3D) {
            self.views.insert(0, ViewId::DEFAULT_3D);
        }
        let views = self.views.clone();
        let mut seen = Vec::new();
        self.dock.retain_tabs(|tab| {
            let keep = !seen.contains(tab)
                && match tab {
                    Tab::View(view) => views.contains(view),
                    _ => true,
                };
            seen.push(*tab);
            keep
        });
        self
    }

    fn close(&mut self, tab: Tab) {
        if let Some(path) = self.dock.find_tab(&tab) {
            self.dock.remove_tab(path);
        }
    }

    fn focus(&mut self, tab: Tab) {
        if let Some(path) = self.dock.find_tab(&tab) {
            let _ = self.dock.set_active_tab(path);
            self.dock.set_focused_node_and_surface(path.node_path());
        }
    }

    /// Adds a tab in a new area to the right of everything, or as the only
    /// tab if the window is empty.
    fn add_beside_panels(&mut self, tab: Tab) {
        let surface = self.dock.main_surface_mut();
        if surface.is_empty() {
            surface.push_to_first_leaf(tab);
        } else {
            surface.split_right(NodeIndex::root(), PANEL_WIDTH, vec![tab]);
        }
    }

    fn split(
        &mut self,
        surface: SurfaceIndex,
        node: NodeIndex,
        split: Split,
        share: f32,
        view: ViewId,
    ) -> [NodeIndex; 2] {
        let path = NodePath { surface, node };
        self.dock
            .split(path, split, share, Node::leaf(Tab::View(view)))
    }

    /// Leaves one open view (the active one, if it's open) where it is and
    /// closes the others, returning both, so they can be put back together
    /// around it. `None` if no view is open.
    fn gather_views(&mut self) -> Option<(ViewId, Vec<ViewId>)> {
        let open = self.open_views();
        let anchor = self.active_view()?;
        let others: Vec<ViewId> = open.into_iter().filter(|&v| v != anchor).collect();
        for &view in &others {
            self.close(Tab::View(view));
        }
        Some((anchor, others))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_views(n: usize) -> Layout {
        let mut layout = Layout::default();
        for _ in 1..n {
            layout.new_3d_view();
        }
        layout
    }

    #[test]
    fn revit_default_layout() {
        let layout = Layout::default();
        assert!(layout.is_open(Tab::Properties));
        assert!(layout.is_open(Tab::ProjectBrowser));
        assert_eq!(layout.open_views(), [ViewId::DEFAULT_3D]);
        assert_eq!(layout.active_view(), Some(ViewId::DEFAULT_3D));
    }

    #[test]
    fn new_views_open_as_tabs_beside_the_active_one() {
        let layout = with_views(3);
        assert_eq!(layout.open_views().len(), 3);
        assert_eq!(layout.view_areas(), 1);
        assert_eq!(layout.active_view(), Some(ViewId(2)));
    }

    #[test]
    fn tile_then_tab() {
        for n in 1..=7 {
            let mut layout = with_views(n);
            layout.tile_views();
            assert_eq!(layout.view_areas(), n, "{n} views tiled");
            assert_eq!(layout.open_views().len(), n);
            assert!(layout.is_open(Tab::Properties) && layout.is_open(Tab::ProjectBrowser));
            layout.tab_views();
            assert_eq!(layout.view_areas(), 1, "{n} views tabbed");
            assert_eq!(layout.open_views().len(), n);
        }
    }

    #[test]
    fn close_inactive_views_keeps_the_active_one() {
        let mut layout = with_views(3);
        layout.open_view(ViewId(1));
        layout.close_inactive_views();
        assert_eq!(layout.open_views(), [ViewId(1)]);
        assert_eq!(layout.views().len(), 3, "closing a view doesn't delete it");
    }

    #[test]
    fn panels_close_and_reopen() {
        let mut layout = Layout::default();
        layout.toggle_panel(Tab::Properties);
        assert!(!layout.is_open(Tab::Properties));
        layout.toggle_panel(Tab::ProjectBrowser);
        assert!(!layout.is_open(Tab::ProjectBrowser));
        layout.toggle_panel(Tab::ProjectBrowser);
        layout.toggle_panel(Tab::Properties);
        assert!(layout.is_open(Tab::Properties) && layout.is_open(Tab::ProjectBrowser));
        assert_eq!(layout.view_areas(), 1);
    }

    #[test]
    fn a_view_opens_in_an_empty_window() {
        let mut layout = Layout::default();
        layout.toggle_panel(Tab::Properties);
        layout.toggle_panel(Tab::ProjectBrowser);
        layout.close(Tab::View(ViewId::DEFAULT_3D));
        assert!(layout.dock.main_surface().is_empty());
        layout.open_view(ViewId::DEFAULT_3D);
        assert_eq!(layout.open_views(), [ViewId::DEFAULT_3D]);
        layout.toggle_panel(Tab::Properties);
        assert!(layout.is_open(Tab::Properties));
    }

    #[test]
    fn each_level_gets_a_plan_view_until_it_is_deleted() {
        let mut layout = Layout::default();
        let [a, b] = [(); 2].map(|_| ElementId::generate());
        layout.sync_plans(&[a, b]);
        let plan_a = layout.plan_of(a).unwrap();
        let plan_b = layout.plan_of(b).unwrap();
        assert!(plan_a.is_plan() && plan_b.is_plan() && plan_a != plan_b);
        assert_eq!(layout.plan_level(plan_b), Some(b));
        layout.sync_plans(&[a, b]);
        assert_eq!(layout.plans().len(), 2, "made once");

        layout.open_view(plan_b);
        assert_eq!(layout.active_view(), Some(plan_b));
        // 3D views keep their numbers.
        assert_eq!(layout.new_3d_view(), ViewId(1));
        assert_eq!(layout.views(), [ViewId::DEFAULT_3D, ViewId(1)]);

        layout.sync_plans(&[a]);
        assert_eq!(layout.plan_of(b), None);
        assert!(!layout.open_views().contains(&plan_b), "its tab closed");
        layout.open_view(plan_b);
        assert!(!layout.open_views().contains(&plan_b), "it's gone");
    }

    #[test]
    fn plan_views_are_not_restored_with_the_layout() {
        let mut layout = Layout::default();
        let level = ElementId::generate();
        layout.sync_plans(&[level]);
        layout.open_view(layout.plan_of(level).unwrap());
        #[derive(Default)]
        struct Memory(Option<String>);
        impl eframe::Storage for Memory {
            fn get_string(&self, _: &str) -> Option<String> {
                self.0.clone()
            }
            fn set_string(&mut self, _: &str, value: String) {
                self.0 = Some(value);
            }
            fn remove_string(&mut self, _: &str) {
                self.0 = None;
            }
            fn flush(&mut self) {}
        }
        let mut saved = Memory::default();
        eframe::set_value(&mut saved, "layout", &layout);
        let restored = eframe::get_value::<Layout>(&saved, "layout")
            .unwrap()
            .repair();
        assert!(restored.plans().is_empty());
        assert_eq!(restored.open_views(), [ViewId::DEFAULT_3D]);
    }

    #[test]
    fn repair_drops_unknown_and_repeated_tabs() {
        let mut layout = Layout::default();
        layout.views.clear();
        layout.dock.push_to_first_leaf(Tab::View(ViewId(9)));
        layout.dock.push_to_first_leaf(Tab::Properties);
        let layout = layout.repair();
        assert_eq!(layout.views(), [ViewId::DEFAULT_3D]);
        assert_eq!(layout.open_views(), [ViewId::DEFAULT_3D]);
        let properties = layout
            .dock
            .iter_all_tabs()
            .filter(|(_, t)| **t == Tab::Properties);
        assert_eq!(properties.count(), 1);
    }
}
