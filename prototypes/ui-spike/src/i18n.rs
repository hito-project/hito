//! Fluent-based localisation. Every user-facing string goes through `tr`.

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
use unic_langid::{LanguageIdentifier, langid};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    EsAr,
    En,
}

pub struct I18n {
    lang: Lang,
    es: FluentBundle<FluentResource>,
    en: FluentBundle<FluentResource>,
}

fn bundle(id: LanguageIdentifier, src: &str) -> FluentBundle<FluentResource> {
    let res = FluentResource::try_new(src.to_owned()).expect("valid .ftl");
    let mut b = FluentBundle::new(vec![id]);
    // No Unicode isolation marks around arguments: egui would draw them as boxes.
    b.set_use_isolating(false);
    b.add_resource(res).expect("no duplicate keys");
    b
}

impl I18n {
    pub fn new(lang: Lang) -> Self {
        Self {
            lang,
            es: bundle(langid!("es-AR"), include_str!("../i18n/es-AR.ftl")),
            en: bundle(langid!("en"), include_str!("../i18n/en.ftl")),
        }
    }

    pub fn lang(&self) -> Lang {
        self.lang
    }

    pub fn set_lang(&mut self, lang: Lang) {
        self.lang = lang;
    }

    fn current(&self) -> &FluentBundle<FluentResource> {
        match self.lang {
            Lang::EsAr => &self.es,
            Lang::En => &self.en,
        }
    }

    pub fn tr(&self, key: &str) -> String {
        self.tr_args(key, None)
    }

    pub fn tr_args(&self, key: &str, args: Option<&FluentArgs<'_>>) -> String {
        let b = self.current();
        let Some(msg) = b.get_message(key) else {
            return format!("⟨{key}⟩");
        };
        let Some(pattern) = msg.value() else {
            return format!("⟨{key}⟩");
        };
        let mut errors = vec![];
        b.format_pattern(pattern, args, &mut errors).into_owned()
    }
}
