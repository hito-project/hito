//! The ribbon: tabs by discipline, each with groups of buttons, as in Revit
//! ([ADR 0007](https://github.com/hito-project/hito/blob/main/docs/decisions/0007-ui-familiar-to-current-users.md)).
//!
//! Each button either runs a window [`Action`] or stands for a command that
//! doesn't exist yet. Commands come with #24; their buttons show disabled
//! until then.

use eframe::egui;

use super::layout::Tab;
use crate::i18n::I18n;

/// Something a ribbon button does to the window itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    New3dView,
    CloseInactiveViews,
    TabViews,
    TileViews,
    TogglePanel(Tab),
}

#[derive(Debug, Clone, Copy)]
enum Item {
    Action(Action, &'static str),
    /// A command from a later story, by its message key.
    Command(&'static str),
    /// Revit's User Interface menu: turns the panels on and off.
    UserInterface,
}

struct Group {
    title: &'static str,
    items: &'static [Item],
}

struct RibbonTab {
    title: &'static str,
    groups: &'static [Group],
}

/// The ribbon's tabs and groups, with message keys for their text. Only the
/// M1 commands (#33 to #41, #36, #163) are listed.
const TABS: &[RibbonTab] = &[
    RibbonTab {
        title: "ribbon-tab-structure",
        groups: &[
            Group {
                title: "ribbon-group-structure",
                items: &[
                    Item::Command("command-beam"),
                    Item::Command("command-wall"),
                    Item::Command("command-column"),
                    Item::Command("command-slab"),
                ],
            },
            Group {
                title: "ribbon-group-foundation",
                items: &[Item::Command("command-isolated-footing")],
            },
            Group {
                title: "ribbon-group-datum",
                items: &[
                    Item::Command("command-level"),
                    Item::Command("command-grid"),
                ],
            },
        ],
    },
    RibbonTab {
        title: "ribbon-tab-view",
        groups: &[
            Group {
                title: "ribbon-group-create",
                items: &[Item::Action(Action::New3dView, "action-new-3d-view")],
            },
            Group {
                title: "ribbon-group-windows",
                items: &[
                    Item::Action(Action::CloseInactiveViews, "action-close-inactive-views"),
                    Item::Action(Action::TabViews, "action-tab-views"),
                    Item::Action(Action::TileViews, "action-tile-views"),
                    Item::UserInterface,
                ],
            },
        ],
    },
    RibbonTab {
        title: "ribbon-tab-manage",
        groups: &[Group {
            title: "ribbon-group-settings",
            items: &[
                Item::Command("command-materials"),
                Item::Command("command-project-units"),
            ],
        }],
    },
];

/// The ribbon's state: which tab is showing.
#[derive(Debug, Default)]
pub struct Ribbon {
    selected: usize,
}

const BUTTON_SIZE: egui::Vec2 = egui::vec2(64.0, 48.0);

impl Ribbon {
    /// Shows the ribbon. Returns the action of the button the user pressed.
    /// `is_open` says which panels are open, for the User Interface menu.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        i18n: &I18n,
        is_open: impl Fn(Tab) -> bool,
    ) -> Option<Action> {
        ui.horizontal(|ui| {
            for (index, tab) in TABS.iter().enumerate() {
                ui.selectable_value(&mut self.selected, index, i18n.tr(tab.title));
            }
        });
        let mut action = None;
        ui.horizontal(|ui| {
            for group in TABS[self.selected].groups {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            for item in group.items {
                                if let Some(a) = item_ui(ui, item, i18n, &is_open) {
                                    action = Some(a);
                                }
                            }
                        });
                        ui.label(egui::RichText::new(i18n.tr(group.title)).small().weak());
                    });
                });
            }
        });
        action
    }
}

fn item_ui(
    ui: &mut egui::Ui,
    item: &Item,
    i18n: &I18n,
    is_open: &impl Fn(Tab) -> bool,
) -> Option<Action> {
    match *item {
        Item::Action(action, label) => {
            let button = egui::Button::new(i18n.tr(label)).min_size(BUTTON_SIZE);
            ui.add(button).clicked().then_some(action)
        }
        Item::Command(label) => {
            let button = egui::Button::new(i18n.tr(label)).min_size(BUTTON_SIZE);
            ui.add_enabled(false, button)
                .on_disabled_hover_text(i18n.tr("command-unavailable"));
            None
        }
        Item::UserInterface => {
            let mut action = None;
            ui.menu_button(i18n.tr("action-user-interface"), |ui| {
                for (panel, label) in [
                    (Tab::ProjectBrowser, "panel-project-browser"),
                    (Tab::Properties, "panel-properties"),
                ] {
                    let mut open = is_open(panel);
                    if ui.checkbox(&mut open, i18n.tr(label)).changed() {
                        action = Some(Action::TogglePanel(panel));
                    }
                }
            });
            action
        }
    }
}
