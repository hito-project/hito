//! The window layout: where the dockable panels are, and which views are
//! open, as tabs or tiles. It's saved when the program closes and restored
//! the next time it starts.

use egui_dock::{DockState, Node, NodeIndex, NodePath, Split, SurfaceIndex};
use serde::{Deserialize, Serialize};

/// A view. A placeholder until views are elements in the core (#43): every
/// view is a 3D view, numbered in the order it was made. Number 0 is the
/// default 3D view, `{3D}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ViewId(pub u32);

impl ViewId {
    pub const DEFAULT_3D: ViewId = ViewId(0);
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
    /// Every view, open or not, in the order they were made.
    views: Vec<ViewId>,
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
            active: Some(ViewId::DEFAULT_3D),
        }
    }
}

impl Layout {
    /// Every view, open or not, in the order they were made.
    pub fn views(&self) -> &[ViewId] {
        &self.views
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
        if !self.views.contains(&view) {
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
    /// close to square as it can.
    pub fn tile_views(&mut self) {
        let Some((anchor, others)) = self.gather_views() else {
            return;
        };
        let views: Vec<ViewId> = [anchor].into_iter().chain(others).collect();
        let columns = views.len().isqrt() + usize::from(views.len().isqrt().pow(2) < views.len());
        let rows: Vec<&[ViewId]> = views.chunks(columns).collect();

        let path = self
            .dock
            .find_tab(&Tab::View(anchor))
            .expect("the anchor view is open");
        let surface = path.surface;
        // The first tile keeps the anchor. Splitting a leaf moves it into a
        // new child node and leaves every other node where it is.
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
