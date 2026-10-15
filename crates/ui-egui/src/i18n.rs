//! Interface text only: presentation content, file names and command identifiers stay intact.

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Language {
    #[default]
    System,
    PtBr,
    En,
}

impl Language {
    pub const ALL: [Self; 3] = [Self::System, Self::PtBr, Self::En];

    pub fn label(self) -> &'static str {
        match self {
            Self::System => tr("System language"),
            Self::PtBr => "Português (Brasil)",
            Self::En => "English",
        }
    }

    pub fn from_locale(locale: &str) -> Self {
        let language = locale.split(['-', '_', '.', '@']).next().unwrap_or("");
        if language.eq_ignore_ascii_case("pt") {
            Self::PtBr
        } else {
            Self::En
        }
    }

    pub fn resolve(self, locale: Option<&str>) -> Self {
        if self == Self::System {
            Self::from_locale(locale.unwrap_or(""))
        } else {
            self
        }
    }
}

thread_local! {
    static CURRENT: Cell<Language> = const { Cell::new(Language::En) };
}

pub fn current() -> Language {
    CURRENT.get()
}

pub fn set_current(language: Language) {
    deckcraft_model::set_pt_br(matches!(language, Language::PtBr));
    CURRENT.set(language);
}

const CATALOG_PT_BR: &str = include_str!("i18n/pt_br.tsv");
static PT_BR: OnceLock<HashMap<String, String>> = OnceLock::new();

fn catalog() -> &'static HashMap<String, String> {
    PT_BR.get_or_init(|| {
        CATALOG_PT_BR
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .filter_map(|line| line.split_once('\t'))
            .map(|(source, text)| (source.replace("\\n", "\n"), text.replace("\\n", "\n")))
            .collect()
    })
}

/// Exact lookup at an explicit interface boundary; unknown text falls back to English.
pub fn tr(text: &str) -> &str {
    if CURRENT.get() == Language::PtBr {
        catalog().get(text).map(String::as_str).unwrap_or(text)
    } else {
        text
    }
}

/// Context-specific interface label with the original English source as fallback.
pub fn tr_context<'a>(source: &'a str, key: &str) -> &'a str {
    if CURRENT.get() == Language::PtBr {
        catalog().get(key).map(String::as_str).unwrap_or(source)
    } else {
        source
    }
}

pub fn placeholder_prompt(kind: deckcraft_model::PhType) -> &'static str {
    tr(kind.prompt())
}

/// Substitute already-formatted values without interpreting their contents as interface text.
pub fn format(source: &str, values: &[String]) -> String {
    let text = tr(source);
    let mut out = String::new();
    let mut rest = text;
    let mut index = 0;
    while let Some(start) = rest.find('{') {
        out.push_str(rest.get(..start).unwrap_or(""));
        let tail = rest.get(start + 1..).unwrap_or("");
        if let Some(after) = tail.strip_prefix('{') {
            out.push('{');
            rest = after;
        } else if let Some(end) = tail.find('}') {
            let key = &tail[..end];
            let val = if let Ok(i) = key.parse::<usize>() {
                values.get(i).map(String::as_str)
            } else {
                let v = values.get(index).map(String::as_str);
                index += 1;
                v
            };
            out.push_str(val.unwrap_or(""));
            rest = tail.get(end + 1..).unwrap_or("");
        } else {
            out.push_str(rest);
            return out;
        }
    }
    out.push_str(&rest.replace("}}", "}"));
    out
}

/// Integer count forms
pub fn count(n: usize, english: &str, singular: &str, plural: &str) -> String {
    let word = if CURRENT.get() == Language::PtBr {
        if n == 1 { singular } else { plural }
    } else {
        english
    };
    std::format!("{n} {word}")
}

/// Recovery notice
pub fn recovered(n: usize) -> String {
    let amount = count(
        n,
        if n == 1 { "unsaved presentation" } else { "unsaved presentations" },
        "apresentação não salva",
        "apresentações não salvas",
    );
    format("Recovered {} from the last session", &[amount])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translations() {
        set_current(Language::PtBr);
        assert_eq!(tr("Cut"), "Recortar");
        assert_eq!(tr("Copy"), "Copiar");
        assert_eq!(tr("Paste"), "Colar");
        assert_eq!(tr("Grid & Guides"), "Grade e Guias");
        assert_eq!(tr("Format Shape"), "Formatar Forma");
        assert_eq!(tr("Animation Pane"), "Painel de Animação");
        assert_eq!(tr_context("Home", "@ribbon:Home"), "Página Inicial");
        assert_eq!(tr_context("Insert", "@ribbon:Insert"), "Inserir");
        assert_eq!(tr("New\nSlide"), "Novo\nSlide");
        assert_eq!(tr("Add\nAnimation"), "Adicionar\nAnimação");
        assert_eq!(tr("Account"), "Conta");
        assert_eq!(tr("Options"), "Opções");
        assert_eq!(tr("Preset gradients"), "Gradientes predefinidos");
        assert_eq!(format("Slide {0} of {1}", &["1".into(), "5".into()]), "Slide 1 de 5");
    }

    #[test]
    fn test_english_fallback() {
        set_current(Language::En);
        assert_eq!(tr("Cut"), "Cut");
        assert_eq!(tr_context("Home", "@ribbon:Home"), "Home");
    }

    #[test]
    fn test_format_and_count() {
        set_current(Language::PtBr);
        assert_eq!(recovered(1), "Recuperadas 1 apresentação não salva da última sessão");
        assert_eq!(recovered(3), "Recuperadas 3 apresentações não salvas da última sessão");
    }
}
