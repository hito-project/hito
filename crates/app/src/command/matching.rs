//! Finding commands by what the user typed: their localised label, their
//! language-neutral name or their alias, ignoring case and accents.
//!
//! Completion works like AutoCAD's AutoComplete with mid-string search:
//! commands that start with the input come first, then those that contain
//! it.

use icu_normalizer::DecomposingNormalizerBorrowed;
use icu_normalizer::properties::CanonicalCombiningClassMapBorrowed;

use super::{Command, Registry};
use crate::i18n::I18n;

/// `text` in lower case, without accents or other combining marks, so
/// `area` matches `Área` and `nino` matches `Niño`.
///
/// It decomposes `text` (Unicode NFKD) and drops the combining marks, so it
/// works for any script, not only Spanish. NFKD also turns compatibility
/// forms into plain ones, such as `m²` into `m2`.
pub fn fold(text: &str) -> String {
    let marks = CanonicalCombiningClassMapBorrowed::new();
    DecomposingNormalizerBorrowed::new_nfkd()
        .normalize_iter(text.chars().flat_map(char::to_lowercase))
        .filter(|&c| marks.get_u8(c) == 0)
        .collect()
}

impl Registry {
    /// The commands that match `input`: first those whose label, name or
    /// alias starts with it, then those that contain it.
    pub fn complete(&self, i18n: &I18n, input: &str) -> Vec<&Command> {
        let query = fold(input.trim());
        if query.is_empty() {
            return Vec::new();
        }
        let (mut starts, mut contains) = (Vec::new(), Vec::new());
        for command in self.iter() {
            let names = names(i18n, command);
            if names.iter().any(|n| n.starts_with(&query)) {
                starts.push(command);
            } else if names.iter().any(|n| n.contains(&query)) {
                contains.push(command);
            }
        }
        starts.extend(contains);
        starts
    }

    /// The command whose label, name or alias is exactly `input`.
    pub fn resolve(&self, i18n: &I18n, input: &str) -> Option<&Command> {
        let query = fold(input.trim());
        self.iter()
            .find(|command| names(i18n, command).contains(&query))
    }
}

/// The folded names a command answers to.
fn names(i18n: &I18n, command: &Command) -> Vec<String> {
    let mut names = vec![fold(&i18n.tr(command.label)), fold(command.name)];
    names.extend(command.alias.map(fold));
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Run;
    use crate::i18n::Locale;

    fn registry() -> Registry {
        let mut registry = Registry::new();
        for (name, label, alias) in [
            ("BEAM", "command-beam", Some("BM")),
            ("COLUMN", "command-column", Some("CL")),
            ("ISOLATEDFOOTING", "command-isolated-footing", None),
            ("UNITS", "command-project-units", Some("UN")),
        ] {
            let run = Run::Unavailable;
            registry
                .register(Command {
                    name,
                    label,
                    alias,
                    run,
                })
                .unwrap();
        }
        registry
    }

    fn names(found: Vec<&Command>) -> Vec<&'static str> {
        found.into_iter().map(|c| c.name).collect()
    }

    #[test]
    fn fold_drops_case_and_accents() {
        assert_eq!(fold("Configuración"), "configuracion");
        assert_eq!(fold("NIÑO"), "nino");
        assert_eq!(fold("Pingüino"), "pinguino");
        assert_eq!(fold("Ça"), "ca");
        assert_eq!(fold("m²"), "m2");
    }

    #[test]
    fn completion_by_label_name_or_alias() {
        let registry = registry();
        let es = I18n::new(Locale::EsAr);
        assert_eq!(names(registry.complete(&es, "col")), ["COLUMN"]);
        assert_eq!(names(registry.complete(&es, "viga")), ["BEAM"]);
        assert_eq!(names(registry.complete(&es, "beam")), ["BEAM"]);
        // Those that start with it first: "i" starts ISOLATEDFOOTING, and is
        // inside "viga" and "units".
        let found = names(registry.complete(&es, "i"));
        assert_eq!(found, ["ISOLATEDFOOTING", "BEAM", "UNITS"]);
        assert_eq!(
            names(registry.complete(&es, "aislada")),
            ["ISOLATEDFOOTING"]
        );
        assert!(registry.complete(&es, "  ").is_empty());
    }

    #[test]
    fn resolve_is_exact_and_ignores_case_and_accents() {
        let registry = registry();
        let es = I18n::new(Locale::EsAr);
        let en = I18n::new(Locale::En);
        let resolve = |i18n, input| registry.resolve(i18n, input).map(|c| c.name);
        assert_eq!(resolve(&es, "cl"), Some("COLUMN"));
        assert_eq!(resolve(&es, "COLUMNA"), Some("COLUMN"));
        assert_eq!(resolve(&en, "column"), Some("COLUMN"));
        // The language-neutral name works in any language.
        assert_eq!(resolve(&es, "column"), Some("COLUMN"));
        assert_eq!(resolve(&es, "base aislada"), Some("ISOLATEDFOOTING"));
        assert_eq!(resolve(&es, "col"), None);
    }

    #[test]
    fn names_and_aliases_are_unique() {
        let mut registry = registry();
        let taken = |name, alias| Command {
            name,
            label: "command-beam",
            alias,
            run: Run::Unavailable,
        };
        assert!(registry.register(taken("beam", None)).is_err());
        assert!(registry.register(taken("OTHER", Some("cl"))).is_err());
        assert!(registry.register(taken("OTHER", Some("CO"))).is_ok());
        assert!(registry.alias_continues("c"));
        assert!(!registry.alias_continues("CL"));
        assert_eq!(registry.by_alias("bm").map(|c| c.name), Some("BEAM"));
    }
}
