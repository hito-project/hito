//! The ribbon: tabs by discipline, each with groups of buttons, as in Revit
//! ([ADR 0007](https://github.com/hito-project/hito/blob/main/docs/decisions/0007-ui-familiar-to-current-users.md)).
//!
//! Each button runs a [command](crate::command), the same one its name or
//! alias runs from the command line. Commands that aren't available yet
//! show disabled.

use eframe::egui;

use super::layout::Tab;
use super::theme::Theme;
use crate::command::{Registry, Run};
use crate::i18n::I18n;
use crate::viewport::Navigation;

/// Something a ribbon button does to the window itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    New3dView,
    CloseInactiveViews,
    TabViews,
    TileViews,
    TogglePanel(Tab),
    SetTheme(Theme),
    SetNavigation(Navigation),
    /// Zooms the active view to fit the model.
    ZoomToFit,
}

/// What the user pressed in the ribbon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pressed {
    /// A command's button, by the command's name.
    Command(&'static str),
    /// A setting that isn't a command, such as the theme.
    Action(Action),
}

#[derive(Debug, Clone, Copy)]
enum Item {
    /// A command's button, by the command's name.
    Command(&'static str),
    /// Revit's User Interface menu: turns the panels on and off, and picks
    /// the theme and how the mouse wheel moves 3D views.
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
                    Item::Command("BEAM"),
                    Item::Command("WALL"),
                    Item::Command("COLUMN"),
                    Item::Command("SLAB"),
                ],
            },
            Group {
                title: "ribbon-group-opening",
                items: &[Item::Command("VERTICALOPENING")],
            },
            Group {
                title: "ribbon-group-foundation",
                items: &[Item::Command("ISOLATEDFOOTING")],
            },
            Group {
                title: "ribbon-group-datum",
                items: &[Item::Command("LEVEL"), Item::Command("GRID")],
            },
        ],
    },
    // Revit's Architecture tab, after Structure so the M1 structural
    // commands still show first.
    RibbonTab {
        title: "ribbon-tab-architecture",
        groups: &[Group {
            title: "ribbon-group-build",
            items: &[
                Item::Command("WALL"),
                Item::Command("DOOR"),
                Item::Command("WINDOW"),
            ],
        }],
    },
    RibbonTab {
        title: "ribbon-tab-view",
        groups: &[
            Group {
                title: "ribbon-group-create",
                items: &[Item::Command("VIEW3D")],
            },
            Group {
                title: "ribbon-group-windows",
                items: &[
                    Item::Command("CLOSEINACTIVE"),
                    Item::Command("TABVIEWS"),
                    Item::Command("TILEVIEWS"),
                    Item::UserInterface,
                ],
            },
        ],
    },
    RibbonTab {
        title: "ribbon-tab-manage",
        groups: &[Group {
            title: "ribbon-group-settings",
            items: &[Item::Command("MATERIALS"), Item::Command("UNITS")],
        }],
    },
];

/// The ribbon's state: which tab is showing.
#[derive(Debug, Default)]
pub struct Ribbon {
    selected: usize,
}

const BUTTON_SIZE: egui::Vec2 = egui::vec2(64.0, 48.0);

/// What the ribbon needs to know about the window to draw itself.
pub(super) struct State<'a, F> {
    pub i18n: &'a I18n,
    pub commands: &'a Registry,
    /// Which panels are open, for the User Interface menu.
    pub is_open: F,
    pub theme: Theme,
    pub navigation: Navigation,
}

impl Ribbon {
    /// Shows the ribbon. Returns what the user pressed.
    pub(super) fn ui<F: Fn(Tab) -> bool>(
        &mut self,
        ui: &mut egui::Ui,
        state: &State<'_, F>,
    ) -> Option<Pressed> {
        ui.horizontal(|ui| {
            for (index, tab) in TABS.iter().enumerate() {
                ui.selectable_value(&mut self.selected, index, state.i18n.tr(tab.title));
            }
        });
        let mut pressed = None;
        ui.horizontal(|ui| {
            for group in TABS[self.selected].groups {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            for item in group.items {
                                if let Some(p) = item_ui(ui, item, state) {
                                    pressed = Some(p);
                                }
                            }
                        });
                        let title = state.i18n.tr(group.title);
                        ui.label(egui::RichText::new(title).small().weak());
                    });
                });
            }
        });
        pressed
    }
}

fn item_ui<F: Fn(Tab) -> bool>(
    ui: &mut egui::Ui,
    item: &Item,
    state: &State<'_, F>,
) -> Option<Pressed> {
    let i18n = state.i18n;
    match *item {
        Item::Command(name) => {
            let command = state
                .commands
                .get(name)
                .expect("the ribbon's commands are registered");
            let label = i18n.tr(command.label);
            // Revit's tooltips name the shortcut, as in "Column (CL)".
            let tooltip = match command.alias {
                Some(alias) => i18n.tr_args(
                    "command-tooltip",
                    [("command", label.clone().into()), ("alias", alias.into())],
                ),
                None => label.clone(),
            };
            let button = egui::Button::new(label).min_size(BUTTON_SIZE);
            if matches!(command.run, Run::Unavailable) {
                ui.add_enabled(false, button)
                    .on_disabled_hover_text(i18n.tr("command-unavailable"));
                return None;
            }
            ui.add(button)
                .on_hover_text(tooltip)
                .clicked()
                .then_some(Pressed::Command(command.name))
        }
        Item::UserInterface => {
            let mut pressed = None;
            ui.menu_button(i18n.tr("action-user-interface"), |ui| {
                for (panel, name) in [
                    (Tab::ProjectBrowser, "PROJECTBROWSER"),
                    (Tab::Properties, "PROPERTIES"),
                ] {
                    let label = state.commands.get(name).map(|c| c.label).unwrap_or(name);
                    let mut open = (state.is_open)(panel);
                    if ui.checkbox(&mut open, i18n.tr(label)).changed() {
                        pressed = Some(Pressed::Command(name));
                    }
                }
                ui.separator();
                ui.label(egui::RichText::new(i18n.tr("ui-theme")).small().weak());
                for choice in Theme::ALL {
                    if ui
                        .radio(state.theme == choice, i18n.tr(choice.label()))
                        .clicked()
                    {
                        pressed = Some(Pressed::Action(Action::SetTheme(choice)));
                    }
                }
                ui.separator();
                ui.label(egui::RichText::new(i18n.tr("ui-navigation")).small().weak());
                for choice in Navigation::ALL {
                    if ui
                        .radio(state.navigation == choice, i18n.tr(choice.label()))
                        .clicked()
                    {
                        pressed = Some(Pressed::Action(Action::SetNavigation(choice)));
                    }
                }
            });
            pressed
        }
    }
}
