//! Interface languages and localization.
//! Strings in code stay in English and act as lookup keys into language catalogs (`*.tsv`).

mod catalog;

use std::cell::Cell;
use std::sync::OnceLock;

use catalog::Catalog;

pub const AUTO: &str = "auto";

pub struct LangInfo {
    pub code: &'static str,
    pub name: &'static str,
    pub source: &'static str,
    catalog: OnceLock<Catalog>,
}

pub static LANGUAGES: [LangInfo; 2] = [
    LangInfo { code: "en", name: "English", source: "", catalog: OnceLock::new() },
    LangInfo { code: "it", name: "Italiano", source: include_str!("it.tsv"), catalog: OnceLock::new() },
];

impl LangInfo {
    fn catalog(&self) -> &Catalog {
        self.catalog.get_or_init(|| {
            let (catalog, errors) = Catalog::parse(self.source);
            for error in errors {
                log::warn!("{} interface catalog: {error}; that entry shows in English", self.code);
            }
            catalog
        })
    }
}

#[derive(Clone, Copy)]
pub struct Lang(&'static LangInfo);

impl std::fmt::Debug for Lang {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Lang({})", self.0.code)
    }
}

impl PartialEq for Lang {
    fn eq(&self, other: &Self) -> bool {
        self.0.code == other.0.code
    }
}

impl Eq for Lang {}

impl Lang {
    pub const EN: Lang = Lang(&LANGUAGES[0]);
    pub const IT: Lang = Lang(&LANGUAGES[1]);

    pub fn code(self) -> &'static str {
        self.0.code
    }

    pub fn name(self) -> &'static str {
        self.0.name
    }

    pub fn from_code(code: &str) -> Option<Lang> {
        LANGUAGES.iter().find(|l| l.code.eq_ignore_ascii_case(code)).map(Lang)
    }

    pub fn from_pref(pref: &str) -> Lang {
        Lang::from_code(pref).unwrap_or_else(system_lang)
    }

    pub fn all() -> impl Iterator<Item = Lang> {
        LANGUAGES.iter().map(Lang)
    }

    fn catalog(self) -> &'static Catalog {
        self.0.catalog()
    }
}

pub fn normalize_pref(pref: &str) -> Option<&'static str> {
    if pref.eq_ignore_ascii_case(AUTO) {
        return Some(AUTO);
    }
    Lang::from_code(pref).map(Lang::code)
}

fn candidates(tag: &str) -> Vec<String> {
    let base = tag.split(['.', '@']).next().unwrap_or("").replace('_', "-").to_ascii_lowercase();
    let parts: Vec<&str> = base.split('-').filter(|p| !p.is_empty()).collect();
    let mut out = Vec::new();
    for n in (1..=parts.len()).rev() {
        out.push(parts.get(..n).unwrap_or_default().join("-"));
    }
    out
}

pub fn lang_from_tag(tag: &str) -> Option<Lang> {
    let cands = candidates(tag);
    if matches!(cands.first().map(String::as_str), Some("c" | "posix")) {
        return Some(Lang::EN);
    }
    cands.iter().find_map(|c| Lang::from_code(c))
}

pub fn first_supported<'a>(tags: impl IntoIterator<Item = &'a str>) -> Option<Lang> {
    tags.into_iter().find_map(lang_from_tag)
}

pub fn system_lang() -> Lang {
    if cfg!(test) {
        return Lang::EN;
    }
    static SYSTEM: OnceLock<Lang> = OnceLock::new();
    *SYSTEM.get_or_init(detect_system_lang)
}

fn detect_system_lang() -> Lang {
    if let Some(l) = std::env::var("DECKCRAFT_LANGUAGE").ok().and_then(|v| lang_from_tag(&v)) {
        return l;
    }
    let tags: Vec<String> = sys_locale::get_locales().take(16).collect();
    first_supported(tags.iter().map(String::as_str)).unwrap_or(Lang::EN)
}

thread_local! {
    static CURRENT: Cell<Lang> = const { Cell::new(Lang::EN) };
}

pub fn set_current(lang: Lang) {
    CURRENT.set(lang);
}

pub fn current() -> Lang {
    CURRENT.get()
}

pub fn t(s: &str) -> &str {
    tr(current(), s)
}

pub fn tr(lang: Lang, s: &str) -> &str {
    lang.catalog().plain(s).unwrap_or(s)
}

pub fn tc<'a>(context: &str, s: &'a str) -> &'a str {
    trc(current(), context, s)
}

pub fn trc<'a>(lang: Lang, context: &str, s: &'a str) -> &'a str {
    lang.catalog().in_context(context, s).unwrap_or(s)
}

pub fn fmt(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some((before, after_open)) = rest.split_once('{') {
        out.push_str(before);
        let Some((name, after_close)) = after_open.split_once('}') else {
            out.push('{');
            out.push_str(after_open);
            return out;
        };
        match args.iter().find(|(key, _)| *key == name) {
            Some((_, value)) => out.push_str(value),
            None => {
                out.push('{');
                out.push_str(name);
                out.push('}');
            }
        }
        rest = after_close;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_parses_without_error() {
        let (cat, errs) = Catalog::parse(include_str!("it.tsv"));
        assert!(errs.is_empty(), "Catalog errors: {errs:?}");
        assert_eq!(cat.plain("Insert"), Some("Inserisci"));
        assert_eq!(cat.plain("Transitions"), Some("Transizioni"));
        assert_eq!(cat.plain("Slide Show"), Some("Presentazione"));
    }

    #[test]
    fn tr_translates_known_and_keeps_unknown() {
        assert_eq!(tr(Lang::IT, "Home"), "Home");
        assert_eq!(tr(Lang::IT, "Insert"), "Inserisci");
        assert_eq!(tr(Lang::IT, "Unknown String"), "Unknown String");
    }
}
