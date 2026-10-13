//! Interface text only: presentation content, file names and command identifiers stay intact.
//!
//! A catalog per language (`i18n/*.tsv`, English source `<TAB>` translation) maps the English text
//! in the code to display text when the interface is drawn. Text without an entry shows in
//! English. Arabic is a right-to-left language: its catalog is put into display order when it
//! loads ([`crate::bidi`]), because egui lays words out left to right, and the window is mirrored
//! ([`crate::rtl`]).
//!
//! # Adding a language
//! 1. Add `xx.tsv` next to `uk.tsv` (translate from the *meaning* of the English text, clean-room:
//!    never from Microsoft PowerPoint's or another product's localisation).
//! 2. Add a [`Language`] variant, its [`Language::code`], [`Language::from_locale`] arm and
//!    catalog.

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

pub type Catalog = HashMap<String, String>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Language {
    #[default]
    System,
    En,
    Uk,
    Ar,
}

impl Language {
    pub const ALL: [Self; 4] = [Self::System, Self::En, Self::Uk, Self::Ar];

    pub fn label(self) -> &'static str {
        match self {
            Self::System => tr("System language"),
            Self::En => "English",
            Self::Uk => "Українська",
            Self::Ar => "العربية",
        }
    }

    /// The code `app.language` takes.
    pub fn code(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::En => "en",
            Self::Uk => "uk",
            Self::Ar => "ar",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.code() == code)
    }

    /// The language for a system locale such as `uk-UA`, `ar_EG.UTF-8` or `ar` (any `ar-*`);
    /// English when there is no catalog for it.
    pub fn from_locale(locale: &str) -> Self {
        let language = locale.split(['-', '_', '.', '@']).next().unwrap_or("");
        if language.eq_ignore_ascii_case("uk") {
            Self::Uk
        } else if language.eq_ignore_ascii_case("ar") {
            Self::Ar
        } else {
            Self::En
        }
    }

    pub fn resolve(self, locale: Option<&str>) -> Self {
        if self == Self::System { Self::from_locale(locale.unwrap_or("")) } else { self }
    }

    /// Is the language written right to left, so the window is mirrored?
    pub fn rtl(self) -> bool {
        self == Self::Ar
    }
}

thread_local! {
    static CURRENT: Cell<Language> = const { Cell::new(Language::En) };
}

pub fn current() -> Language {
    CURRENT.get()
}

pub fn set_current(language: Language) {
    CURRENT.set(language);
}

const CATALOG: &str = include_str!("i18n/uk.tsv");
const AR_CATALOG: &str = include_str!("i18n/ar.tsv");
static UK: OnceLock<Catalog> = OnceLock::new();
static AR: OnceLock<Catalog> = OnceLock::new();
static AR_LOGICAL: OnceLock<Catalog> = OnceLock::new();

fn parse(text: &str) -> Catalog {
    text.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_once('\t'))
        .map(|(source, text)| (source.replace("\\n", "\n"), text.replace("\\n", "\n")))
        .collect()
}

/// The Ukrainian catalog.
pub fn catalog() -> &'static Catalog {
    UK.get_or_init(|| parse(CATALOG))
}

/// The Arabic catalog, in display order (see [`crate::bidi::display_rtl`]).
pub fn arabic() -> &'static Catalog {
    AR.get_or_init(|| parse(AR_CATALOG).into_iter().map(|(source, text)| (source, crate::bidi::display_rtl(&text))).collect())
}

/// The Arabic catalog in typing order, for matching what a person types against labels.
fn arabic_logical() -> &'static Catalog {
    AR_LOGICAL.get_or_init(|| parse(AR_CATALOG))
}

/// The catalog of the language being drawn; `None` for English.
fn active() -> Option<&'static Catalog> {
    match CURRENT.get() {
        Language::Uk => Some(catalog()),
        Language::Ar => Some(arabic()),
        Language::System | Language::En => None,
    }
}

/// Exact lookup at an explicit interface boundary; unknown text falls back to English.
pub fn tr(text: &str) -> &str {
    let found = active().and_then(|catalog| catalog.get(text)).map(String::as_str);
    #[cfg(test)]
    if found.is_none() && active().is_some() {
        misses::note(text);
    }
    found.unwrap_or(text)
}

/// What the tests saw looked up without an entry in the language being drawn.
#[cfg(test)]
pub(crate) mod misses {
    use std::cell::RefCell;
    use std::collections::BTreeSet;

    thread_local! {
        static MISSES: RefCell<BTreeSet<String>> = const { RefCell::new(BTreeSet::new()) };
    }

    pub fn note(text: &str) {
        // Text already translated, or with no words (digits, symbols), is not a missing entry.
        if text.chars().any(|c| c.is_ascii_alphabetic()) {
            MISSES.with(|m| m.borrow_mut().insert(text.to_string()));
        }
    }

    pub fn take() -> BTreeSet<String> {
        MISSES.with(|m| std::mem::take(&mut *m.borrow_mut()))
    }
}

/// Every translation of the language being drawn, joined: the text the interface fonts must be
/// able to draw.
pub fn interface_text() -> String {
    active()
        .map(|catalog| {
            catalog.values().map(String::as_str).collect::<Vec<_>>().join(
                "
",
            )
        })
        .unwrap_or_default()
}

/// Like [`tr`], but a right-to-left language's text in typing order rather than display order:
/// for searching labels with what a person types.
pub fn tr_logical(text: &str) -> &str {
    match CURRENT.get() {
        Language::Ar => arabic_logical().get(text).map(String::as_str).unwrap_or(text),
        _ => tr(text),
    }
}

/// Context-specific interface label with the original English source as fallback.
pub fn tr_context<'a>(source: &'a str, key: &str) -> &'a str {
    active().and_then(|catalog| catalog.get(key)).map(String::as_str).unwrap_or(source)
}

/// The hint a placeholder shows on the slide, in typing order: the slide's own text layout puts
/// right-to-left script in order, so this is not rearranged for egui.
pub fn placeholder_prompt(kind: deckcraft_model::PhType) -> &'static str {
    tr_logical(kind.prompt())
}

/// Substitute already-formatted values without interpreting their contents as interface text.
///
/// `values` are as a person would type them. In a right-to-left language the sentence is filled in
/// typing order and the whole of it put into display order, so a name inside it falls where the
/// bidirectional rules put it ([`crate::bidi::display_rtl`]); a label without a translation is
/// English and keeps its order, with the values in display order.
pub fn format(source: &str, values: &[String]) -> String {
    if current().rtl() {
        return match arabic_logical().get(source) {
            Some(template) => crate::bidi::display_rtl(&fill(template, values, |v| v.to_string())),
            None => fill(source, values, |v| crate::bidi::visual(v).into_owned()),
        };
    }
    fill(tr(source), values, |v| v.to_string())
}

/// `source` in the interface language, for a label that may run over several lines: in a
/// right-to-left language it is broken into lines to fit the room `ui` has ([`crate::rtl::wrap`]),
/// since egui would break it from the wrong end.
pub fn prose(ui: &egui::Ui, source: &str) -> String {
    prose_fmt(ui, source, &[])
}

/// [`prose`] for a sentence with `{…}` placeholders filled by `values`.
pub fn prose_fmt(ui: &egui::Ui, source: &str, values: &[String]) -> String {
    if current().rtl()
        && let Some(template) = arabic_logical().get(source)
    {
        let font = egui::TextStyle::Body.resolve(ui.style());
        // A little short of the room: bold and shaped text is not quite as wide as it measures.
        return crate::rtl::wrap(ui, &fill(template, values, |v| v.to_string()), &font, ui.available_width() * 0.96);
    }
    format(source, values)
}

/// [`tr_logical`] with its `{…}` placeholders filled by `values`: a phrase in typing order, to go
/// into a sentence that [`format`] puts into display order as a whole.
pub fn logical(source: &str, values: &[String]) -> String {
    fill(tr_logical(source), values, |v| v.to_string())
}

/// `name` in quotation marks, as the interface language writes them: “name”, or «name» in
/// Arabic (which the bidirectional rules mirror).
pub fn quoted(name: &str) -> String {
    if current().rtl() { format!("«{name}»") } else { format!("“{name}”") }
}

/// A label with its keyboard shortcut in brackets after it: `Bold (Ctrl+B)`, or its mirror in a
/// right-to-left language.
pub fn with_shortcut(label: &str, shortcut: &str) -> String {
    if current().rtl() { crate::bidi::display_rtl(&format!("{} ({shortcut})", tr_logical(label))) } else { format!("{} ({shortcut})", tr(label)) }
}

/// `template` with its `{…}` placeholders replaced by `values`, in order (`{{` and `}}` are braces).
fn fill(template: &str, values: &[String], show: impl Fn(&str) -> String) -> String {
    let mut out = String::new();
    let mut rest = template;
    let mut index = 0;
    while let Some(start) = rest.find('{') {
        out.push_str(rest.get(..start).unwrap_or(""));
        let tail = rest.get(start + 1..).unwrap_or("");
        if let Some(after) = tail.strip_prefix('{') {
            out.push('{');
            rest = after;
        } else if let Some(end) = tail.find('}') {
            out.push_str(&show(values.get(index).map(String::as_str).unwrap_or("")));
            index += 1;
            rest = tail.get(end + 1..).unwrap_or("");
        } else {
            out.push_str(rest);
            return out;
        }
    }
    out.push_str(&rest.replace("}}", "}"));
    out
}

/// Integer count forms. Ukrainian: 1/21; 2–4/22–24; 0/5–19. Arabic uses count-neutral wording
/// (`الشرائح: 3`), so it needs no plural forms.
pub fn count(n: usize, english: &str, one: &str, few: &str, many: &str) -> String {
    let word = match CURRENT.get() {
        Language::Uk => match (n % 10, n % 100) {
            (1, last) if last != 11 => one,
            (2..=4, last) if !(12..=14).contains(&last) => few,
            _ => many,
        },
        Language::Ar => return crate::bidi::display_rtl(&format!("{}: {n}", tr_logical(arabic_noun(english)))),
        Language::System | Language::En => english,
    };
    format!("{n} {word}")
}

/// The catalog key of the plural noun a count is about (the English forms the callers pass).
fn arabic_noun(english: &str) -> &str {
    match english {
        "slide" | "slides" => "Slides",
        "presentation" | "presentations" => "Presentations",
        "Column" | "Columns" => "Columns",
        other => other,
    }
}

/// Recovery notice includes accusative Ukrainian forms.
pub fn recovered(n: usize) -> String {
    let amount = match CURRENT.get() {
        Language::Ar => format!("{} ({n})", tr_logical("unsaved presentations")),
        _ => count(
            n,
            if n == 1 { "unsaved presentation" } else { "unsaved presentations" },
            "незбережену презентацію",
            "незбережені презентації",
            "незбережених презентацій",
        ),
    };
    format("Recovered {} from the last session", &[amount])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placeholders(text: &str) -> Vec<&str> {
        let mut out = vec![];
        let mut rest = text;
        while let Some((_, tail)) = rest.split_once('{') {
            if let Some(tail) = tail.strip_prefix('{') {
                rest = tail;
                continue;
            }
            if let Some((field, tail)) = tail.split_once('}') {
                out.push(field);
                rest = tail;
            } else {
                break;
            }
        }
        out
    }

    /// Every catalog, by language name, in typing order.
    fn catalogs() -> [(&'static str, &'static Catalog); 2] {
        [("Ukrainian", catalog()), ("Arabic", arabic_logical())]
    }

    /// Every label the interface takes from tables in the code rather than from literal calls.
    fn required_labels() -> Vec<&'static str> {
        let mut required = vec![];
        required.extend(crate::UI_COMMANDS.iter().map(|c| c.1));
        required.extend(deckcraft_engine::Session::new().commands().into_iter().map(|c| c.label));
        for (menu, items) in crate::menus::menu_tree() {
            required.push(menu);
            required.extend(items.into_iter().filter(|(label, _)| *label != "-").map(|(label, _)| label));
        }
        required.extend(deckcraft_geom::preset::CATALOG.iter().map(|shape| shape.label));
        required.extend(deckcraft_geom::preset::Category::ALL.iter().map(|category| category.label()));
        required.extend(deckcraft_model::anim::ANIMATIONS.iter().map(|effect| effect.1));
        required.extend(deckcraft_model::anim::TRANSITIONS.iter().flat_map(|effect| [effect.1, effect.2]));
        required.extend(deckcraft_model::chart::ChartType::MAIN.iter().map(|kind| kind.label()));
        required.extend(deckcraft_model::defaults::SLIDE_SIZES.iter().map(|size| size.0));
        required.extend(deckcraft_color::STANDARD_COLORS.iter().map(|color| color.1));
        required.extend(deckcraft_color::SchemeSlot::THEME.iter().map(|slot| slot.label()));
        required.extend(["@ribbon:Home", "@ribbon:Insert"]);
        required.extend(crate::backstage::translatable());
        required
    }

    #[test]
    fn all_command_menu_and_gallery_labels_are_translated() {
        let required = required_labels();
        for (language, catalog) in catalogs() {
            let transitions = deckcraft_model::anim::TRANSITIONS
                .iter()
                .flat_map(|effect| effect.4.iter().map(move |option| deckcraft_model::anim::transition_option_label(effect.0, option)));
            let animations = deckcraft_model::anim::ANIMATIONS.iter().flat_map(|effect| effect.5).map(|option| crate::ribbon::option_label(option));
            let options: Vec<String> = transitions
                .chain(animations)
                .filter(|label| !label.is_empty() && !label.chars().all(|c| c.is_ascii_digit()))
                .filter(|label| !catalog.contains_key(label))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            assert!(options.is_empty(), "missing {language} effect options: {options:#?}");
            let missing: Vec<_> = required.iter().filter(|key| !catalog.contains_key(**key)).collect();
            assert!(missing.is_empty(), "missing visible metadata in {language}: {missing:#?}");
        }
    }

    /// The text of the string literal that starts `tail` (just past its opening quote).
    fn literal(tail: &str) -> Option<String> {
        let mut escaped = false;
        let end = tail.char_indices().find_map(|(index, c)| {
            if escaped {
                escaped = false;
                None
            } else if c == '\\' {
                escaped = true;
                None
            } else if c == '"' {
                Some(index)
            } else {
                None
            }
        })?;
        serde_json::from_str(&format!("\"{}\"", &tail[..end])).ok()
    }

    #[test]
    fn every_literal_translation_call_has_a_catalog_entry() {
        let mut missing: std::collections::BTreeSet<(&str, String)> = Default::default();
        for source in [
            include_str!("backstage.rs"),
            include_str!("canvas.rs"),
            include_str!("credits.rs"),
            include_str!("dialogs.rs"),
            include_str!("fillui.rs"),
            include_str!("lib.rs"),
            include_str!("media.rs"),
            include_str!("menus.rs"),
            include_str!("panes.rs"),
            include_str!("ribbon.rs"),
            include_str!("selpane.rs"),
            include_str!("show.rs"),
            include_str!("sorter.rs"),
            include_str!("status.rs"),
            include_str!("textures.rs"),
            include_str!("thumbs.rs"),
            include_str!("toast.rs"),
            include_str!("widgets.rs"),
            include_str!("../../../apps/deckcraft/src/main.rs"),
            include_str!("../../../apps/deckcraft-web/src/web.rs"),
        ] {
            for prefix in ["i18n::tr(\"", "i18n::format(\"", "set_status(\"", "i18n::tr_logical(\"", "i18n::prose(ui, \"", "i18n::prose_fmt(ui, \""] {
                let mut rest = source.split("#[cfg(test)]").next().unwrap_or(source);
                while let Some((_, tail)) = rest.split_once(prefix) {
                    let key = literal(tail).expect("a string literal");
                    for (language, catalog) in catalogs() {
                        if !catalog.contains_key(&key) {
                            missing.insert((language, key.clone()));
                        }
                    }
                    rest = &tail[key.len().min(tail.len())..];
                }
            }
        }
        assert!(missing.is_empty(), "literal translation calls without a catalog entry: {missing:#?}");
    }

    #[test]
    fn language_command_keeps_document_data_and_identifiers_intact() {
        let mut app = crate::SlideApp::new(deckcraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", serde_json::json!({})).unwrap();
        app.run("insert.textBox", serde_json::json!({"text": "File {customer} / Звіт مرحبا", "rect": [1, 2, 100, 40]})).unwrap();
        let before = serde_json::to_value(&app.session.active().unwrap().doc).unwrap();
        app.run("app.language", serde_json::json!({"language": "uk"})).unwrap();
        assert_eq!(app.ui.interface_language, Language::Uk);
        assert_eq!(tr("File"), "Файл");
        app.run("app.language", serde_json::json!({"language": "ar"})).unwrap();
        assert_eq!(app.ui.interface_language, Language::Ar);
        assert_eq!(tr("File"), "ملف");
        assert_eq!(before, serde_json::to_value(&app.session.active().unwrap().doc).unwrap());
        assert!(app.run("app.language", serde_json::json!({"language": "invalid"})).is_err());
        assert_eq!(app.ui.interface_language, Language::Ar);
        set_current(Language::En);
    }

    #[test]
    fn bundled_ui_fonts_cover_the_ukrainian_alphabet_without_system_fonts() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Name("bold".into()), egui::FontFamily::Monospace] {
                // egui 0.36 has_glyph() can report a valid fallback face as a replacement.
                // Check the configured fallback and its actual font charmap below instead.
                let fallback_installed = ui.ctx().fonts_mut(|fonts| {
                    fonts.definitions().families.get(&family).is_some_and(|names| names.iter().any(|name| name == "Ubuntu-Light"))
                });
                assert!(fallback_installed, "missing bundled Ukrainian fallback in {family:?}");
            }
        });
        output.textures_delta.clear();
        let db = deckcraft_fonts::FontDb::with_font_dirs(vec![]);
        let face = db.face(deckcraft_fonts::LAST_RESORT_FAMILY, "Regular");
        let letters: std::collections::HashSet<_> =
            catalog().values().flat_map(|value| value.chars()).chain("ҐґЄєІіЇї".chars()).filter(|c| ('\u{0400}'..='\u{04ff}').contains(c)).collect();
        for c in letters {
            assert_ne!(face.glyph_for(c), 0, "missing document fallback glyph: {c}");
        }
    }

    /// With the craft-fonts Arabic face embedded (release builds, CI), it draws every Arabic letter
    /// the catalog uses; the shaped forms (initial, medial, final) are the shaper's business.
    #[test]
    fn the_embedded_arabic_face_covers_the_arabic_catalog() {
        if !deckcraft_fonts::CRAFT_FONTS.iter().any(|f| f.family == "Noto Sans Arabic") {
            return;
        }
        let db = deckcraft_fonts::FontDb::with_font_dirs(vec![]);
        let face = db.face("Noto Sans Arabic", "Regular");
        assert_eq!(face.family, "Noto Sans Arabic");
        for c in arabic().values().flat_map(|value| value.chars()).filter(|c| ('\u{0600}'..='\u{06FF}').contains(c)) {
            assert!(face.covers(c), "missing Arabic glyph: {c} (U+{:04X})", u32::from(c));
        }
    }

    // An explicit local visual check; the generated PNGs stay outside the repository.
    #[test]
    #[ignore = "writes translation review screenshots; run explicitly with DECKCRAFT_I18N_SHOTS"]
    fn interface_preview() {
        let dir = std::env::var("DECKCRAFT_I18N_SHOTS").unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        for (code, language) in [("en", Language::En), ("uk", Language::Uk), ("ar", Language::Ar)] {
            for (name, dialog) in [("editor", None), ("preferences", Some("preferences")), ("table", Some("table"))] {
                let mut app = crate::SlideApp::new(deckcraft_engine::Session::new(), crate::Services::default());
                app.run("file.new", serde_json::json!({})).unwrap();
                app.ui.interface_language = language;
                app.dialog = dialog.map(crate::dialogs::Dialog::new);
                let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui(|ui| {
                    app.logic(ui.ctx());
                    app.ui(ui);
                });
                harness.run_steps(6);
                harness.render().unwrap().save(format!("{dir}/{code}-{name}.png")).unwrap();
            }
        }
        set_current(Language::En);
    }

    #[test]
    fn locales_and_preferences_are_backward_compatible() {
        for locale in ["uk", "uk-UA", "uk_UA.UTF-8", "UK_ua@euro"] {
            assert_eq!(Language::from_locale(locale), Language::Uk);
        }
        for locale in ["ar", "ar-SA", "ar-EG", "ar_AE.UTF-8", "AR_ma@euro", "ar-Arab-JO"] {
            assert_eq!(Language::from_locale(locale), Language::Ar, "{locale}");
        }
        for locale in ["", "en-US", "ru-UA", "ukrainian", "arabic", "ara", "ast", "az-AZ"] {
            assert_eq!(Language::from_locale(locale), Language::En, "{locale}");
        }
        assert_eq!(Language::Uk.resolve(Some("en")), Language::Uk);
        assert_eq!(Language::System.resolve(Some("ar-SA")), Language::Ar);
        assert_eq!(Language::Ar.resolve(Some("en")), Language::Ar);
        for language in Language::ALL {
            assert_eq!(Language::from_code(language.code()), Some(language));
        }
        let old: crate::UiState = serde_json::from_str("{}").unwrap();
        assert_eq!(old.interface_language, Language::System);
        for language in [Language::Uk, Language::Ar] {
            let mut ui = old.clone();
            ui.interface_language = language;
            let saved = serde_json::to_string(&ui).unwrap();
            let round_trip: crate::UiState = serde_json::from_str(&saved).unwrap();
            assert_eq!(round_trip.interface_language, language);
        }
        assert!(serde_json::to_string(&Language::Ar).unwrap().contains("ar"));
        assert!(Language::Ar.rtl() && !Language::Uk.rtl() && !Language::En.rtl() && !Language::System.rtl());
    }

    #[test]
    fn catalogs_are_unique_complete_and_preserve_placeholders() {
        for (language, text) in [("Ukrainian", CATALOG), ("Arabic", AR_CATALOG)] {
            let mut seen = std::collections::HashSet::new();
            for (line_no, line) in text.lines().enumerate() {
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let (source, translation) = line.split_once('\t').unwrap_or_else(|| panic!("{language} line {} has no tab", line_no + 1));
                assert!(!translation.contains('\t'), "{language} line {} has a third column", line_no + 1);
                assert!(seen.insert(source), "{language}: duplicate on {}: {source}", line_no + 1);
                assert!(!translation.trim().is_empty(), "{language}: empty translation: {source}");
                assert_eq!(placeholders(source), placeholders(translation), "{language}: placeholder names/order/format: {source}");
                assert_eq!(source.ends_with('…'), translation.ends_with('…'), "{language}: trailing ellipsis differs: {source}");
            }
        }
        // Both translate exactly the same labels, so neither falls back to English for the other's.
        let mut only_one: Vec<_> = catalog().keys().filter(|key| !arabic_logical().contains_key(*key)).collect();
        only_one.extend(arabic_logical().keys().filter(|key| !catalog().contains_key(*key)));
        assert!(only_one.is_empty(), "keys in one catalog only: {only_one:?}");
    }

    #[test]
    fn lookup_and_format_preserve_user_values_and_fallbacks() {
        set_current(Language::Uk);
        assert_eq!(tr("File"), "Файл");
        assert_eq!(tr_context("Insert", "@ribbon:Insert"), "Вставлення");
        assert_eq!(tr("Insert"), "Вставити");
        assert_eq!(tr("a user-defined slide title"), "a user-defined slide title");
        let user = "File {user} / Звіт.pptx".to_string();
        assert!(format("Saved {path}", std::slice::from_ref(&user)).ends_with(&user));
        set_current(Language::En);
        assert_eq!(tr("File"), "File");
    }

    #[test]
    fn arabic_labels_are_in_display_order_and_fall_back_to_english() {
        set_current(Language::Ar);
        // One word is the same either way; two words are swapped for egui, which lays out left to
        // right; Latin words keep their own order.
        assert_eq!(tr("Save"), "حفظ");
        assert_eq!(tr("Save As…"), "…باسم حفظ");
        assert_eq!(tr_logical("Save As…"), "حفظ باسم…");
        assert_eq!(tr_context("Insert", "@ribbon:Insert"), "إدراج");
        assert_eq!(tr("a user-defined slide title"), "a user-defined slide title");
        assert_eq!(tr_logical("a user-defined slide title"), "a user-defined slide title");
        // Values go in whole, where the sentence puts them; a name is not translated or reversed.
        let name = "Quarterly Report.pptx".to_string();
        let saved = format("Saved {path}", std::slice::from_ref(&name));
        assert!(saved.contains("Quarterly Report.pptx"), "{saved}");
        assert!(saved.starts_with("Quarterly Report.pptx"), "the name leads, on the right once drawn: {saved}");
        let slide = format("Slide {} of {}", &["3".into(), "12".into()]);
        assert_eq!(slide, "12 من 3 الشريحة");
        // A name in Arabic script is put in display order too.
        let arabic_name = format("Saved {path}", &["تقرير الربع.pptx".to_string()]);
        assert!(arabic_name.contains("الربع تقرير") || arabic_name.contains("pptx."), "{arabic_name}");
        // An untranslated label keeps its English order.
        assert_eq!(format("Totally new label {}", &["x".into()]), "Totally new label x");
        set_current(Language::En);
        assert_eq!(tr("Save"), "Save");
    }

    #[test]
    fn ukrainian_integer_counts_cover_teens_and_large_values() {
        set_current(Language::Uk);
        for (n, word) in [
            (0, "слайдів"),
            (1, "слайд"),
            (2, "слайди"),
            (4, "слайди"),
            (5, "слайдів"),
            (11, "слайдів"),
            (12, "слайдів"),
            (14, "слайдів"),
            (21, "слайд"),
            (24, "слайди"),
            (111, "слайдів"),
        ] {
            assert_eq!(count(n, "slides", "слайд", "слайди", "слайдів"), format!("{n} {word}"));
        }
        set_current(Language::En);
    }

    #[test]
    fn arabic_counts_need_no_plural_forms() {
        set_current(Language::Ar);
        // "Slides: n", whatever n is: the same noun for 0, 1, 2, 3, 11 and 100.
        for n in [0, 1, 2, 3, 10, 11, 100, 101] {
            assert_eq!(count(n, "slides", "", "", ""), format!("{n} :الشرائح"));
        }
        assert_eq!(count(1, "slide", "", "", ""), "1 :الشرائح");
        assert_eq!(count(2, "Columns", "", "", ""), format!("2 :{}", tr("Columns")));
        let notice = recovered(3);
        assert!(notice.contains("(3)") && notice.contains("تمت") || notice.contains("استعادة"), "{notice}");
        set_current(Language::En);
        assert_eq!(count(2, "slides", "", "", ""), "2 slides");
        assert_eq!(recovered(2), "Recovered 2 unsaved presentations from the last session");
    }
}
