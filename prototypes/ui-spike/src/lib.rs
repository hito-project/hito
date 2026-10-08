//! Throwaway prototype for spike #4: the riskiest UI needs from ADR 0007 in egui.

pub mod commands;
pub mod i18n;
pub mod text;
pub mod viewport;

use commands::CommandId;
use egui_dock::{DockArea, DockState, NodeIndex};
use i18n::{I18n, Lang};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Browser,
    Properties,
    Viewport,
    Elements,
    Text,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RibbonTab {
    Structure,
    Annotate,
    View,
}

pub struct Props {
    pub type_name: usize,
    pub base_level: usize,
    pub top_level: usize,
    pub width: f64,
    pub depth: f64,
    pub height: f64,
    pub material: usize,
    pub cover: f64,
}

const TYPES: &[&str] = &["C-30x30", "C-40x40", "C-30x60"];
const LEVELS: &[&str] = &["PB", "1P", "2P", "Azotea"];
const MATERIALS: &[&str] = &["H-25", "H-30", "H-35"];

pub struct HitoApp {
    pub i18n: I18n,
    ribbon_tab: RibbonTab,
    dock: DockState<Panel>,
    pub props: Props,
    pub viewport: viewport::Viewport,
    pub cmdline: String,
    completion_sel: usize,
    /// Pending letters of a two-letter shortcut.
    shortcut_buf: String,
    /// Every command that ran, in order. Tests read this.
    pub log: Vec<CommandId>,
    pub status: String,
    fonts: Vec<&'static str>,
}

impl HitoApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let fonts = text::install_fallback_fonts(&cc.egui_ctx);
        let mut dock = DockState::new(vec![Panel::Viewport, Panel::Elements, Panel::Text]);
        let surface = dock.main_surface_mut();
        let [center, _right] = surface.split_right(NodeIndex::root(), 0.75, vec![Panel::Properties]);
        surface.split_left(center, 0.22, vec![Panel::Browser]);
        Self {
            i18n: I18n::new(Lang::EsAr),
            ribbon_tab: RibbonTab::Structure,
            dock,
            props: Props {
                type_name: 0,
                base_level: 0,
                top_level: 1,
                width: 0.30,
                depth: 0.30,
                height: 3.00,
                material: 1,
                cover: 0.025,
            },
            viewport: Default::default(),
            cmdline: String::new(),
            completion_sel: 0,
            shortcut_buf: String::new(),
            log: vec![],
            status: String::new(),
            fonts,
        }
    }

    pub fn run(&mut self, id: CommandId) {
        self.log.push(id);
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("name", self.i18n.tr(commands::get(id).key));
        self.status = self.i18n.tr_args("cmdline-ran", Some(&args));
    }

    fn ribbon_button(&mut self, ui: &mut egui::Ui, id: CommandId) {
        let c = commands::get(id);
        let label = self.i18n.tr(c.key);
        let btn = egui::Button::new(egui::RichText::new(&label).size(13.0)).min_size(egui::vec2(64.0, 52.0));
        let resp = ui.add(btn).on_hover_text(format!("{label} ({})", c.shortcut));
        if resp.clicked() {
            self.run(id);
        }
    }

    fn ribbon_group(&mut self, ui: &mut egui::Ui, title_key: &str, ids: &[CommandId]) {
        let title = self.i18n.tr(title_key);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    for &id in ids {
                        self.ribbon_button(ui, id);
                    }
                });
                ui.label(egui::RichText::new(title).small().weak());
            });
        });
    }

    fn ribbon(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (tab, key) in [
                (RibbonTab::Structure, "tab-structure"),
                (RibbonTab::Annotate, "tab-annotate"),
                (RibbonTab::View, "tab-view"),
            ] {
                let label = self.i18n.tr(key);
                ui.selectable_value(&mut self.ribbon_tab, tab, label);
            }
        });
        ui.horizontal(|ui| match self.ribbon_tab {
            RibbonTab::Structure => {
                self.ribbon_group(ui, "group-structure", &[CommandId::Wall, CommandId::Column, CommandId::Beam, CommandId::Slab]);
                self.ribbon_group(ui, "group-datum", &[CommandId::Grid, CommandId::Level]);
                self.ribbon_group(ui, "group-modify", &[CommandId::Move, CommandId::Copy]);
            }
            RibbonTab::Annotate => {
                self.ribbon_group(ui, "group-datum", &[CommandId::Dimension]);
            }
            RibbonTab::View => {
                let title = self.i18n.tr("group-language");
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            let es = self.i18n.tr("cmd-lang-es");
                            let en = self.i18n.tr("cmd-lang-en");
                            let mut lang = self.i18n.lang();
                            ui.selectable_value(&mut lang, Lang::EsAr, es);
                            ui.selectable_value(&mut lang, Lang::En, en);
                            self.i18n.set_lang(lang);
                        });
                        ui.label(egui::RichText::new(title).small().weak());
                    });
                });
            }
        });
    }

    /// AutoCAD-style command line with autocomplete above it.
    fn command_line(&mut self, ui: &mut egui::Ui) {
        let hint = self.i18n.tr("cmdline-hint");
        let edit_id = egui::Id::new("cmdline");
        let matches = commands::complete(&self.i18n, &self.cmdline);
        self.completion_sel = self.completion_sel.min(matches.len().saturating_sub(1));

        // Keys that drive the popup must be consumed before the TextEdit sees them.
        let focused = ui.memory(|m| m.has_focus(edit_id));
        let (mut accept, mut submit) = (false, false);
        if focused && !matches.is_empty() {
            ui.input_mut(|i| {
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                    self.completion_sel = (self.completion_sel + 1) % matches.len();
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                    self.completion_sel = (self.completion_sel + matches.len() - 1) % matches.len();
                }
                accept = i.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
            });
        }

        ui.horizontal(|ui| {
            ui.label("⏵");
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.cmdline)
                    .id(edit_id)
                    .hint_text(hint)
                    .desired_width(320.0),
            );
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                submit = true;
            }
            ui.label(egui::RichText::new(&self.status).weak());

            if focused && !matches.is_empty() {
                let rows = matches.len() as f32;
                let pos = resp.rect.left_top() - egui::vec2(0.0, rows * 22.0 + 12.0);
                egui::Area::new(egui::Id::new("cmdline-completions"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(pos)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.set_min_width(resp.rect.width());
                            for (i, &id) in matches.iter().enumerate() {
                                let c = commands::get(id);
                                let text = format!("{}   {}   {}", self.i18n.tr(c.key), c.global, c.shortcut);
                                if ui.selectable_label(i == self.completion_sel, text).clicked() {
                                    self.completion_sel = i;
                                    accept = true;
                                }
                            }
                        });
                    });
            }
        });

        if accept && let Some(&id) = matches.get(self.completion_sel) {
            self.cmdline = self.i18n.tr(commands::get(id).key);
            submit = true;
        }
        if submit {
            let input = std::mem::take(&mut self.cmdline);
            if let Some(id) = commands::resolve(&self.i18n, &input).or_else(|| matches.first().copied()) {
                self.run(id);
            } else if !input.trim().is_empty() {
                let mut args = fluent_bundle::FluentArgs::new();
                args.set("input", input);
                self.status = self.i18n.tr_args("cmdline-unknown", Some(&args));
            }
            ui.memory_mut(|m| m.request_focus(edit_id));
            self.completion_sel = 0;
        }
    }

    /// Revit-style two-letter shortcuts, active when no text field has focus.
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.memory(|m| m.focused().is_some()) {
            self.shortcut_buf.clear();
            return;
        }
        let typed: String = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Text(t) => Some(t.clone()),
                    _ => None,
                })
                .collect()
        });
        for ch in typed.chars().filter(|c| c.is_ascii_alphabetic()) {
            self.shortcut_buf.push(ch);
            if self.shortcut_buf.len() == 2 {
                if let Some(id) = commands::by_shortcut(&self.shortcut_buf) {
                    self.run(id);
                }
                self.shortcut_buf.clear();
            }
        }
    }
}

/// Labels a row of the property grid and links the label to the editor, so
/// screen readers announce "Height, 3.00 m" instead of an anonymous number.
fn prop_row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui) -> egui::Response) {
    let l = ui.label(label);
    let r = add(ui);
    r.labelled_by(l.id);
    ui.end_row();
}

fn length(value: &mut f64) -> egui::DragValue<'_> {
    egui::DragValue::new(value).speed(0.01).range(0.0..=100.0).fixed_decimals(3).suffix(" m")
}

fn combo(ui: &mut egui::Ui, id: &str, sel: &mut usize, items: &[&str]) -> egui::Response {
    egui::ComboBox::from_id_salt(id)
        .selected_text(items[*sel])
        .show_index(ui, sel, items.len(), |i| items[i])
}

struct Tabs<'a> {
    app: &'a mut HitoApp,
    rs: Option<egui_wgpu::RenderState>,
}

impl egui_dock::TabViewer for Tabs<'_> {
    type Tab = Panel;

    fn id(&mut self, tab: &mut Panel) -> egui::Id {
        egui::Id::new(*tab as u8)
    }

    fn title(&mut self, tab: &mut Panel) -> egui::WidgetText {
        let key = match tab {
            Panel::Browser => "panel-browser",
            Panel::Properties => "panel-properties",
            Panel::Viewport => "panel-viewport",
            Panel::Elements => "panel-elements",
            Panel::Text => "panel-text",
        };
        self.app.i18n.tr(key).into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Panel) {
        let tr = |k: &str| self.app.i18n.tr(k);
        match tab {
            Panel::Browser => {
                egui::CollapsingHeader::new("Vistas").default_open(true).show(ui, |ui| {
                    egui::CollapsingHeader::new("Plantas estructurales").default_open(true).show(ui, |ui| {
                        for l in LEVELS {
                            let _ = ui.selectable_label(false, *l);
                        }
                    });
                    let _ = ui.selectable_label(true, "{3D}");
                });
                egui::CollapsingHeader::new("Planos").show(ui, |ui| {
                    ui.label("E-01 Fundaciones");
                    ui.label("E-02 Losa 1P");
                });
            }
            Panel::Properties => {
                let (t_type, t_cons, t_dims, t_struct) =
                    (tr("props-type"), tr("props-constraints"), tr("props-dimensions"), tr("props-structural"));
                let names = [
                    tr("props-base-level"),
                    tr("props-top-level"),
                    tr("props-width"),
                    tr("props-depth"),
                    tr("props-height"),
                    tr("props-material"),
                    tr("props-cover"),
                ];
                let p = &mut self.app.props;
                ui.horizontal(|ui| {
                    ui.label(&t_type);
                    combo(ui, "type", &mut p.type_name, TYPES);
                });
                egui::CollapsingHeader::new(t_cons).default_open(true).show(ui, |ui| {
                    egui::Grid::new("cons").num_columns(2).striped(true).show(ui, |ui| {
                        prop_row(ui, &names[0], |ui| combo(ui, "base", &mut p.base_level, LEVELS));
                        prop_row(ui, &names[1], |ui| combo(ui, "top", &mut p.top_level, LEVELS));
                    });
                });
                egui::CollapsingHeader::new(t_dims).default_open(true).show(ui, |ui| {
                    egui::Grid::new("dims").num_columns(2).striped(true).show(ui, |ui| {
                        prop_row(ui, &names[2], |ui| ui.add(length(&mut p.width)));
                        prop_row(ui, &names[3], |ui| ui.add(length(&mut p.depth)));
                        prop_row(ui, &names[4], |ui| ui.add(length(&mut p.height)));
                    });
                });
                egui::CollapsingHeader::new(t_struct).default_open(true).show(ui, |ui| {
                    egui::Grid::new("struct").num_columns(2).striped(true).show(ui, |ui| {
                        prop_row(ui, &names[5], |ui| combo(ui, "mat", &mut p.material, MATERIALS));
                        prop_row(ui, &names[6], |ui| ui.add(length(&mut p.cover)));
                    });
                });
            }
            Panel::Viewport => {
                let hint = tr("viewport-hint");
                self.app.viewport.ui(ui, self.rs.as_ref(), &hint);
            }
            Panel::Elements => {
                // Virtualised: only visible rows are laid out, so 100,000 rows cost the same as 30.
                const N: usize = 100_000;
                let mut args = fluent_bundle::FluentArgs::new();
                args.set("count", N);
                ui.label(self.app.i18n.tr_args("elements-count", Some(&args)));
                let row_h = ui.text_style_height(&egui::TextStyle::Body);
                egui::ScrollArea::vertical().auto_shrink(false).show_rows(ui, row_h, N, |ui, range| {
                    for i in range {
                        ui.label(format!("P-{i:06}   {}   {}   {}", TYPES[i % 3], LEVELS[i % 4], MATERIALS[i % 3]));
                    }
                });
            }
            Panel::Text => text::ui(ui, &self.app.fonts),
        }
    }

    fn is_closeable(&self, _tab: &Panel) -> bool {
        false
    }
}

impl eframe::App for HitoApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shortcuts(&ctx);
        egui::Panel::top("ribbon").show(ui, |ui| self.ribbon(ui));
        egui::Panel::bottom("cmdline").show(ui, |ui| self.command_line(ui));
        let rs = frame.wgpu_render_state().cloned();
        let mut dock = std::mem::replace(&mut self.dock, DockState::new(vec![]));
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            DockArea::new(&mut dock)
                .show_close_buttons(false)
                .show_inside(ui, &mut Tabs { app: self, rs });
        });
        self.dock = dock;
    }
}
