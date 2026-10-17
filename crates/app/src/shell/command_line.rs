//! The command line at the bottom of the window, as in AutoCAD, and Revit's
//! keyboard shortcuts
//! ([ADR 0007](https://github.com/hito-project/hito/blob/main/docs/decisions/0007-ui-familiar-to-current-users.md)).
//!
//! Typing in the command line lists the commands that match, above it. A
//! running command shows its prompt in place of the command line's label,
//! and takes the answer from it. With nothing focused, a command's alias
//! runs it at once, as Revit's shortcuts do.

use eframe::egui;

use crate::command::{Command, Registry, Session};
use crate::i18n::I18n;

/// The command line's state between frames.
#[derive(Debug, Default)]
pub(super) struct CommandLine {
    text: String,
    /// The highlighted completion.
    selected: usize,
    /// The keys of a shortcut typed so far.
    keys: String,
    /// Whether a command was running in the last frame.
    prompting: bool,
    /// Whether the command line had the focus when the running command
    /// started, so it keeps the focus when the command ends.
    keep_focus: bool,
    /// Move the focus here in the next frame.
    focus: bool,
}

/// What the user asked for in the command line.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Request {
    /// Entered text, for the session to read.
    Enter(String),
    /// Picked a completion.
    Run(&'static str),
}

const ID: &str = "command-line";

impl CommandLine {
    fn id() -> egui::Id {
        egui::Id::new(ID)
    }

    /// Clears what was typed, as Esc does.
    pub fn clear(&mut self) {
        self.text.clear();
        self.keys.clear();
        self.selected = 0;
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, i18n: &I18n, session: &Session) -> Option<Request> {
        let id = Self::id();
        let running = session.running().is_some();
        let focused = ui.memory(|m| m.has_focus(id));
        if running && !self.prompting {
            // A command just started: its prompts come here.
            self.keep_focus = focused || self.focus;
            self.focus = true;
        } else if !running && self.prompting && !self.keep_focus {
            ui.memory_mut(|m| m.surrender_focus(id));
        }
        self.prompting = running;
        if std::mem::take(&mut self.focus) {
            ui.memory_mut(|m| m.request_focus(id));
        }

        let registry = session.registry();
        let completions = if running || !focused {
            Vec::new()
        } else {
            registry.complete(i18n, &self.text)
        };
        self.selected = self.selected.min(completions.len().saturating_sub(1));

        // The keys that move through the completions are taken before the
        // text field sees them.
        let mut picked = None;
        if !completions.is_empty() {
            ui.input_mut(|i| {
                let len = completions.len();
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                    self.selected = (self.selected + 1) % len;
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                    self.selected = (self.selected + len - 1) % len;
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::Tab) {
                    picked = Some(self.selected);
                }
            });
        }

        if let Some(message) = session.message() {
            ui.label(egui::RichText::new(message).weak());
        }
        let mut entered = false;
        let response = ui
            .horizontal(|ui| {
                let label = match session.prompt() {
                    Some(prompt) => i18n.tr(prompt.message),
                    None => i18n.tr("command-line-label"),
                };
                let label = ui.label(label);
                let edit = egui::TextEdit::singleline(&mut self.text)
                    .id(id)
                    .hint_text(i18n.tr("command-line-hint"))
                    .desired_width(f32::INFINITY);
                let response = ui.add(edit).labelled_by(label.id);
                entered = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                response
            })
            .inner;

        if !completions.is_empty() {
            let clicked = completion_list(ui, &response, i18n, &completions, self.selected);
            picked = picked.or(clicked);
        }

        let request = if let Some(index) = picked {
            Some(Request::Run(completions[index].name))
        } else if entered {
            // Enter runs the highlighted completion, unless the text names a
            // command exactly.
            let exact = registry.resolve(i18n, &self.text).is_some();
            match completions.get(self.selected) {
                Some(command) if !exact => Some(Request::Run(command.name)),
                _ => Some(Request::Enter(self.text.clone())),
            }
        } else {
            None
        };
        if request.is_some() {
            self.clear();
            // Stay in the command line for the next command or answer.
            self.focus = true;
            if !running {
                self.keep_focus = true;
            }
        }
        request
    }

    /// Revit's keyboard shortcuts: with nothing focused, typing a command's
    /// alias runs it. Returns that command's name.
    pub fn shortcuts(&mut self, ctx: &egui::Context, registry: &Registry) -> Option<&'static str> {
        if ctx.memory(|m| m.focused().is_some()) {
            self.keys.clear();
            return None;
        }
        // The keys are taken, so that the command line doesn't also get them
        // when a command moves the focus there.
        let mut typed = Vec::new();
        ctx.input_mut(|i| {
            i.events.retain(|event| match event {
                egui::Event::Text(text) => {
                    typed.push(text.clone());
                    false
                }
                _ => true,
            })
        });
        let mut run = None;
        for text in typed {
            for c in text.chars() {
                if c == ' ' {
                    // Space runs an alias that a longer one starts with.
                    run = run.or(registry.by_alias(&self.keys).map(|c| c.name));
                    self.keys.clear();
                    continue;
                }
                if !c.is_alphanumeric() {
                    self.keys.clear();
                    continue;
                }
                self.keys.push(c);
                if !registry.alias_continues(&self.keys) && registry.by_alias(&self.keys).is_none()
                {
                    // No alias starts this way: start again from this key.
                    self.keys = c.to_string();
                }
                if registry.alias_continues(&self.keys) {
                    continue;
                }
                run = run.or(registry.by_alias(&self.keys).map(|c| c.name));
                self.keys.clear();
            }
        }
        run
    }
}

/// The completions, in a list above the command line. Returns the one
/// clicked.
fn completion_list(
    ui: &egui::Ui,
    line: &egui::Response,
    i18n: &I18n,
    completions: &[&Command],
    selected: usize,
) -> Option<usize> {
    let mut clicked = None;
    egui::Area::new(egui::Id::new("command-line-completions"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::LEFT_BOTTOM)
        .fixed_pos(line.rect.left_top())
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(line.rect.width());
                for (index, command) in completions.iter().enumerate() {
                    // The label, then the names that work in any language.
                    let mut text = i18n.tr(command.label);
                    text.push_str("   ");
                    text.push_str(command.name);
                    if let Some(alias) = command.alias {
                        text.push_str("   ");
                        text.push_str(alias);
                    }
                    if ui.selectable_label(index == selected, text).clicked() {
                        clicked = Some(index);
                    }
                }
            });
        });
    clicked
}
