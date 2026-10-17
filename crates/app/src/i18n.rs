//! Localisation: every user-facing string goes through [`I18n::tr`]
//! ([ADR 0008](https://github.com/hito-project/hito/blob/main/docs/decisions/0008-argentina-first-international-by-design.md)).
//!
//! The text comes from Fluent message files in `i18n/`, one per locale
//! ([ADR 0015](https://github.com/hito-project/hito/blob/main/docs/decisions/0015-ui-toolkit.md)).
//! They're built into the program. Choosing the language in settings comes
//! with #54.

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource, FluentValue};
use unic_langid::{LanguageIdentifier, langid};

/// A language the user interface is available in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    /// Spanish (Argentina), the first language
    /// ([ADR 0008](https://github.com/hito-project/hito/blob/main/docs/decisions/0008-argentina-first-international-by-design.md)).
    #[default]
    EsAr,
    En,
}

impl Locale {
    pub const ALL: [Locale; 2] = [Locale::EsAr, Locale::En];

    fn id(self) -> LanguageIdentifier {
        match self {
            Locale::EsAr => langid!("es-AR"),
            Locale::En => langid!("en"),
        }
    }

    /// The locale's message file.
    pub fn messages(self) -> &'static str {
        match self {
            Locale::EsAr => include_str!("../i18n/es-AR.ftl"),
            Locale::En => include_str!("../i18n/en.ftl"),
        }
    }
}

/// The messages of one locale.
pub struct I18n {
    locale: Locale,
    bundle: FluentBundle<FluentResource>,
}

impl I18n {
    pub fn new(locale: Locale) -> Self {
        // The message files are part of the program and tested, so they parse.
        let resource =
            FluentResource::try_new(locale.messages().to_owned()).expect("the message file parses");
        let mut bundle = FluentBundle::new(vec![locale.id()]);
        bundle
            .add_resource(resource)
            .expect("the message file has no duplicate messages");
        Self { locale, bundle }
    }

    pub fn locale(&self) -> Locale {
        self.locale
    }

    /// The text of message `key`.
    pub fn tr(&self, key: &str) -> String {
        self.format(key, None)
    }

    /// The text of message `key` with a single argument, such as a count.
    pub fn tr_with<'a>(
        &self,
        key: &str,
        name: &'a str,
        value: impl Into<FluentValue<'a>>,
    ) -> String {
        self.tr_args(key, [(name, value.into())])
    }

    /// The text of message `key` with several arguments.
    pub fn tr_args<'a>(
        &self,
        key: &str,
        args: impl IntoIterator<Item = (&'a str, FluentValue<'a>)>,
    ) -> String {
        let args = args.into_iter().collect();
        self.format(key, Some(&args))
    }

    /// A missing message shows its key, so it's visible in the window and
    /// caught by the tests instead of leaving a blank.
    fn format(&self, key: &str, args: Option<&FluentArgs<'_>>) -> String {
        let Some(pattern) = self.bundle.get_message(key).and_then(|m| m.value()) else {
            return format!("⟨{key}⟩");
        };
        let mut errors = Vec::new();
        self.bundle
            .format_pattern(pattern, args, &mut errors)
            .into_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    /// The message names in a locale's file: the lines that start `name =`.
    fn keys(locale: Locale) -> BTreeSet<&'static str> {
        locale
            .messages()
            .lines()
            .filter_map(|line| line.split_once(" = ").map(|(name, _)| name))
            .filter(|name| name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
            .collect()
    }

    #[test]
    fn every_locale_has_the_same_messages() {
        let english = keys(Locale::En);
        assert!(english.contains("app-title"));
        for locale in Locale::ALL {
            assert_eq!(keys(locale), english, "{locale:?}");
        }
    }

    #[test]
    fn messages_format() {
        let i18n = I18n::new(Locale::En);
        assert_eq!(i18n.tr("panel-properties"), "Properties");
        assert_eq!(i18n.tr("view-default-3d"), "{3D}");
        assert!(i18n.tr_with("view-3d-numbered", "number", 2).contains('2'));
        assert_eq!(i18n.tr("no-such-message"), "⟨no-such-message⟩");
        assert_eq!(
            I18n::new(Locale::EsAr).tr("panel-properties"),
            "Propiedades"
        );
    }
}
