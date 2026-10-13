//! DeckCraft's egui frontend: a PowerPoint-style UI over `deckcraft-engine`.
//!
//! The UI is thin: every action goes through [`SlideApp::run`], which handles UI commands
//! (views, panes, zoom) here and sends everything else to the engine. The ribbon, menus,
//! shortcuts, the command palette and the control channel all share that entry point.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod canvas;
pub mod control;
pub mod credits;
pub mod dialogs;
pub mod fillui;
pub mod icons;
pub mod media;
pub mod menus;
pub mod panes;
pub mod ribbon;
pub mod show;
pub mod sorter;
pub mod status;
pub mod textures;
pub mod theme;
pub mod thumbs;
pub mod widgets;

use std::sync::mpsc::{Receiver, Sender};

use deckcraft_engine::{Mods, Session, UiRequest};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub use control::{ControlRequest, ControlResponse};

pub type ReadFn = Box<dyn Fn(&str) -> Result<Vec<u8>, String>>;
pub type WriteFn = Box<dyn FnMut(&str, &[u8]) -> Result<(), String>>;
pub type PickFn = Box<dyn FnMut(&str) -> Option<String>>;
pub type OpenAsyncFn = Box<dyn FnMut(&str)>;
pub type DownloadFn = Box<dyn FnMut(&str, &[u8])>;
/// Files `(name, bytes)` delivered asynchronously by the host (web file picker, dropped files).
pub type Inbox = std::sync::Arc<std::sync::Mutex<Vec<(String, Vec<u8>)>>>;

/// Platform services injected by the host (desktop or web).
#[derive(Default)]
pub struct Services {
    /// Open-file dialog for a purpose (`open`, `picture`, `audio`, `video`) → path.
    pub pick_open: Option<PickFn>,
    /// Save dialog with a suggested name → path.
    pub pick_save: Option<PickFn>,
    pub read: Option<ReadFn>,
    pub write: Option<WriteFn>,
    /// Asynchronous open dialog (web): the chosen file arrives through `inbox`.
    pub open_async: Option<OpenAsyncFn>,
    /// Hand bytes to the user as a named file (browser download).
    pub download: Option<DownloadFn>,
    pub inbox: Option<Inbox>,
    /// Image on the system clipboard (PNG bytes), when the host can read one.
    pub clipboard_image: Option<Box<dyn FnMut() -> Option<Vec<u8>>>>,
    /// Audio output for media playback (cpal on desktop); without one media plays silently.
    pub audio_out: Option<Box<dyn deckcraft_media::AudioOut>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ViewMode {
    #[default]
    Normal,
    Outline,
    Sorter,
    NotesPage,
    Reading,
}

/// Persisted UI state.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UiState {
    pub brightness: theme::Brightness,
    pub tab: String,
    pub ribbon_collapsed: bool,
    pub view: ViewMode,
    pub notes: bool,
    pub notes_height: f32,
    pub thumbs_width: f32,
    /// Open task pane: `format`, `animation`, `selection`, `comments`, `designer`, `background`.
    pub pane: Option<String>,
    pub pane_width: f32,
    pub ruler: bool,
    pub gridlines: bool,
    pub guides: bool,
    pub smart_guides: bool,
    /// None = fit to window.
    pub zoom: Option<f32>,
    pub sorter_zoom: f32,
    pub recent: Vec<String>,
    pub presenter_view: bool,
    pub ui_scale: f32,
    /// Format Shape pane: tab (`fill`, `effects`, `size`, `picture`, `text`).
    pub format_tab: String,
    pub grayscale: bool,
}

impl Default for UiState {
    fn default() -> Self {
        UiState {
            brightness: theme::Brightness::Light,
            tab: "Home".into(),
            ribbon_collapsed: false,
            view: ViewMode::Normal,
            notes: true,
            notes_height: 52.0,
            thumbs_width: 236.0,
            pane: None,
            pane_width: 300.0,
            ruler: false,
            gridlines: false,
            guides: false,
            smart_guides: true,
            zoom: None,
            sorter_zoom: 1.0,
            recent: vec![],
            presenter_view: true,
            ui_scale: 1.0,
            format_tab: "fill".into(),
            grayscale: false,
        }
    }
}

/// UI-only commands (id, label, shortcut, params).
pub const UI_COMMANDS: &[(&str, &str, Option<&str>, &str)] = &[
    ("view.normal", "Normal", Some("Cmd+1"), "{}"),
    ("view.outline", "Outline View", Some("Cmd+4"), "{}"),
    ("view.sorter", "Slide Sorter", Some("Cmd+2"), "{}"),
    ("view.notesPage", "Notes Page", Some("Cmd+3"), "{}"),
    ("view.reading", "Reading View", None, "{}"),
    ("view.notes", "Notes", None, "{on?: bool}"),
    ("view.ruler", "Ruler", None, "{on?: bool}"),
    ("view.gridlines", "Gridlines", Some("Shift+F9"), "{on?: bool}"),
    ("view.guides", "Guides", Some("Alt+F9"), "{on?: bool}"),
    ("view.zoomIn", "Zoom In", Some("Cmd+Plus"), "{}"),
    ("view.zoomOut", "Zoom Out", Some("Cmd+-"), "{}"),
    ("view.zoom", "Zoom…", None, "{percent}"),
    ("view.fit", "Fit to Window", Some("Cmd+Alt+O"), "{}"),
    ("view.pane", "Task Pane", None, "{pane: format|animation|selection|comments|designer|background|null}"),
    ("view.tab", "Ribbon Tab", None, "{tab}"),
    ("view.collapseRibbon", "Collapse the Ribbon", Some("Cmd+F1"), "{}"),
    ("view.dark", "Dark Mode", None, "{on?: bool}"),
    ("view.grayscale", "Grayscale", None, "{on?: bool}"),
    ("show.start", "Slide Show", Some("F5"), "{from?: index}"),
    ("show.presenter", "Presenter View", None, "{on?: bool}"),
    ("show.end", "End Show", None, "{}"),
    ("app.home", "Home", None, "{on?}"),
    ("app.about", "About DeckCraft", None, "{}"),
    ("app.palette", "Command Palette", Some("Cmd+Shift+P"), "{}"),
    ("app.preferences", "Preferences…", Some("Cmd+,"), "{}"),
    ("app.openDialog", "Open…", None, "{purpose?}"),
    ("app.insertPictureDialog", "Picture from File…", None, "{}"),
    ("app.insertAudioDialog", "Audio from File…", None, "{}"),
    ("app.insertVideoDialog", "Video from File…", None, "{}"),
    ("app.saveAsDialog", "Save As…", None, "{}"),
    ("app.exportDialog", "Export…", None, "{}"),
    ("app.dialog", "Open Dialog", None, "{id}"),
    ("app.links", "Community Links", None, "{}"),
];

pub struct Perf {
    pub frame_ms: f64,
    pub render_ms: f64,
    pub fps: f64,
}

pub struct SlideApp {
    pub session: Session,
    pub services: Services,
    pub ui: UiState,
    pub textures: textures::Textures,
    pub control_rx: Option<Receiver<ControlRequest>>,
    pub dialog: Option<dialogs::Dialog>,
    pub show: Option<show::Show>,
    /// Audio/video playback (editor control bar, slide show, `media.*` commands).
    pub media: media::MediaHost,
    pub status: Option<(String, f64)>,
    pub perf: Perf,
    /// Synthetic input events (control channel `ui.click` etc.), injected one per frame.
    pub synthetic: Vec<egui::Event>,
    /// Canvas screen rect and points-per-slide-point scale (for control & tests).
    pub canvas_rect: Option<egui::Rect>,
    pub canvas_scale: f32,
    pub slide_rect: Option<egui::Rect>,
    pub palette: Option<(String, usize)>,
    /// Integrated title bar (macOS traffic lights over our chrome).
    pub integrated_titlebar: bool,
    /// The start screen is shown over the open presentations (Home), with the document count
    /// and active index when it was opened: opening or creating a presentation leaves it.
    pub home: Option<(usize, Option<usize>)>,
    pending_urls: Vec<String>,
    shot_token: u64,
    queued_shots: Vec<(u64, f64, u32)>,
    pending_shots: Vec<(u64, Option<String>, Sender<ControlResponse>, f64)>,
    styled: bool,
    restyle: bool,
    fonts_ready: bool,
    last_time: f64,
    /// When AutoRecover data was last written (seconds, egui time).
    pub last_recovery: f64,
    /// Quit was confirmed (changes saved or discarded): let the window close.
    pub quit_confirmed: bool,
    pub(crate) notes_buf: (u64, usize, String),
    /// Thumbnail pane drag (slide index, current drop index).
    pub(crate) thumb_drag: Option<(usize, usize)>,
    /// F6 moved the keyboard to the thumbnail pane (arrows change slides; it shows a focus ring).
    pub(crate) thumbs_focused: bool,
}

impl SlideApp {
    pub fn new(session: Session, mut services: Services) -> Self {
        let audio_out = services.audio_out.take();
        SlideApp {
            session,
            services,
            ui: UiState::default(),
            textures: textures::Textures::default(),
            control_rx: None,
            dialog: None,
            show: None,
            media: media::MediaHost::new(audio_out),
            status: None,
            perf: Perf { frame_ms: 0.0, render_ms: 0.0, fps: 0.0 },
            synthetic: vec![],
            canvas_rect: None,
            canvas_scale: 1.0,
            slide_rect: None,
            palette: None,
            integrated_titlebar: false,
            home: None,
            pending_urls: vec![],
            shot_token: 0,
            queued_shots: vec![],
            pending_shots: vec![],
            styled: false,
            restyle: true,
            fonts_ready: false,
            last_time: 0.0,
            last_recovery: 0.0,
            quit_confirmed: false,
            notes_buf: (0, usize::MAX, String::new()),
            thumb_drag: None,
            thumbs_focused: false,
        }
    }

    pub fn with_control(mut self, rx: Receiver<ControlRequest>) -> Self {
        self.control_rx = Some(rx);
        self
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), now_ms()));
    }

    /// Run a command by id (UI commands here, everything else in the engine). Errors show in the
    /// status bar and are returned.
    pub fn run(&mut self, id: &str, params: Value) -> Result<Value, String> {
        let r = if UI_COMMANDS.iter().any(|c| c.0 == id) {
            self.run_ui(id, &params)
        } else {
            self.session.execute(id, &params).map_err(|e| e.to_string())
        };
        if let Err(e) = &r {
            self.set_status(e.clone());
        }
        self.drain_requests();
        r
    }

    fn docs_key(&self) -> (usize, Option<usize>) {
        (self.session.documents().len(), self.session.active_index())
    }

    /// Home is open over a presentation (not just the start screen with nothing open).
    pub fn home_open(&self) -> bool {
        self.home.is_some() && self.session.active().is_some()
    }

    fn flag(&mut self, params: &Value, cur: bool) -> bool {
        params.get("on").and_then(Value::as_bool).unwrap_or(!cur)
    }

    fn run_ui(&mut self, id: &str, p: &Value) -> Result<Value, String> {
        match id {
            "view.normal" => self.ui.view = ViewMode::Normal,
            "view.outline" => self.ui.view = ViewMode::Outline,
            "view.sorter" => self.ui.view = ViewMode::Sorter,
            "view.notesPage" => self.ui.view = ViewMode::NotesPage,
            "view.reading" => self.start_show(self.session.active().map(|d| d.selection.slide).unwrap_or(0), true),
            "view.notes" => self.ui.notes = self.flag(p, self.ui.notes),
            "view.ruler" => self.ui.ruler = self.flag(p, self.ui.ruler),
            "view.gridlines" => self.ui.gridlines = self.flag(p, self.ui.gridlines),
            "view.guides" => self.ui.guides = self.flag(p, self.ui.guides),
            "view.zoomIn" | "view.zoomOut" => {
                let cur = self.ui.zoom.unwrap_or(self.canvas_scale);
                let steps = [0.1, 0.25, 0.33, 0.5, 0.66, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0];
                let next = if id == "view.zoomIn" {
                    steps.iter().copied().find(|s| *s > cur + 0.01).unwrap_or(4.0)
                } else {
                    steps.iter().rev().copied().find(|s| *s < cur - 0.01).unwrap_or(0.1)
                };
                self.ui.zoom = Some(next);
            }
            "view.zoom" => {
                let pct = p.get("percent").and_then(Value::as_f64).ok_or("missing `percent`")?;
                self.ui.zoom = Some((pct as f32 / 100.0).clamp(0.1, 4.0));
            }
            "view.fit" => self.ui.zoom = None,
            "view.pane" => {
                let pane = p.get("pane").and_then(Value::as_str).map(String::from);
                self.ui.pane =
                    if pane.is_some() && pane == self.ui.pane && p.get("toggle").and_then(Value::as_bool).unwrap_or(false) { None } else { pane };
                if let Some(tab) = p.get("tab").and_then(Value::as_str) {
                    self.ui.format_tab = tab.to_string();
                }
            }
            "view.tab" => {
                let tab = p.get("tab").and_then(Value::as_str).ok_or("missing `tab`")?;
                self.ui.tab = tab.to_string();
                self.ui.ribbon_collapsed = false;
            }
            "view.collapseRibbon" => self.ui.ribbon_collapsed = !self.ui.ribbon_collapsed,
            "view.dark" => {
                let on = self.flag(p, self.ui.brightness == theme::Brightness::Dark);
                self.ui.brightness = if on { theme::Brightness::Dark } else { theme::Brightness::Light };
                self.restyle = true;
            }
            "view.grayscale" => self.ui.grayscale = self.flag(p, self.ui.grayscale),
            "show.start" => {
                let from = p.get("from").and_then(Value::as_u64).map(|v| v as usize).unwrap_or(0);
                self.start_show(from, false);
            }
            "show.presenter" => self.ui.presenter_view = self.flag(p, self.ui.presenter_view),
            "show.end" => {
                self.show = None;
                self.media.stop_owned(media::Owner::Show);
            }
            "app.home" => {
                let on = self.flag(p, self.home.is_some());
                self.home = (on && self.session.active().is_some()).then(|| self.docs_key());
            }
            "app.about" => self.dialog = Some(dialogs::Dialog::new("about")),
            "app.palette" => self.palette = Some((String::new(), 0)),
            "app.preferences" => self.dialog = Some(dialogs::Dialog::new("preferences")),
            "app.openDialog" => self.pick_and_open(),
            "app.insertPictureDialog" => self.pick_and_insert("picture", "insert.picture"),
            "app.insertAudioDialog" => self.pick_and_insert("audio", "insert.audio"),
            "app.insertVideoDialog" => self.pick_and_insert("video", "insert.video"),
            "app.saveAsDialog" => self.save_as_dialog("deckcraft"),
            "app.exportDialog" => self.dialog = Some(dialogs::Dialog::new("export")),
            "app.dialog" => {
                let d = p.get("id").and_then(Value::as_str).ok_or("missing `id`")?;
                self.dialog = Some(dialogs::Dialog::new(d));
            }
            "app.links" => {
                use deckcraft_engine::links::*;
                return Ok(json!({"discord": DISCORD, "website": WEBSITE, "apps": APPS, "appPage": APP_PAGE, "github": GITHUB}));
            }
            _ => return Err(format!("unknown UI command `{id}`")),
        }
        Ok(Value::Null)
    }

    /// Alt+F5 / Option+Return: the show from the current slide, in Presenter View.
    pub fn start_presenter(&mut self) {
        self.media.stop_owned(media::Owner::Editor);
        if let Some(d) = self.session.active() {
            self.show = Some(show::Show::new(&d.doc, d.selection.slide, false, true));
        }
    }

    /// F6 / Shift+F6 (Normal view): move the keyboard between the slide, the notes and the
    /// thumbnails, in that order.
    pub(crate) fn cycle_pane(&mut self, ctx: &egui::Context, back: bool) {
        #[derive(Clone, Copy, PartialEq)]
        enum Pane {
            Slide,
            Notes,
            Thumbs,
        }
        let notes_id = egui::Id::new("notes_edit");
        let slides = self.session.active().is_some_and(|d| d.selection.target == deckcraft_engine::Target::Slides);
        let normal = matches!(self.ui.view, ViewMode::Normal | ViewMode::Outline);
        if !normal {
            return;
        }
        let mut order = vec![Pane::Slide];
        if self.ui.notes && slides {
            order.push(Pane::Notes);
        }
        order.push(Pane::Thumbs);
        let cur = if ctx.memory(|m| m.has_focus(notes_id)) {
            Pane::Notes
        } else if self.thumbs_focused {
            Pane::Thumbs
        } else {
            Pane::Slide
        };
        let n = order.len();
        let i = order.iter().position(|p| *p == cur).unwrap_or(0);
        let next = order.get(if back { (i + n - 1) % n } else { (i + 1) % n }).copied().unwrap_or(Pane::Slide);
        ctx.memory_mut(|m| {
            if let Some(f) = m.focused() {
                m.surrender_focus(f);
            }
        });
        self.thumbs_focused = false;
        match next {
            Pane::Notes => ctx.memory_mut(|m| m.request_focus(notes_id)),
            Pane::Thumbs => {
                // The thumbnail pane works on whole slides: arrows change slides, Ctrl/Cmd+Up/Down move them.
                self.thumbs_focused = true;
                if self.session.active().is_some() {
                    let _ = self.session.execute("edit.deselect", &json!({}));
                }
            }
            Pane::Slide => {}
        }
    }

    pub fn start_show(&mut self, from: usize, reading: bool) {
        self.media.stop_owned(media::Owner::Editor);
        if let Some(d) = self.session.active() {
            self.show = Some(show::Show::new(&d.doc, from, reading, self.ui.presenter_view && !reading));
        }
    }

    fn pick_and_open(&mut self) {
        if let Some(pick) = self.services.pick_open.as_mut() {
            if let Some(path) = pick("open") {
                let _ = self.open_path(&path);
            }
        } else if let Some(f) = self.services.open_async.as_mut() {
            f("open");
        }
    }

    pub fn open_path(&mut self, path: &str) -> Result<(), String> {
        self.session.execute("file.open", &json!({"path": path})).map_err(|e| e.to_string())?;
        self.ui.recent.retain(|p| p != path);
        self.ui.recent.insert(0, path.to_string());
        self.ui.recent.truncate(12);
        self.ui.view = ViewMode::Normal;
        Ok(())
    }

    fn pick_and_insert(&mut self, purpose: &str, cmd: &str) {
        if let Some(pick) = self.services.pick_open.as_mut() {
            if let Some(path) = pick(purpose) {
                let r = match self.services.read.as_ref().map(|r| r(&path)) {
                    Some(Ok(bytes)) => {
                        let name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        self.session.execute(cmd, &json!({"name": name, "data": deckcraft_engine::cmd::base64_encode(&bytes)}))
                    }
                    _ => self.session.execute(cmd, &json!({"path": path})),
                };
                if let Err(e) = r {
                    self.set_status(e.to_string());
                }
            }
        } else if let Some(f) = self.services.open_async.as_mut() {
            f(purpose);
        }
    }

    pub fn save_as_dialog(&mut self, format: &str) {
        let name = self.session.active().map(|d| d.title()).unwrap_or_else(|| "Presentation".into());
        let ext = if format == "pptx" { "pptx" } else { "deckcraft" };
        let suggested = format!("{name}.{ext}");
        if let Some(pick) = self.services.pick_save.as_mut() {
            if let Some(path) = pick(&suggested) {
                match self.session.execute("file.saveAs", &json!({"path": path})) {
                    Ok(_) => {
                        self.set_status(format!("Saved {path}"));
                        self.ui.recent.retain(|p| *p != path);
                        self.ui.recent.insert(0, path);
                    }
                    Err(e) => self.set_status(e.to_string()),
                }
            }
        } else if let Some(dl) = self.services.download.as_mut()
            && let Some(d) = self.session.active()
        {
            match deckcraft_engine::cmd::file::save_bytes(&d.doc, ext) {
                Ok(bytes) => dl(&suggested, &bytes),
                Err(e) => self.set_status(e.to_string()),
            }
        }
    }

    /// Save every presentation with unsaved changes (asking for a location where needed).
    /// Returns false if one wasn't saved (cancelled or failed).
    pub fn save_all(&mut self) -> bool {
        let dirty: Vec<usize> = self.session.documents().iter().enumerate().filter(|(_, d)| d.is_dirty()).map(|(i, _)| i).collect();
        for i in dirty {
            self.session.set_active(i);
            self.save();
            if self.session.documents().get(i).is_some_and(|d| d.is_dirty()) {
                return false;
            }
        }
        true
    }

    pub fn save(&mut self) {
        if self.session.active().and_then(|d| d.path.clone()).is_some() {
            match self.session.execute("file.save", &json!({})) {
                Ok(_) => self.set_status("Saved"),
                Err(e) => self.set_status(e.to_string()),
            }
        } else {
            self.save_as_dialog("deckcraft");
        }
    }

    /// Act on requests commands left for the UI.
    fn drain_requests(&mut self) {
        for r in std::mem::take(&mut self.session.ui_requests) {
            match r {
                UiRequest::Dialog { id, params } => {
                    let mut d = dialogs::Dialog::new(&id);
                    d.params = params;
                    self.dialog = Some(d);
                }
                UiRequest::Pane { id } => self.ui.pane = Some(id),
                UiRequest::StartShow { from } => self.start_show(from, false),
                UiRequest::Message { text } => self.set_status(text),
                UiRequest::PickFile { purpose } => {
                    if purpose == "saveAs" {
                        self.save_as_dialog("deckcraft");
                    }
                }
                UiRequest::Media { action, shape, ms } => {
                    let owner = if self.show.is_some() { media::Owner::Show } else { media::Owner::Editor };
                    if let Some(doc) = self.session.active().map(|d| d.doc.clone()) {
                        self.media.request(&doc, &action, shape, ms, owner);
                    }
                }
            }
        }
    }

    fn drain_inbox(&mut self) {
        let Some(inbox) = self.services.inbox.clone() else { return };
        let files: Vec<(String, Vec<u8>)> = inbox.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default();
        for (name, bytes) in files {
            self.open_or_insert_bytes(&name, &bytes);
        }
    }

    /// A dropped or picked file: presentations open, media is inserted.
    pub fn open_or_insert_bytes(&mut self, name: &str, bytes: &[u8]) {
        let lower = name.to_ascii_lowercase();
        let data = deckcraft_engine::cmd::base64_encode(bytes);
        let ct = deckcraft_engine::cmd::insert::content_type(name, bytes);
        let cmd = if lower.ends_with(".deckcraft")
            || lower.ends_with(".slidecraft")
            || lower.ends_with(".pptx")
            || lower.ends_with(".potx")
            || lower.ends_with(".ppsx")
        {
            "file.openBytes"
        } else if ct.starts_with("image/") {
            "insert.picture"
        } else if ct.starts_with("audio/") {
            "insert.audio"
        } else if ct.starts_with("video/") {
            "insert.video"
        } else if lower.ends_with(".txt") || lower.ends_with(".md") {
            "slide.fromOutline"
        } else {
            self.set_status(format!("{name}: DeckCraft can't open this kind of file"));
            return;
        };
        let p = if cmd == "slide.fromOutline" { json!({"text": String::from_utf8_lossy(bytes)}) } else { json!({"name": name, "data": data}) };
        if self.session.active().is_none() && cmd != "file.openBytes" {
            let _ = self.session.execute("file.new", &json!({}));
        }
        if let Err(e) = self.session.execute(cmd, &p) {
            self.set_status(e.to_string());
        }
    }

    /// Per-frame logic before layout: styles, control requests, screenshots, shortcuts.
    pub fn logic(&mut self, ctx: &egui::Context) {
        let scale = self.ui.ui_scale.clamp(0.5, 3.0);
        if (ctx.zoom_factor() - scale).abs() > 1e-3 {
            ctx.set_zoom_factor(scale);
        }
        if !self.styled {
            theme::install_fonts(ctx);
            self.styled = true;
            self.restyle = true;
        } else {
            self.fonts_ready = true;
        }
        if self.restyle {
            theme::apply(ctx, &theme::Tokens::for_brightness(self.ui.brightness));
            self.restyle = false;
        }
        let now = ctx.input(|i| i.time);
        let dt = now - self.last_time;
        if dt > 0.0 {
            self.perf.fps = self.perf.fps * 0.9 + (1.0 / dt).min(240.0) * 0.1;
        }
        self.last_time = now;
        self.drain_control(ctx);
        if self.fonts_ready {
            self.drain_inbox();
        }
        if !self.synthetic.is_empty() {
            ctx.request_repaint();
        }
        self.collect_screenshots(ctx);
        self.issue_screenshots(ctx);
        // Closing the window with unsaved changes asks first.
        if ctx.input(|i| i.viewport().close_requested()) && !self.quit_confirmed && self.session.documents().iter().any(|d| d.is_dirty()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.show = None;
            if self.dialog.as_ref().is_none_or(|d| d.id != "quit") {
                self.dialog = Some(dialogs::Dialog::new("quit"));
            }
        }
        // AutoRecover: unsaved presentations are written to the recovery folder periodically.
        if self.session.recovery_dir.is_some() && now - self.last_recovery > (self.session.prefs.recovery_minutes * 60.0).max(10.0) {
            self.last_recovery = now;
            if self.session.documents().iter().any(|d| d.is_dirty())
                && let Err(e) = self.session.execute("file.recovery.save", &json!({}))
            {
                self.set_status(format!("Couldn't save AutoRecover information: {e}"));
            }
        }
        // On the web, dropped files can only be read asynchronously: the host reads them and
        // delivers them through `Services::inbox`.
        #[cfg(not(target_arch = "wasm32"))]
        for f in ctx.input(|i| i.raw.dropped_files.clone()) {
            let name = f.path().file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "dropped file".into());
            match f.bytes() {
                Ok(bytes) => self.open_or_insert_bytes(&name, &bytes),
                Err(e) => self.set_status(e),
            }
        }
    }

    /// Inject synthetic events (one pointer event per frame).
    pub fn raw_input_hook(&mut self, raw: &mut egui::RawInput) {
        if self.synthetic.is_empty() {
            return;
        }
        let n = match self.synthetic.first() {
            Some(egui::Event::PointerMoved(_) | egui::Event::PointerButton { .. }) => 1,
            _ => self.synthetic.iter().position(|e| matches!(e, egui::Event::Key { pressed: false, .. })).map_or(self.synthetic.len(), |i| i + 1),
        };
        if let Some(egui::Event::PointerMoved(p) | egui::Event::PointerButton { pos: p, .. }) = self.synthetic.first() {
            raw.events.push(egui::Event::PointerMoved(*p));
        }
        let n = n.min(self.synthetic.len());
        raw.events.extend(self.synthetic.drain(..n));
    }

    /// Keyboard and clipboard for the slide (when no text field has focus).
    fn keyboard(&mut self, ctx: &egui::Context) {
        let mac = ctx.os() == egui::os::OperatingSystem::Mac;
        self.session.mac_keys = mac;
        if self.dialog.as_ref().is_some_and(|d| d.modal()) || self.palette.is_some() {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        if ctx.input(|i| i.pointer.any_pressed()) || self.session.active().is_some_and(|d| !d.selection.shapes.is_empty()) {
            self.thumbs_focused = false;
        }
        // F6 cycles the panes even from the notes field.
        let f6 = events.iter().find_map(|e| match e {
            egui::Event::Key { key: egui::Key::F6, pressed: true, modifiers, .. } => Some(modifiers.shift),
            _ => None,
        });
        if let Some(back) = f6
            && !self.home_open()
            && self.session.active().is_some()
        {
            self.cycle_pane(ctx, back);
            return;
        }
        if ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        if self.home_open() {
            // Only app shortcuts (Open, Command Palette…) and Escape, which goes back to the
            // presentation: typing, pasting or deleting would edit the hidden presentation.
            for e in events {
                if let egui::Event::Key { key, physical_key, pressed: true, modifiers, .. } = e {
                    let mods = mods_of(modifiers, mac);
                    if key == egui::Key::Escape {
                        self.home = None;
                    } else if mods.cmd || mods.alt {
                        menus::ui_shortcut(self, ctx, shortcut_key(key, physical_key, modifiers.shift), mods);
                    }
                }
            }
            self.drain_requests();
            return;
        }
        for (i, e) in events.iter().enumerate() {
            match e {
                egui::Event::Text(t) => {
                    if self.session.active().is_some_and(|d| d.selection.text.is_some() || d.selection.shapes.len() == 1)
                        && let Err(e) = self.session.type_text(t)
                    {
                        self.set_status(e.to_string());
                    }
                }
                egui::Event::Copy => {
                    if let Ok(v) = self.session.execute("edit.copy", &json!({}))
                        && let Some(t) = v.get("text").and_then(Value::as_str)
                    {
                        ctx.copy_text(t.to_string());
                    }
                }
                egui::Event::Cut => {
                    if let Ok(v) = self.session.execute("edit.cut", &json!({}))
                        && let Some(t) = v.get("text").and_then(Value::as_str)
                    {
                        ctx.copy_text(t.to_string());
                    }
                }
                egui::Event::Paste(text) => {
                    // An image on the clipboard (screenshots, copied pictures) becomes a picture.
                    let img = if text.is_empty() { self.services.clipboard_image.as_mut().and_then(|f| f()) } else { None };
                    let r = match img {
                        Some(png) => self
                            .session
                            .execute("insert.picture", &json!({"name": "Pasted image.png", "data": deckcraft_engine::cmd::base64_encode(&png)})),
                        None => self.session.execute("edit.paste", &json!({"text": text})),
                    };
                    if let Err(e) = r {
                        self.set_status(e.to_string());
                    }
                }
                egui::Event::Key { key, physical_key, pressed: true, modifiers, .. } => {
                    // AltGr: winit strips Ctrl+Alt from the right Alt key on AltGr layouts, but
                    // a backend that reports AltGr as Ctrl+Alt sends the character too. A Ctrl+Alt
                    // key that also types one (µ, [ on some layouts) is text, not a shortcut.
                    if !mac && modifiers.ctrl && modifiers.alt && matches!(events.get(i + 1), Some(egui::Event::Text(_))) {
                        continue;
                    }
                    let key = shortcut_key(*key, *physical_key, modifiers.shift);
                    let mods = mods_of(*modifiers, mac);
                    if !self.session.tool.drawing_freeform() && menus::ui_shortcut(self, ctx, key, mods) {
                        continue;
                    }
                    let name = key_name(key);
                    match self.session.key(name, mods) {
                        Ok(_) => {}
                        Err(e) => self.set_status(e.to_string()),
                    }
                }
                _ => {}
            }
        }
        self.drain_requests();
    }

    /// Lay out the whole window.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if !self.fonts_ready {
            ctx.request_repaint();
            return;
        }
        let t0 = now_ms();
        let t = theme::Tokens::get(&ctx);
        self.tick_media(&ctx);
        if self.show.is_some() {
            show::ui(self, ui);
            self.perf.frame_ms = now_ms() - t0;
            return;
        }
        if self.home.is_some_and(|k| k != self.docs_key()) {
            self.home = None;
        }
        self.keyboard(&ctx);
        ribbon::title_bar(self, ui);
        if self.session.active().is_some() && !self.home_open() {
            ribbon::show(self, ui);
            status::status_bar(self, ui);
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| {
            if self.session.active().is_none() || self.home_open() {
                dialogs::start_screen(self, ui);
                return;
            }
            match self.ui.view {
                ViewMode::Sorter => sorter::show(self, ui),
                ViewMode::NotesPage => sorter::notes_page(self, ui),
                ViewMode::Outline | ViewMode::Normal | ViewMode::Reading => {
                    if self.ui.pane.is_some() {
                        panes::show(self, ui);
                    }
                    if self.ui.view == ViewMode::Outline {
                        sorter::outline_pane(self, ui);
                    } else {
                        thumbs::show(self, ui);
                    }
                    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.canvas)).show(ui, |ui| {
                        if self.ui.notes {
                            status::notes_pane(self, ui);
                        }
                        canvas::show(self, ui);
                    });
                }
            }
        });
        dialogs::show(self, &ctx);
        menus::palette(self, &ctx);
        for url in std::mem::take(&mut self.pending_urls) {
            ctx.open_url(egui::OpenUrl::new_tab(url));
        }
        self.drain_requests();
        self.perf.frame_ms = now_ms() - t0;
    }

    /// Media clocks and video textures; the editor's media stops when its slide is left.
    fn tick_media(&mut self, ctx: &egui::Context) {
        if !self.media.any_active() {
            self.session.media_status.clear();
            return;
        }
        if self.show.is_none()
            && let Some(d) = self.session.active()
        {
            let here: Vec<deckcraft_model::ShapeId> = if d.selection.target == deckcraft_engine::Target::Slides {
                media::slide_media(&d.doc, d.selection.slide).into_iter().map(|m| m.0).collect()
            } else {
                vec![]
            };
            let gone: Vec<_> =
                self.media.ids().into_iter().filter(|id| !here.contains(id) && self.media.owner(*id) == Some(media::Owner::Editor)).collect();
            for id in gone {
                self.media.stop(id);
            }
        }
        self.media.tick(ctx, &mut self.session);
    }

    pub fn open_url(&mut self, url: &str) {
        self.pending_urls.push(url.to_string());
    }

    fn drain_control(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.control_rx.take() else { return };
        while let Ok(req) = rx.try_recv() {
            let reply = req.reply.clone();
            match control::handle(self, ctx, &req) {
                control::Outcome::Done(v) => {
                    let _ = reply.send(v);
                }
                control::Outcome::Screenshot { path } => {
                    self.shot_token += 1;
                    let token = self.shot_token;
                    self.queued_shots.push((token, now_ms() + 120.0, 0));
                    self.pending_shots.push((token, path, reply, now_ms() + 8000.0));
                }
            }
        }
        self.control_rx = Some(rx);
    }

    fn issue_screenshots(&mut self, ctx: &egui::Context) {
        let now = now_ms();
        self.queued_shots.retain_mut(|(token, at, frames)| {
            *frames += 1;
            if now >= *at && *frames >= 3 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(*token)));
                false
            } else {
                true
            }
        });
        if !self.queued_shots.is_empty() || !self.pending_shots.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    fn collect_screenshots(&mut self, ctx: &egui::Context) {
        if self.pending_shots.is_empty() {
            return;
        }
        let events: Vec<_> = ctx.input(|i| {
            i.raw
                .events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Screenshot { user_data, image, .. } => {
                        let token = user_data.data.as_ref().and_then(|d| d.downcast_ref::<u64>()).copied()?;
                        Some((token, image.clone()))
                    }
                    _ => None,
                })
                .collect()
        });
        for (token, image) in events {
            if let Some(i) = self.pending_shots.iter().position(|(t, ..)| *t == token) {
                let (_, path, reply, _) = self.pending_shots.remove(i);
                let _ = reply.send(control::save_screenshot(self, &image, path.as_deref()));
            }
        }
        let now = now_ms();
        self.pending_shots.retain(|(_, _, reply, deadline)| {
            if now < *deadline {
                return true;
            }
            let _ = reply.send(json!({"ok": false, "error": "no frame was presented (screen locked or window hidden); use ui.render"}));
            false
        });
    }
}

/// Engine modifiers for egui's: Ctrl or Command is `cmd`; on a Mac, Control is also `control`
/// (PowerPoint for Mac's Command+Control shortcuts).
pub fn mods_of(m: egui::Modifiers, mac: bool) -> Mods {
    Mods { shift: m.shift, alt: m.alt, cmd: m.command || m.ctrl, control: mac && m.ctrl }
}

/// The key a shortcut names. A shifted symbol is the key it's typed on (US layout), so Cmd+Shift+[
/// and Cmd+Shift+1 match although the key reports `{` or `!`.
pub fn shortcut_key(key: egui::Key, physical: Option<egui::Key>, shift: bool) -> egui::Key {
    use egui::Key::*;
    if !shift {
        return key;
    }
    match key {
        Exclamationmark | OpenCurlyBracket | CloseCurlyBracket | Questionmark | Colon | Pipe | Plus => physical.unwrap_or(match key {
            Exclamationmark => Num1,
            OpenCurlyBracket => OpenBracket,
            CloseCurlyBracket => CloseBracket,
            Questionmark => Slash,
            Colon => Semicolon,
            Pipe => Backslash,
            _ => Equals,
        }),
        _ => key,
    }
}

/// Engine key name for an egui key.
pub fn key_name(key: egui::Key) -> &'static str {
    use egui::Key::*;
    match key {
        Enter => "Enter",
        Escape => "Escape",
        Backspace => "Backspace",
        Delete => "Delete",
        Tab => "Tab",
        ArrowLeft => "Left",
        ArrowRight => "Right",
        ArrowUp => "Up",
        ArrowDown => "Down",
        Home => "Home",
        End => "End",
        PageUp => "PageUp",
        PageDown => "PageDown",
        Space => "Space",
        F2 => "F2",
        F5 => "F5",
        F7 => "F7",
        other => other.name(),
    }
}

pub fn now_ms() -> f64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0)
    }
    #[cfg(target_arch = "wasm32")]
    {
        js_now()
    }
}

#[cfg(target_arch = "wasm32")]
fn js_now() -> f64 {
    js_sys::Date::now()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn title_text(app: &SlideApp) -> String {
        let d = app.session.active().unwrap();
        let sh = &d.doc.slides[0].shapes[0];
        sh.text.as_ref().map(|t| t.paragraphs.iter().map(|p| p.text()).collect::<Vec<_>>().join("\n")).unwrap_or_default()
    }

    /// #17: Home created a new blank presentation instead of going to the start screen.
    #[test]
    fn home_shows_the_start_screen_over_the_open_presentation() {
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_ui_state(
            |ui, app: &mut SlideApp| {
                app.logic(ui.ctx());
                app.ui(ui);
            },
            SlideApp::new(Session::with_new(), Services::default()),
        );
        // The app keeps requesting repaints, so step frames instead of run().
        harness.run_steps(2);
        let title = harness.state().session.active().unwrap().doc.slides[0].shapes[0].id.0;
        harness.state_mut().session.execute("text.edit", &json!({"id": title})).unwrap();

        harness.state_mut().run("app.home", json!({})).unwrap();
        harness.run_steps(2);
        assert!(harness.state().home_open());
        assert_eq!(harness.state().session.documents().len(), 1, "Home doesn't create a presentation");

        // Typing on the start screen doesn't reach the presentation behind it; Escape goes back.
        harness.event(egui::Event::Text("a".into()));
        harness.key_press(egui::Key::Backspace);
        harness.run_steps(2);
        assert_eq!(title_text(harness.state()), "");
        assert!(!harness.state().session.active().unwrap().is_dirty());
        harness.key_press(egui::Key::Escape);
        harness.run_steps(2);
        assert!(!harness.state().home_open());

        // Home again toggles; picking a theme on the start screen (a new presentation) leaves it.
        harness.state_mut().run("app.home", json!({})).unwrap();
        harness.run_steps(2);
        assert!(harness.state().home_open());
        harness.state_mut().run("app.home", json!({})).unwrap();
        assert!(!harness.state().home_open());
        harness.state_mut().run("app.home", json!({"on": true})).unwrap();
        harness.state_mut().run("file.new", json!({})).unwrap();
        harness.run_steps(2);
        assert!(!harness.state().home_open(), "a new presentation leaves Home");
        assert_eq!(harness.state().session.documents().len(), 2);
    }

    #[test]
    fn home_needs_an_open_presentation() {
        let mut app = SlideApp::new(Session::new(), Services::default());
        app.run("app.home", json!({})).unwrap();
        assert!(!app.home_open() && app.home.is_none(), "with nothing open the start screen shows anyway");
    }

    type Os = egui::os::OperatingSystem;

    fn harness(os: Os) -> egui_kittest::Harness<'static, SlideApp> {
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_ui_state(
            |ui, app: &mut SlideApp| {
                app.logic(ui.ctx());
                app.ui(ui);
            },
            SlideApp::new(Session::with_new(), Services::default()),
        );
        h.ctx.set_os(os);
        h.run_steps(2);
        h
    }

    /// A key press as egui-winit reports it (`physical` is the key's US-layout position).
    fn press(h: &mut egui_kittest::Harness<'static, SlideApp>, key: egui::Key, physical: Option<egui::Key>, modifiers: egui::Modifiers) {
        h.event(egui::Event::Key { key, physical_key: physical, pressed: true, repeat: false, modifiers });
        h.event(egui::Event::Key { key, physical_key: physical, pressed: false, repeat: false, modifiers });
        h.run_steps(2);
    }

    fn mods(ctrl: bool, shift: bool, alt: bool, mac_cmd: bool) -> egui::Modifiers {
        egui::Modifiers { alt, ctrl, shift, mac_cmd, command: mac_cmd || ctrl }
    }

    fn n_slides(h: &egui_kittest::Harness<'static, SlideApp>) -> usize {
        h.state().session.active().unwrap().doc.slides.len()
    }

    fn dialog(h: &mut egui_kittest::Harness<'static, SlideApp>) -> Option<String> {
        h.state_mut().dialog.take().map(|d| d.id)
    }

    /// Type a title and select its shape (formatting keys then apply to all its text).
    fn select_title(h: &mut egui_kittest::Harness<'static, SlideApp>) {
        let id = h.state().session.active().unwrap().current_slide().unwrap().shapes[0].id.0;
        let s = &mut h.state_mut().session;
        s.execute("edit.select", &json!({"ids": [id]})).unwrap();
        s.execute("text.edit", &json!({})).unwrap();
        s.execute("text.insert", &json!({"text": "H2O"})).unwrap();
        s.execute("edit.select", &json!({"ids": [id]})).unwrap();
    }

    fn subscript(h: &mut egui_kittest::Harness<'static, SlideApp>) -> bool {
        h.state_mut().session.execute("format.state", &json!({})).unwrap()["subscript"] == true
    }

    /// PowerPoint for Windows keys through the real key path: egui events → `menus::ui_shortcut`
    /// → `Session::key`.
    #[test]
    fn windows_shortcuts_through_the_ui() {
        use egui::Key::*;
        let mut h = harness(Os::Windows);
        let ctrl = mods(true, false, false, false);
        press(&mut h, M, None, ctrl);
        assert_eq!(n_slides(&h), 2, "Ctrl+M: new slide");
        press(&mut h, Num2, None, ctrl);
        assert_eq!(h.state().ui.view, ViewMode::Sorter);
        press(&mut h, Num1, None, ctrl);
        assert_eq!(h.state().ui.view, ViewMode::Normal);

        // egui names punctuation keys (`CloseBracket`) and reports Shift+[ as `{`.
        let a = h.state_mut().session.execute("shape.insert", &json!({"preset": "rect", "rect": [10, 10, 50, 50]})).unwrap()["id"].as_u64().unwrap();
        h.state_mut().session.execute("shape.insert", &json!({"preset": "rect", "rect": [90, 10, 50, 50]})).unwrap();
        h.state_mut().session.execute("edit.select", &json!({"ids": [a]})).unwrap();
        let order = |h: &egui_kittest::Harness<'static, SlideApp>| {
            h.state().session.active().unwrap().current_slide().unwrap().shapes.iter().map(|s| s.id.0 as u64).collect::<Vec<_>>()
        };
        press(&mut h, CloseBracket, Some(CloseBracket), ctrl);
        assert_eq!(order(&h).last(), Some(&a), "Ctrl+]: Bring Forward");
        press(&mut h, OpenCurlyBracket, Some(OpenBracket), mods(true, true, false, false));
        assert_eq!(order(&h).first(), Some(&a), "Ctrl+Shift+[: Send to Back");
        press(&mut h, ArrowRight, None, mods(false, false, true, false));
        assert_eq!(
            h.state().session.active().unwrap().shape(deckcraft_model::ShapeId(a as u32)).unwrap().xfrm.unwrap().rot,
            15.0,
            "Alt+Right rotates"
        );

        press(&mut h, F7, None, egui::Modifiers::NONE);
        assert_eq!(dialog(&mut h).as_deref(), Some("spelling"));
        // AltGr arrives as Ctrl+Alt: AltGr+M types µ on some layouts and isn't New Comment. The
        // key and its text come in the same frame.
        let altgr = mods(true, false, true, false);
        h.input_mut().events.extend([
            egui::Event::Key { key: M, physical_key: Some(M), pressed: true, repeat: false, modifiers: altgr },
            egui::Event::Text("µ".into()),
        ]);
        h.run_steps(2);
        assert_eq!(dialog(&mut h), None);
        press(&mut h, M, Some(M), mods(true, false, true, false));
        assert_eq!(dialog(&mut h).as_deref(), Some("comment"), "Ctrl+Alt+M: New Comment");

        // Ctrl+= is Subscript, not zoom, and Ctrl+Shift+= (reported as `+`) Superscript.
        select_title(&mut h);
        press(&mut h, Equals, Some(Equals), ctrl);
        assert!(subscript(&mut h));
        assert_eq!(h.state().ui.zoom, None);
        press(&mut h, Plus, Some(Equals), mods(true, true, false, false));
        assert_eq!(h.state_mut().session.execute("format.state", &json!({})).unwrap()["superscript"], true);

        press(&mut h, F5, None, mods(false, false, true, false));
        assert!(h.state().show.as_ref().is_some_and(|s| s.presenter), "Alt+F5: Presenter View");
    }

    /// PowerPoint for Mac keys where they differ from Windows.
    #[test]
    fn mac_shortcuts_through_the_ui() {
        use egui::Key::*;
        let mut h = harness(Os::Mac);
        let cmd = mods(false, false, false, true);
        press(&mut h, M, None, cmd);
        assert_eq!(n_slides(&h), 1, "Cmd+M is the system's Minimize, not New Slide");
        press(&mut h, N, None, mods(false, true, false, true));
        assert_eq!(n_slides(&h), 2, "Cmd+Shift+N: new slide");
        press(&mut h, M, None, mods(false, false, true, true));
        assert_eq!(dialog(&mut h).as_deref(), Some("paragraph"), "Cmd+Option+M: Paragraph");
        press(&mut h, M, None, mods(false, true, false, true));
        assert_eq!(dialog(&mut h).as_deref(), Some("comment"), "Cmd+Shift+M: New Comment");
        select_title(&mut h);
        press(&mut h, Equals, None, cmd);
        assert!(h.state().ui.zoom.is_some(), "Cmd+= zooms in");
        assert!(!subscript(&mut h));
        press(&mut h, Equals, None, mods(true, false, false, true));
        assert!(subscript(&mut h), "Cmd+Control+=: Subscript");
        press(&mut h, Num1, Some(Num1), mods(false, true, false, true));
        assert_eq!(h.state().ui.pane.as_deref(), Some("format"), "Cmd+Shift+1: Format pane");
    }

    /// F6 / Shift+F6 move the keyboard between the slide, the notes and the thumbnails.
    #[test]
    fn f6_cycles_the_panes() {
        let mut h = harness(Os::Windows);
        h.state_mut().session.execute("slide.new", &json!({})).unwrap();
        h.state_mut().session.execute("slide.go", &json!({"index": 0})).unwrap();
        let notes = egui::Id::new("notes_edit");
        press(&mut h, egui::Key::F6, None, egui::Modifiers::NONE);
        assert!(h.ctx.memory(|m| m.has_focus(notes)), "slide → notes");
        press(&mut h, egui::Key::F6, None, egui::Modifiers::NONE);
        assert!(!h.ctx.memory(|m| m.has_focus(notes)));
        assert!(h.state().thumbs_focused, "notes → thumbnails");
        press(&mut h, egui::Key::ArrowDown, None, egui::Modifiers::NONE);
        assert_eq!(h.state().session.active().unwrap().selection.slide, 1, "arrows change slides in the thumbnail pane");
        press(&mut h, egui::Key::F6, None, egui::Modifiers::NONE);
        assert!(!h.state().thumbs_focused, "thumbnails → slide");
        press(&mut h, egui::Key::F6, None, mods(false, true, false, false));
        assert!(h.state().thumbs_focused, "Shift+F6 goes back");
    }
}
