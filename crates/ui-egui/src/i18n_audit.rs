//! The Arabic interface, walked state by state: every ribbon tab, File page, dialog, task pane,
//! view, menu and ribbon control is laid out in Arabic. Nothing may panic (egui's `Grid` asserts
//! against right-to-left parents, so a grid that skips [`crate::rtl::grid`] fails here), the window
//! must be mirrored, and no English word may be drawn that is not a name, a unit or the
//! presentation's own text.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::cell::Cell;
use std::collections::{BTreeMap, HashSet};

use deckcraft_engine::Session;
use egui::{Context, Event, Pos2, Rect, vec2};
use serde_json::json;

use crate::i18n::{self, Language};
use crate::{Services, SlideApp};

thread_local! {
    // Input time, advanced every frame so tooltips and animations move on.
    static TIME: Cell<f64> = const { Cell::new(0.0) };
}

/// The text of every shape drawn, with the rectangle it fills.
fn collect(shape: &egui::Shape, out: &mut Vec<(String, Rect)>) {
    match shape {
        egui::Shape::Text(t) => out.push((t.galley.text().to_string(), t.galley.rect.translate(t.pos.to_vec2()))),
        egui::Shape::Vec(v) => v.iter().for_each(|s| collect(s, out)),
        _ => {}
    }
}

/// Run a frame per entry of `steps` (its events), then a few quiet ones, at `width` points, and
/// return every text drawn in the last.
fn frames_at(ctx: &Context, app: &mut SlideApp, width: f32, steps: Vec<Vec<Event>>) -> Vec<(String, Rect)> {
    let screen = Rect::from_min_size(Pos2::ZERO, vec2(width, 900.0));
    let mut drawn = Vec::new();
    let n = steps.len() + 3;
    let mut steps = steps.into_iter();
    for _ in 0..n {
        TIME.set(TIME.get() + 0.1);
        let input =
            egui::RawInput { screen_rect: Some(screen), time: Some(TIME.get()), events: steps.next().unwrap_or_default(), ..Default::default() };
        let mut out = ctx.run_ui(input, |ui| {
            app.logic(ui.ctx());
            app.ui(ui);
        });
        drawn.clear();
        for s in &out.shapes {
            collect(&s.shape, &mut drawn);
        }
        out.textures_delta.clear();
    }
    drawn
}

fn frame(ctx: &Context, app: &mut SlideApp) -> Vec<(String, Rect)> {
    frames_at(ctx, app, 1700.0, Vec::new())
}

/// Move there, press, release: one frame each, as a real click arrives.
fn click(at: Pos2, button: egui::PointerButton) -> Vec<Vec<Event>> {
    let press = |pressed| Event::PointerButton { pos: at, button, pressed, modifiers: Default::default() };
    vec![vec![Event::PointerMoved(at)], vec![press(true)], vec![press(false)]]
}

fn escape() -> Vec<Vec<Event>> {
    vec![vec![Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }]]
}

/// Centres of the enabled, clickable widgets drawn last frame in the ribbon's card.
fn ribbon_controls(ctx: &Context) -> Vec<Pos2> {
    ctx.viewport(|v| {
        v.prev_pass
            .widgets
            .layers()
            .filter(|(layer, _)| matches!(layer.order, egui::Order::Background | egui::Order::Middle))
            .flat_map(|(_, w)| w.iter())
            .filter(|w| w.enabled && w.sense.senses_click() && w.rect.width() < 300.0)
            .map(|w| w.rect.center())
            .filter(|c| c.y > 76.0 && c.y < 140.0)
            .collect()
    })
}

/// An app with a new presentation, showing in `language`.
fn app_in(ctx: &Context, language: Language) -> SlideApp {
    ctx.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
    let mut app = SlideApp::new(Session::with_new(), Services::default());
    app.ui.interface_language = language;
    // Fonts install over the first frames.
    for _ in 0..3 {
        frame(ctx, &mut app);
    }
    app
}

/// A slide picture as PNG bytes, for the Picture Format tab.
fn picture_png(app: &SlideApp) -> Vec<u8> {
    let doc = app.session.active().unwrap().doc.clone();
    deckcraft_render::render_slide(&doc, 0, &deckcraft_render::RenderOpts { scale: 0.2, ..Default::default() }).to_png()
}

/// Select what a contextual tab belongs to.
fn prepare(app: &mut SlideApp, tab: &str) {
    match tab {
        "Shape Format" => drop(app.run("shape.insert", json!({"preset": "rect", "rect": [100, 100, 200, 100]}))),
        "Picture Format" => {
            let data = deckcraft_engine::cmd::base64_encode(&picture_png(app));
            drop(app.run("insert.picture", json!({"data": data, "name": "p.png"})));
        }
        "Table Design" | "Layout" => drop(app.run("insert.table", json!({"rows": 2, "cols": 2}))),
        "Chart Design" => drop(app.run("insert.chart", json!({"type": "column"}))),
        "Playback" => {
            let data = deckcraft_engine::cmd::base64_encode(include_bytes!("../../media/tests/fixtures/tone.mp3"));
            drop(app.run("insert.audio", json!({"data": data, "name": "tone.mp3"})));
        }
        "Slide Master" => drop(app.run("view.slideMaster", json!({}))),
        _ => {}
    }
    drop(app.run("view.tab", json!({"tab": tab})));
}

const CONTEXTUAL: [&str; 7] = ["Shape Format", "Picture Format", "Table Design", "Layout", "Chart Design", "Playback", "Slide Master"];

const DIALOGS: [&str; 27] = [
    "table",
    "slideSize",
    "headerFooter",
    "hyperlink",
    "setupShow",
    "zoom",
    "comment",
    "altText",
    "outline",
    "renameSection",
    "renameShape",
    "renameLayout",
    "paragraph",
    "about",
    "preferences",
    "export",
    "symbol",
    "equation",
    "spelling",
    "accessibility",
    "language",
    "customShow",
    "chartData",
    "trim",
    "smartart",
    "close",
    "unknown",
];

const PANES: [&str; 6] = ["format", "background", "animation", "selection", "comments", "designer"];

/// Words (ASCII letters in a row) that are right to see in an Arabic interface: product, format and
/// technology names, units, key names and the like.
const NAMES: &[&str] = &[
    "DeckCraft",
    "ArtCraft",
    "PDF",
    "PNG",
    "JPEG",
    "JPG",
    "GIF",
    "PPTX",
    "PPSX",
    "POTX",
    "TXT",
    "MD",
    "Markdown",
    "PowerPoint",
    "Microsoft",
    "SmartArt",
    "WordArt",
    "GitHub",
    "Discord",
    "MIT",
    "Apache",
    "Rust",
    "OFL",
    "URL",
    "RRGGBB",
    "ISO",
    "pt",
    "px",
    "in",
    "mm",
    "cm",
    "MB",
    "KB",
    "GB",
    "Ctrl",
    "Shift",
    "Alt",
    "Cmd",
    "Esc",
    "Tab",
    "Enter",
    "Backspace",
    "Delete",
    "End",
    "Home",
    "Return",
    "Escape",
    "Space",
    "PageUp",
    "PageDown",
    "Del",
    "Ins",
    "Win",
    "Inter",
    "Aa",
    "Abc",
    "Mac",
    "Windows",
    "Linux",
    "AI",
    "OK",
    "Claude",
    "Codex",
    "Gemini",
    "GPT",
    "Anthropic",
    "OpenAI",
    "Google",
    "LibreOffice",
    "HTTP",
    "HTTPS",
    "www",
    "com",
    "txt",
    "pptx",
    "pdf",
    "png",
    "jpg",
    "jpeg",
    "gif",
    "deckcraft",
    "https",
    "mailto",
    "RTL",
    "LTR",
    "UTF",
    "TIFF",
    "BMP",
    "WEBP",
    "OR",
    "Fable",
    "Opus",
    "Sonnet",
    "Haiku",
    "Presenter",
    "abc",
    "ab",
    "unknown",
    "Qqq",
    "Zzz",
    "Sans",
    "Serif",
    "Deutsch",
    "Español",
    "Français",
    "Italiano",
    "Português",
    "MP",
    "MKV",
    "WEBM",
    "MOV",
    "FLAC",
    "OGG",
    "AAC",
    "WMA",
    "WAV",
    "OPUS",
    "ALAC",
    "AV",
    "VP",
    "HEVC",
    "MPEG",
    "SVG",
    "EMF",
    "WMF",
    "ICO",
    "ECMA",
    "DOCX",
    "XLSX",
    "XML",
    "JSON",
    "CSS",
    "HTML",
    "SF",
    "CT",
    "ID",
    "Wi",
    "Fi",
    "Dk",
    "Lt",
    "Tx",
    "Bg",
    "Hlink",
    "FolHlink",
    "A",
    "B",
    "C",
    "D",
    "E",
    "F",
];

/// Is `text` (drawn in the Arabic interface) free of English words that should have been translated?
fn only_names(text: &str, doc_text: &str, fonts: &HashSet<String>, title: &str) -> bool {
    let plain = text.trim();
    // The names of languages are written in themselves.
    if ["Deutsch", "Español", "Français", "Italiano", "Português", "English (United States)"].contains(&plain) {
        return true;
    }
    // A contributor's GitHub name, and what the engine answers a script (not translated).
    if plain.starts_with('@') || plain.contains("command `") || plain.contains("invalid parameters") {
        return true;
    }
    // The presentation's name, in the title bar and in sentences about it.
    if !title.is_empty() && plain.contains(title) {
        return only_names(&plain.replace(title, ""), doc_text, fonts, "");
    }
    // The presentation's own text and names, and installed fonts: not interface wording.
    if doc_text.contains(plain) || fonts.contains(plain) {
        return true;
    }
    // A file path, a version, a hex colour: nothing to translate.
    if plain.contains('\\') || plain.contains('/') || plain.starts_with('#') || plain.starts_with("http") {
        return true;
    }
    plain
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|w| w.len() >= 2)
        .all(|w| NAMES.contains(&w) || doc_text.contains(w) || fonts.contains(w) || fonts.iter().any(|f| f.contains(w)))
}

/// Every string the presentation holds as a value (its text, names and descriptions; not the names
/// of its fields).
fn document_text(app: &SlideApp) -> String {
    fn values(v: &serde_json::Value, out: &mut String) {
        match v {
            serde_json::Value::String(s) => {
                out.push_str(s);
                out.push('\n');
            }
            serde_json::Value::Array(a) => a.iter().for_each(|x| values(x, out)),
            serde_json::Value::Object(o) => o.values().for_each(|x| values(x, out)),
            _ => {}
        }
    }
    let mut out = String::new();
    if let Some(d) = app.session.active() {
        values(&serde_json::to_value(&*d.doc).unwrap_or_default(), &mut out);
    }
    out
}

/// Keeps what the audit found: English text that should have been translated, by where it was drawn.
#[derive(Default)]
struct Findings {
    leaks: BTreeMap<String, Vec<String>>,
}

impl Findings {
    fn record(&mut self, app: &SlideApp, state: &str, drawn: &[(String, Rect)]) {
        let doc_text = document_text(app);
        let title = app.session.active().map(|d| d.title()).unwrap_or_default();
        let fonts: HashSet<String> = deckcraft_fonts::FontDb::global().families().into_iter().collect();
        for (text, _) in drawn {
            if !only_names(text, &doc_text, &fonts, &title) {
                self.leaks.entry(text.clone()).or_default().push(state.to_string());
            }
        }
    }

    fn report(&self) -> String {
        let mut out = String::new();
        for (text, states) in &self.leaks {
            let mut states = states.clone();
            states.sort();
            states.dedup();
            out.push_str(&format!("{text:?}  [{}]\n", states.join(", ")));
        }
        out
    }
}

/// The text drawn in Arabic for `english`.
fn ar(english: &str) -> &str {
    i18n::arabic().get(english).map(String::as_str).unwrap_or_else(|| panic!("no Arabic entry for {english:?}"))
}

fn x_of(drawn: &[(String, Rect)], text: &str) -> f32 {
    drawn.iter().find(|(t, _)| t.trim() == text.trim()).map(|(_, r)| r.center().x).unwrap_or_else(|| panic!("{text:?} is not drawn"))
}

/// The interface is mirrored: reading starts at the right.
#[test]
fn the_arabic_interface_is_mirrored() {
    let ctx = Context::default();
    let mut app = app_in(&ctx, Language::Ar);
    let drawn = frames_at(&ctx, &mut app, 1700.0, Vec::new());
    assert!(crate::rtl::on());
    // Tabs run from the right: File, Home, Insert … and the Present button is at the far left.
    let (file, home, insert) = (
        x_of(&drawn, ar("File")),
        x_of(&drawn, i18n::tr_context("Home", "@ribbon:Home")),
        x_of(&drawn, i18n::tr_context("Insert", "@ribbon:Insert")),
    );
    assert!(file > home && home > insert && file > 1500.0, "tabs: {file} {home} {insert}");
    assert!(x_of(&drawn, ar("  Present  ")) < 400.0, "Present: {}", x_of(&drawn, ar("  Present  ")));
    // Ribbon groups follow: the first group (Paste) is at the right of the second (New Slide).
    assert!(x_of(&drawn, ar("Paste")) > x_of(&drawn, ar("Cut")) || x_of(&drawn, ar("Cut")) > 1000.0, "the clipboard group starts at the right");
    // The status bar starts at the right; its zoom is at the left.
    let slide = i18n::format("Slide {} of {}", &["1".into(), "1".into()]);
    assert!(x_of(&drawn, &slide) > 1500.0, "the slide counter is at the right");
    assert!(drawn.iter().any(|(t, r)| t.contains('%') && r.center().x < 500.0), "the zoom is at the left");
    // The slide thumbnails are on the right: their number is.
    assert!(drawn.iter().any(|(t, r)| t == "1" && r.center().x > 1400.0), "the thumbnail number is at the right");
    // The File tab: the side menu is on the right.
    app.open_backstage(crate::backstage::Page::Info);
    let drawn = frame(&ctx, &mut app);
    assert!(
        x_of(&drawn, ar("Info")) > 1500.0 || drawn.iter().any(|(t, r)| t.trim() == ar("Info") && r.center().x > 1500.0),
        "the File menu is at the right"
    );
    app.close_backstage();
    // A task pane is on the left (it ends the window).
    let _ = app.run("view.pane", json!({"pane": "format"}));
    let _ = app.run("shape.insert", json!({"preset": "rect", "rect": [100, 100, 200, 100]}));
    let drawn = frame(&ctx, &mut app);
    assert!(drawn.iter().any(|(t, r)| t == ar("Format Shape") && r.center().x < 600.0), "the task pane is at the left");
    i18n::set_current(Language::En);

    // English is as it was.
    let ctx = Context::default();
    let mut app = app_in(&ctx, Language::En);
    let drawn = frames_at(&ctx, &mut app, 1700.0, Vec::new());
    assert!(!crate::rtl::on());
    assert!(x_of(&drawn, "File") < x_of(&drawn, "Home") && x_of(&drawn, "Home") < x_of(&drawn, "Insert") && x_of(&drawn, "Insert") < 400.0);
    assert!(x_of(&drawn, "Present") > 1400.0, "Present stays at the right in English");
    assert!(drawn.iter().any(|(t, r)| t == "1" && r.center().x < 100.0), "the thumbnail number stays at the left in English");
}

/// Everything laid out in Arabic never panics and shows no English: every tab (wide and narrow),
/// File page, dialog, pane, view, menu and ribbon control.
#[test]
fn the_arabic_interface_never_panics_and_is_translated() {
    let mut found = Findings::default();
    let ctx = Context::default();
    let mut app = app_in(&ctx, Language::Ar);
    i18n::misses::take();

    // Ribbon tabs, wide and narrow.
    for tab in crate::ribbon::TABS {
        drop(app.run("view.tab", json!({"tab": tab})));
        for width in [2400.0, 1100.0, 420.0] {
            let drawn = frames_at(&ctx, &mut app, width, Vec::new());
            found.record(&app, &format!("tab {tab} at {width}"), &drawn);
        }
    }
    drop(app.run("view.tab", json!({"tab": "Home"})));
    // File pages.
    for page in ["home", "new", "open", "info", "saveAs", "print", "export", "account"] {
        drop(app.run("app.backstage", json!({"page": page})));
        for width in [1700.0, 500.0] {
            let drawn = frames_at(&ctx, &mut app, width, Vec::new());
            found.record(&app, &format!("File > {page} at {width}"), &drawn);
        }
        // The pages' drop-downs.
    }
    drop(app.run("app.backstage", json!({"on": false})));
    // Dialogs, with a presentation that has something in it.
    drop(app.run("insert.textBox", json!({"text": "Hello", "rect": [10, 10, 100, 40]})));
    drop(app.run("insert.table", json!({"rows": 2, "cols": 2})));
    drop(app.run("insert.chart", json!({"type": "column"})));
    for id in DIALOGS {
        drop(app.run("app.dialog", json!({"id": id})));
        let drawn = frame(&ctx, &mut app);
        found.record(&app, &format!("dialog {id}"), &drawn);
        app.dialog = None;
    }
    // The About window's other tabs.
    for tab in ["contributors", "models"] {
        let mut d = crate::dialogs::Dialog::new("about");
        d.fields.insert("tab".into(), tab.into());
        app.dialog = Some(d);
        let drawn = frame(&ctx, &mut app);
        found.record(&app, &format!("About > {tab}"), &drawn);
        app.dialog = None;
    }
    // Task panes and views.
    for pane in PANES {
        drop(app.run("view.pane", json!({"pane": pane})));
        for width in [1700.0, 600.0] {
            let drawn = frames_at(&ctx, &mut app, width, Vec::new());
            found.record(&app, &format!("pane {pane} at {width}"), &drawn);
        }
        for tab in ["fill", "effects", "size", "text"] {
            app.ui.format_tab = tab.into();
            let drawn = frame(&ctx, &mut app);
            found.record(&app, &format!("pane {pane} {tab}"), &drawn);
        }
    }
    drop(app.run("view.pane", json!({"pane": null})));
    for view in ["view.outline", "view.sorter", "view.notesPage", "view.normal"] {
        drop(app.run(view, json!({})));
        let drawn = frame(&ctx, &mut app);
        found.record(&app, view, &drawn);
    }
    // The command palette, with a search.
    app.palette = Some((String::new(), 0));
    let drawn = frame(&ctx, &mut app);
    found.record(&app, "command palette", &drawn);
    app.palette = Some(("ش".into(), 0));
    let drawn = frame(&ctx, &mut app);
    found.record(&app, "command palette search", &drawn);
    app.palette = None;
    // A right-click on the slide and on a thumbnail.
    for at in [Pos2::new(900.0, 400.0), Pos2::new(1600.0, 250.0)] {
        let drawn = frames_at(&ctx, &mut app, 1700.0, click(at, egui::PointerButton::Secondary));
        found.record(&app, &format!("context menu at {at:?}"), &drawn);
        frames_at(&ctx, &mut app, 1700.0, escape());
    }
    // The slide show and its controls.
    app.start_show(0, false);
    let drawn = frames_at(&ctx, &mut app, 1700.0, vec![vec![Event::PointerMoved(Pos2::new(600.0, 400.0))]]);
    found.record(&app, "slide show", &drawn);
    app.show = None;
    app.start_show(0, true);
    let drawn = frame(&ctx, &mut app);
    found.record(&app, "reading view", &drawn);
    app.show = None;
    // The missing-fonts notice.
    app.toast = Some(crate::toast::Toast {
        text: crate::toast::missing_fonts_message(&["Qqq Sans".into(), "Zzz Serif".into()]).unwrap(),
        at: crate::now_ms(),
    });
    let drawn = frame(&ctx, &mut app);
    found.record(&app, "toast", &drawn);
    app.toast = None;

    // Contextual tabs and the controls of every tab.
    let tabs: Vec<&str> = crate::ribbon::TABS.iter().chain(&CONTEXTUAL).copied().collect();
    for tab in &tabs {
        let open = |tab: &str| {
            let ctx = Context::default();
            let mut app = app_in(&ctx, Language::Ar);
            prepare(&mut app, tab);
            let drawn = frame(&ctx, &mut app);
            (ctx, app, drawn)
        };
        let (ctx, mut app, drawn) = open(tab);
        found.record(&app, &format!("tab {tab}"), &drawn);
        let controls = ribbon_controls(&ctx);
        // Hover each control (its tooltip), click it (its menu or dialog), and put things back.
        for at in controls {
            let hovered = frames_at(&ctx, &mut app, 1700.0, vec![vec![Event::PointerMoved(at)]; 2]);
            found.record(&app, &format!("{tab} > hover"), &hovered);
            let clicked = frames_at(&ctx, &mut app, 1700.0, click(at, egui::PointerButton::Primary));
            found.record(&app, &format!("{tab} > click"), &clicked);
            frames_at(&ctx, &mut app, 1700.0, escape());
            app.dialog = None;
            app.show = None;
            app.backstage = None;
            app.palette = None;
            if app.ui.tab != *tab {
                drop(app.run("view.tab", json!({"tab": tab})));
            }
        }
    }
    let misses = i18n::misses::take();
    i18n::set_current(Language::En);
    assert!(
        found.leaks.is_empty(),
        "{} interface texts are drawn in English:\n{}\nlooked up without an Arabic entry: {misses:#?}",
        found.leaks.len(),
        found.report()
    );
}

/// Screenshots of the Arabic interface in many states, for looking at (the PNGs stay outside the
/// repository). `DECKCRAFT_I18N_SHOTS=<folder> cargo test -p deckcraft-ui-egui arabic_screenshots
/// -- --ignored`.
#[test]
#[ignore = "writes review screenshots; run explicitly with DECKCRAFT_I18N_SHOTS"]
fn arabic_screenshots() {
    use egui_kittest::Harness;

    let dir = std::env::var("DECKCRAFT_I18N_SHOTS").unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let language = match std::env::var("DECKCRAFT_I18N_LANG").as_deref() {
        Ok("en") => Language::En,
        _ => Language::Ar,
    };
    let code = if language == Language::En { "en" } else { "ar" };
    let new_harness = |prepare: &dyn Fn(&mut SlideApp)| {
        let mut app = SlideApp::new(Session::with_new(), Services::default());
        app.ui.interface_language = language;
        prepare(&mut app);
        let mut h = Harness::builder().with_size(vec2(1440.0, 900.0)).build_ui_state(
            |ui, app: &mut SlideApp| {
                app.logic(ui.ctx());
                app.ui(ui);
            },
            app,
        );
        h.run_steps(5);
        h
    };
    let save = |h: &mut Harness<'_, SlideApp>, name: &str| {
        h.run_steps(3);
        h.render().unwrap().save(format!("{dir}/{code}-{name}.png")).unwrap();
    };
    let click_at = |h: &mut Harness<'_, SlideApp>, at: Pos2| {
        h.event(Event::PointerMoved(at));
        h.run_steps(2);
        for pressed in [true, false] {
            h.event(Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() });
            h.run_steps(2);
        }
    };
    let deck = |app: &mut SlideApp| {
        drop(app.run("insert.textBox", json!({"text": "عرض تقديمي تجريبي — Quarterly Report", "rect": [60, 40, 500, 60]})));
        drop(app.run("insert.table", json!({"rows": 3, "cols": 3})));
        drop(app.run("slide.new", json!({})));
        drop(app.run("slide.new", json!({})));
        drop(app.run("slide.go", json!({"index": 0})));
    };
    // Tabs.
    for tab in ["Home", "Insert", "Draw", "Design", "Transitions", "Animations", "Slide Show", "Record", "Review", "View"] {
        let mut h = new_harness(&|app| {
            deck(app);
            app.ui.tab = tab.to_string();
        });
        save(&mut h, &format!("tab-{}", tab.replace(' ', "-").to_lowercase()));
    }
    // Contextual tabs, with the Format pane.
    for tab in ["Shape Format", "Picture Format", "Table Design", "Layout", "Chart Design", "Playback", "Slide Master"] {
        let mut h = new_harness(&|app| {
            prepare(app, tab);
            if tab == "Shape Format" {
                drop(app.run("view.pane", json!({"pane": "format"})));
            }
        });
        save(&mut h, &format!("ctx-{}", tab.replace(' ', "-").to_lowercase()));
    }
    // File pages.
    for page in ["home", "new", "open", "info", "saveAs", "print", "export", "account"] {
        let mut h = new_harness(&|app| {
            deck(app);
            drop(app.run("app.backstage", json!({"page": page})));
        });
        save(&mut h, &format!("file-{page}"));
    }
    // Dialogs.
    for id in [
        "table",
        "slideSize",
        "headerFooter",
        "hyperlink",
        "setupShow",
        "close",
        "symbol",
        "customShow",
        "about",
        "accessibility",
        "paragraph",
        "comment",
        "export",
        "language",
        "smartart",
    ] {
        let mut h = new_harness(&|app| {
            deck(app);
            drop(app.run("app.dialog", json!({"id": id})));
        });
        save(&mut h, &format!("dialog-{id}"));
    }
    // Panes and views.
    for pane in PANES {
        let mut h = new_harness(&|app| {
            deck(app);
            drop(app.run("view.pane", json!({"pane": pane})));
        });
        save(&mut h, &format!("pane-{pane}"));
    }
    for view in ["view.outline", "view.sorter", "view.notesPage"] {
        let mut h = new_harness(&|app| {
            deck(app);
            drop(app.run(view, json!({})));
        });
        save(&mut h, &view.replace("view.", "view-"));
    }
    // The command palette and the slide show.
    let mut h = new_harness(&|app| {
        deck(app);
        app.palette = Some((String::new(), 0));
    });
    save(&mut h, "palette");
    let mut h = new_harness(&|app| {
        deck(app);
        app.start_show(0, false);
    });
    h.event(Event::PointerMoved(Pos2::new(700.0, 500.0)));
    save(&mut h, "show");
    // Menus opened from the ribbon: click a control by where its label is drawn.
    let label_at = |h: &Harness<'_, SlideApp>, english: &str| -> Pos2 {
        let text = i18n::tr(english);
        let mut drawn = Vec::new();
        for s in &h.output().shapes {
            collect(&s.shape, &mut drawn);
        }
        let wanted: Vec<&str> = text.split('\n').collect();
        drawn.iter().find(|(t, _)| wanted.contains(&t.trim())).map(|(_, r)| r.center()).unwrap_or_else(|| panic!("{english:?} is not drawn"))
    };
    for (name, tab, label, dy) in [
        ("menu-paste", "Home", "Paste", 0.0),
        ("menu-new-slide", "Home", "New\nSlide", 0.0),
        ("menu-shapes", "Home", "Shapes", -15.0),
        ("menu-arrange", "Home", "Arrange", -15.0),
        ("menu-quick-styles", "Home", "Quick\nStyles", -15.0),
        ("menu-shape-fill", "Home", "Shape Fill", 0.0),
        ("menu-background-styles", "Design", "Background\nStyles", 0.0),
        ("menu-colors", "Design", "Colors", 0.0),
        ("menu-transition", "Transitions", "Effect\nOptions", -15.0),
        ("menu-table", "Insert", "Table", -15.0),
    ] {
        let mut h = new_harness(&|app| {
            deck(app);
            app.ui.tab = tab.to_string();
            if label == "Shape Fill" || label == "Arrange" || label == "Quick\nStyles" {
                drop(app.run("edit.selectAll", json!({})));
            }
        });
        let at = label_at(&h, label) + vec2(0.0, dy);
        click_at(&mut h, at);
        save(&mut h, name);
    }
    // A right-click on a slide's thumbnail and on the slide.
    for (name, at) in [("context-thumb", Pos2::new(1300.0, 230.0)), ("context-canvas", Pos2::new(700.0, 500.0))] {
        let mut h = new_harness(&deck);
        h.event(Event::PointerMoved(at));
        h.run_steps(2);
        for pressed in [true, false] {
            h.event(Event::PointerButton { pos: at, button: egui::PointerButton::Secondary, pressed, modifiers: Default::default() });
            h.run_steps(2);
        }
        save(&mut h, name);
    }
    i18n::set_current(Language::En);
}
