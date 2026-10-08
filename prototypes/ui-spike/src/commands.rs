//! A tiny command registry: every ribbon button, typed command and shortcut
//! resolves to the same `CommandId`.

use crate::i18n::I18n;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandId {
    Wall,
    Column,
    Beam,
    Slab,
    Grid,
    Level,
    Move,
    Copy,
    Dimension,
}

pub struct Command {
    pub id: CommandId,
    /// Fluent key of the localised name.
    pub key: &'static str,
    /// Revit-style two-letter shortcut.
    pub shortcut: &'static str,
    /// Language-neutral name, accepted in any UI language (like AutoCAD's `_LINE`).
    pub global: &'static str,
}

pub const COMMANDS: &[Command] = &[
    Command { id: CommandId::Wall, key: "cmd-wall", shortcut: "WA", global: "WALL" },
    Command { id: CommandId::Column, key: "cmd-column", shortcut: "CL", global: "COLUMN" },
    Command { id: CommandId::Beam, key: "cmd-beam", shortcut: "BM", global: "BEAM" },
    Command { id: CommandId::Slab, key: "cmd-slab", shortcut: "SB", global: "SLAB" },
    Command { id: CommandId::Grid, key: "cmd-grid", shortcut: "GR", global: "GRID" },
    Command { id: CommandId::Level, key: "cmd-level", shortcut: "LL", global: "LEVEL" },
    Command { id: CommandId::Move, key: "cmd-move", shortcut: "MV", global: "MOVE" },
    Command { id: CommandId::Copy, key: "cmd-copy", shortcut: "CO", global: "COPY" },
    Command { id: CommandId::Dimension, key: "cmd-dimension", shortcut: "DI", global: "DIMENSION" },
];

pub fn get(id: CommandId) -> &'static Command {
    COMMANDS.iter().find(|c| c.id == id).expect("registered")
}

/// Lower-case and strip Spanish accents, so "espanol" matches "Español" and "area" matches "Área".
pub fn fold(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' => 'a',
            'é' => 'e',
            'í' => 'i',
            'ó' => 'o',
            'ú' | 'ü' => 'u',
            'ñ' => 'n',
            c => c,
        })
        .collect()
}

/// Autocomplete: commands whose localised name, global name or shortcut
/// starts with the input come first, then those that contain it.
pub fn complete(i18n: &I18n, input: &str) -> Vec<CommandId> {
    let q = fold(input.trim());
    if q.is_empty() {
        return vec![];
    }
    let mut prefix = vec![];
    let mut contains = vec![];
    for c in COMMANDS {
        let names = [fold(&i18n.tr(c.key)), fold(c.global), fold(c.shortcut)];
        if names.iter().any(|n| n.starts_with(&q)) {
            prefix.push(c.id);
        } else if names.iter().any(|n| n.contains(&q)) {
            contains.push(c.id);
        }
    }
    prefix.extend(contains);
    prefix
}

/// Exact match on name, global name or shortcut.
pub fn resolve(i18n: &I18n, input: &str) -> Option<CommandId> {
    let q = fold(input.trim());
    COMMANDS
        .iter()
        .find(|c| fold(&i18n.tr(c.key)) == q || fold(c.global) == q || fold(c.shortcut) == q)
        .map(|c| c.id)
}

pub fn by_shortcut(keys: &str) -> Option<CommandId> {
    COMMANDS.iter().find(|c| c.shortcut.eq_ignore_ascii_case(keys)).map(|c| c.id)
}
