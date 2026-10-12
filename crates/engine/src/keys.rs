//! Keyboard: text-editing keys while editing text, nudges and shortcuts otherwise. Shared by the
//! UI, the control channel and MCP so a key does the same thing everywhere. Shortcuts follow
//! PowerPoint's published Windows and Mac lists; where the two differ, [`Session::mac_keys`]
//! picks the Mac one. `docs/ui-parity.md` (Shortcut audit) holds the comparison.

use serde_json::{Value, json};

use crate::tools::Mods;
use crate::{Result, Session};

/// Does a shortcut like `Cmd+Shift+]` match `key` + `mods`?
pub fn shortcut_matches(shortcut: &str, key: &str, mods: Mods) -> bool {
    let parts: Vec<&str> = shortcut.split('+').collect();
    let (k, ms) = match parts.as_slice() {
        [.., "", ""] => ("+", &parts[..parts.len().saturating_sub(2)]),
        _ => match parts.split_last() {
            Some((k, ms)) => (*k, ms),
            None => return false,
        },
    };
    let has = |m: &str| ms.iter().any(|x| x.eq_ignore_ascii_case(m));
    k.eq_ignore_ascii_case(key) && (has("cmd") || has("ctrl")) == mods.cmd && has("shift") == mods.shift && (has("alt") || has("option")) == mods.alt
}

/// Which PowerPoint a shortcut comes from. `Windows` also covers Linux, FreeBSD and the web on
/// those systems.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Any,
    Windows,
    Mac,
}

impl Platform {
    pub fn applies(self, mac: bool) -> bool {
        match self {
            Platform::Any => true,
            Platform::Windows => !mac,
            Platform::Mac => mac,
        }
    }
}

/// PowerPoint shortcuts beyond each command's own (`CommandSpec::shortcut`, the one menus show):
/// the other platform's key for the same command, and keys whose meaning differs between Windows
/// and Mac. Looked up before the registry, so a platform's entry wins over a registry key.
pub const EXTRA_SHORTCUTS: &[(&str, &str, Platform)] = &[
    ("Cmd+M", "slide.new", Platform::Windows),
    ("Cmd+Shift+D", "slide.duplicate", Platform::Any),
    ("Cmd+G", "arrange.group", Platform::Windows),
    ("Cmd+Shift+G", "arrange.ungroup", Platform::Windows),
    ("Cmd+Shift+J", "arrange.regroup", Platform::Windows),
    ("Cmd+Shift+F", "arrange.bringToFront", Platform::Mac),
    ("Cmd+Shift+B", "arrange.sendToBack", Platform::Mac),
    ("Cmd+Alt+Shift+F", "arrange.bringForward", Platform::Mac),
    ("Cmd+Alt+Shift+B", "arrange.sendBackward", Platform::Mac),
    ("Cmd+Alt+Shift+C", "animation.painter", Platform::Mac),
    ("Cmd+Alt+Shift+V", "animation.painterApply", Platform::Mac),
    ("Cmd+Shift+Z", "format.clear", Platform::Windows),
    ("Cmd+Shift+Z", "edit.redo", Platform::Mac),
];

/// A key name as `Session::key` matches it: lower case, `ArrowLeft` → `left`, and egui's names for
/// punctuation (`Period`, `OpenBracket`…) → the character, so `Cmd+Shift+.` matches either.
pub fn key_token(key: &str) -> String {
    let k = key.to_ascii_lowercase();
    let k = k.trim_start_matches("arrow");
    match k {
        "return" => "enter",
        "esc" => "escape",
        " " => "space",
        "period" => ".",
        "comma" => ",",
        "equals" => "=",
        "plus" => "+",
        "minus" => "-",
        "openbracket" => "[",
        "closebracket" => "]",
        "backtick" => "`",
        "semicolon" => ";",
        "slash" => "/",
        "backslash" => "\\",
        "quote" => "'",
        other => other,
    }
    .to_string()
}

impl Session {
    /// Handle one key press. `key` is a name (`Enter`, `Escape`, `Backspace`, `Delete`, `Tab`,
    /// `ArrowLeft`…, `Home`, `End`, `PageUp`, `PageDown`, `F5`, `Period`) or a character (`b`, `]`).
    /// Returns `{handled: bool, …}`.
    pub fn key(&mut self, key: &str, mods: Mods) -> Result<Value> {
        let mac = self.mac_keys;
        let editing = self.active().is_some_and(|d| d.selection.text.is_some());
        let has_shapes = self.active().is_some_and(|d| !d.selection.shapes.is_empty());
        let token = key_token(key);
        let k = token.as_str();
        if self.tool.drawing_freeform() && matches!(k, "enter" | "escape") {
            return self.finish_freeform(false);
        }
        let handled = || Ok(json!({"handled": true}));
        if editing && let Some((id, p)) = self.editing_key(k, mods, mac) {
            match id {
                "text.nextPlaceholder" => self.next_placeholder()?,
                "table.nextCell" => self.next_cell(mods.shift)?,
                _ => self.execute(id, &p)?,
            };
            return handled();
        }
        if !editing && has_shapes {
            if let Some((id, p)) = self.object_key(k, mods) {
                self.execute(id, &p)?;
                return handled();
            }
            match k {
                "tab" if !mods.cmd => {
                    self.cycle_selection(!mods.shift)?;
                    return handled();
                }
                "enter" if !mods.cmd && !mods.alt => {
                    self.execute("text.edit", &json!({}))?;
                    return handled();
                }
                // Windows Ctrl+Enter: the next title or body placeholder.
                "enter" if mods.cmd && !mods.shift && !mods.alt && !mac => {
                    self.next_placeholder()?;
                    return handled();
                }
                _ => {}
            }
        }
        if !editing && !has_shapes {
            if !mods.cmd && !mods.alt {
                let r = match k {
                    "pagedown" | "down" | "right" => Some("slide.next"),
                    "pageup" | "up" | "left" => Some("slide.previous"),
                    "home" => Some("slide.first"),
                    "end" => Some("slide.last"),
                    "enter" if !mods.shift => Some("slide.new"),
                    "tab" => {
                        self.cycle_selection(!mods.shift)?;
                        return handled();
                    }
                    _ => None,
                };
                if let Some(id) = r {
                    self.execute(id, &json!({}))?;
                    return handled();
                }
            }
            // Ctrl/Cmd+Up/Down move the slide one place; with Shift, to the start or end.
            if mods.cmd && !mods.alt && matches!(k, "up" | "down") {
                return self.move_slide_key(k == "up", mods.shift);
            }
        }
        // Windows Ctrl+Alt+5: select the first object on the slide.
        if k == "5" && !editing && mods.cmd && mods.alt && !mods.shift && !mac {
            self.select(|_, sel| {
                sel.shapes.clear();
                sel.text = None;
            })?;
            self.cycle_selection(true)?;
            return handled();
        }
        // Extra PowerPoint shortcuts, then the registry's.
        let extra = EXTRA_SHORTCUTS.iter().find(|(sc, _, pl)| pl.applies(mac) && shortcut_matches(sc, k, mods)).map(|x| x.1);
        let found = extra.or_else(|| crate::command_specs().iter().find(|c| c.shortcut.is_some_and(|s| shortcut_matches(s, k, mods))).map(|c| c.id));
        if let Some(id) = found {
            let r = self.execute(id, &json!({}))?;
            return Ok(json!({"handled": true, "command": id, "result": r}));
        }
        Ok(json!({"handled": false}))
    }

    /// Keys while editing text: caret movement, deletion, list levels and table cells. PowerPoint
    /// for Windows moves by words and paragraphs with Ctrl; PowerPoint for Mac by words with
    /// Option and to line or text ends with Command.
    fn editing_key(&self, k: &str, m: Mods, mac: bool) -> Option<(&'static str, Value)> {
        let mv = |to: &str| Some(("text.move", json!({"to": to, "extend": m.shift})));
        let del = |dir: &str| Some(("text.delete", json!({"dir": dir})));
        let ins = |t: &str| Some(("text.insert", json!({"text": t})));
        let sel = self.active().and_then(|d| d.selection.text.as_ref());
        let in_cell = sel.is_some_and(|t| t.cell.is_some());
        // Tab at the start of a paragraph (or over a selection) changes the list level.
        let at_start = sel.is_some_and(|t| t.ordered().0.1 == 0 || t.is_range());
        if m.cmd {
            return match k {
                "left" if !m.alt => mv(if mac { "lineStart" } else { "wordLeft" }),
                "right" if !m.alt => mv(if mac { "lineEnd" } else { "wordRight" }),
                "up" if !m.alt => mv(if mac { "start" } else { "paraStart" }),
                "down" if !m.alt => mv(if mac {
                    "end"
                } else if m.shift {
                    "paraEnd"
                } else {
                    "paraNext"
                }),
                "home" => mv("start"),
                "end" => mv("end"),
                "backspace" => del("wordBackward"),
                "delete" if !mac => del("wordForward"),
                // Ctrl+Tab types a tab in a table cell (Tab moves to the next cell).
                "tab" if in_cell && !m.shift => ins("\t"),
                // PowerPoint for Mac: Cmd+[ / Cmd+] promote / demote the paragraph.
                "[" if mac && !m.shift && !m.alt => Some(("format.outdent", json!({}))),
                "]" if mac && !m.shift && !m.alt => Some(("format.indent", json!({}))),
                "enter" if !mac && !m.shift && !m.alt => Some(("text.nextPlaceholder", Value::Null)),
                _ => None,
            };
        }
        match k {
            // Windows Alt+Shift+Left/Right promote / demote (on a Mac they select by words).
            "left" if m.alt && m.shift && !mac => Some(("format.outdent", json!({}))),
            "right" if m.alt && m.shift && !mac => Some(("format.indent", json!({}))),
            "left" => mv(if m.alt { "wordLeft" } else { "left" }),
            "right" => mv(if m.alt { "wordRight" } else { "right" }),
            "up" => mv(if m.alt { "paraStart" } else { "up" }),
            "down" => mv(if m.alt { "paraEnd" } else { "down" }),
            "home" => mv("lineStart"),
            "end" => mv("lineEnd"),
            "backspace" => del(if m.alt { "wordBackward" } else { "backward" }),
            "delete" => del(if m.alt { "wordForward" } else { "forward" }),
            "enter" => ins(if m.shift { "\u{b}" } else { "\n" }),
            "escape" => Some(("text.exit", json!({}))),
            // PowerPoint for Mac: Option+Shift+Tab types a tab in a table cell.
            "tab" if in_cell && m.alt => ins("\t"),
            "tab" if in_cell => Some(("table.nextCell", Value::Null)),
            "tab" if at_start => Some((if m.shift { "format.outdent" } else { "format.indent" }, json!({}))),
            "tab" => ins("\t"),
            _ => None,
        }
    }

    /// Keys with objects selected: nudge (arrows; Ctrl/Cmd or Option for 1 pt), rotate 15°
    /// (Alt/Option+Left/Right), resize (Shift+arrows), delete, edit, deselect, play media (Space).
    fn object_key(&self, k: &str, m: Mods) -> Option<(&'static str, Value)> {
        let arrow = match k {
            "left" => Some((-1.0, 0.0)),
            "right" => Some((1.0, 0.0)),
            "up" => Some((0.0, -1.0)),
            "down" => Some((0.0, 1.0)),
            _ => None,
        };
        if let Some((x, y)) = arrow {
            let step = if m.cmd || m.alt { 1.0 } else { 6.0 };
            return if m.alt && !m.cmd && !m.shift && x != 0.0 {
                Some(("shape.rotate", json!({"by": 15.0 * x})))
            } else if m.shift && !m.alt {
                // Right/Up grow, Left/Down shrink.
                Some(("shape.resize", json!({"dw": step * x, "dh": -step * y})))
            } else if !m.shift {
                Some(("shape.move", json!({"dx": step * x, "dy": step * y})))
            } else {
                None
            };
        }
        let media = self.active().is_some_and(|d| {
            d.selection.shapes.len() == 1
                && d.selection.shapes.first().and_then(|id| d.shape(*id)).is_some_and(|s| matches!(s.kind, deckcraft_model::ShapeKind::Media(_)))
        });
        match k {
            // Space (Mac) and Ctrl+Space (Windows) play or pause the selected audio or video.
            "space" if media && !m.shift && !m.alt => Some(("media.toggle", json!({}))),
            _ if m.cmd => None,
            "backspace" | "delete" => Some(("edit.delete", json!({}))),
            "f2" => Some(("text.edit", json!({}))),
            "escape" => Some(("edit.deselect", json!({}))),
            _ => None,
        }
    }

    /// Ctrl/Cmd+Up/Down: the current slide one place up or down; with Shift, to the start or end.
    fn move_slide_key(&mut self, up: bool, to_end: bool) -> Result<Value> {
        let st = self.doc()?;
        if st.selection.target != crate::Target::Slides || st.doc.slides.is_empty() {
            return Ok(json!({"handled": false}));
        }
        let (cur, last) = (st.selection.slide, st.doc.slides.len().saturating_sub(1));
        let to = match (up, to_end) {
            (true, true) => 0,
            (true, false) => cur.saturating_sub(1),
            (false, true) => last,
            (false, false) => (cur + 1).min(last),
        };
        if to != cur {
            self.execute("slide.move", &json!({"from": cur, "to": to}))?;
        }
        Ok(json!({"handled": true, "command": "slide.move"}))
    }

    /// Windows Ctrl+Enter: edit the next title or body placeholder on the slide; after the last
    /// one, a new slide, in its first placeholder.
    fn next_placeholder(&mut self) -> Result<Value> {
        use deckcraft_model::PhType;
        let text_ph = |s: &deckcraft_model::Shape| {
            !s.hidden && s.ph_type().is_some_and(|k| matches!(k, PhType::Title | PhType::CtrTitle | PhType::SubTitle | PhType::Body | PhType::Obj))
        };
        let st = self.doc()?;
        let cur = st.selection.text.as_ref().map(|t| t.shape).or_else(|| st.selection.shapes.first().copied());
        let phs: Vec<_> = st.shapes().iter().filter(|s| text_ph(s)).map(|s| s.id).collect();
        let next = match cur.and_then(|c| phs.iter().position(|x| *x == c)) {
            Some(i) => phs.get(i + 1).copied(),
            None => phs.first().copied(),
        };
        if let Some(id) = next {
            return self.execute("text.edit", &json!({"id": id}));
        }
        if st.selection.target != crate::Target::Slides {
            return Ok(Value::Null);
        }
        self.execute("slide.new", &json!({}))?;
        let first = self.doc()?.shapes().iter().find(|s| text_ph(s)).map(|s| s.id);
        match first {
            Some(id) => self.execute("text.edit", &json!({"id": id})),
            None => Ok(Value::Null),
        }
    }

    /// Type text: into the text being edited, or (a shape selected) start editing it.
    pub fn type_text(&mut self, text: &str) -> Result<Value> {
        let st = self.doc()?;
        if st.selection.text.is_none() {
            let Some(id) = st.selection.shapes.first().copied() else { return Ok(json!({"handled": false})) };
            // Audio and video take Space (play/pause) rather than text.
            let media = st.shape(id).is_some_and(|s| matches!(s.kind, deckcraft_model::ShapeKind::Media(_)));
            if st.selection.shapes.len() != 1 || media {
                return Ok(json!({"handled": false}));
            }
            // Typing on a selected shape replaces its text (PowerPoint behaviour).
            self.execute("text.edit", &json!({"id": id}))?;
            self.execute("text.selectAll", &json!({}))?;
        }
        self.execute("text.insert", &json!({"text": text}))
    }

    fn cycle_selection(&mut self, forward: bool) -> Result<()> {
        let st = self.doc()?;
        let ids: Vec<_> = st.shapes().iter().filter(|s| !s.hidden).map(|s| s.id).collect();
        if ids.is_empty() {
            return Ok(());
        }
        let cur = st.selection.shapes.first().and_then(|c| ids.iter().position(|x| x == c));
        let n = ids.len();
        let next = match cur {
            Some(i) if forward => (i + 1) % n,
            Some(i) => (i + n - 1) % n,
            None if forward => 0,
            None => n - 1,
        };
        let id = ids.get(next).copied();
        self.select(|_, sel| {
            sel.shapes = id.into_iter().collect();
            sel.text = None;
        })
    }

    fn next_cell(&mut self, back: bool) -> Result<Value> {
        let st = self.doc()?;
        let Some(t) = st.selection.text.clone() else { return Ok(Value::Null) };
        let Some((r, c)) = t.cell else { return Ok(Value::Null) };
        let Some(deckcraft_model::ShapeKind::Table(tb)) = st.shape(t.shape).map(|s| s.kind.clone()) else { return Ok(Value::Null) };
        let (nr, nc) = (tb.n_rows(), tb.n_cols());
        let idx = r * nc + c;
        if !back && idx + 1 >= nr * nc {
            // Tab in the last cell adds a row.
            self.execute("table.insertRowBelow", &json!({"id": t.shape, "row": r}))?;
            return self.execute("text.edit", &json!({"id": t.shape, "cell": [r + 1, 0]}));
        }
        let ni = if back { idx.saturating_sub(1) } else { idx + 1 };
        self.execute("text.edit", &json!({"id": t.shape, "cell": [ni / nc.max(1), ni % nc.max(1)]}))?;
        self.execute("text.selectAll", &json!({}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_parsing() {
        let m = |shift, alt, cmd| Mods { shift, alt, cmd, control: false };
        assert!(shortcut_matches("Cmd+B", "b", m(false, false, true)));
        assert!(!shortcut_matches("Cmd+B", "b", m(true, false, true)));
        assert!(shortcut_matches("Cmd+Shift+]", "]", m(true, false, true)));
        assert!(shortcut_matches("Cmd+Shift+.", ".", m(true, false, true)));
        assert!(shortcut_matches("Delete", "Delete", m(false, false, false)));
    }

    #[test]
    fn keys_drive_editing() {
        let mut s = Session::with_new();
        let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
        s.execute("edit.select", &json!({"ids": [title]})).unwrap();
        s.type_text("Hi").unwrap();
        s.key("Enter", Mods::default()).unwrap();
        s.type_text("there").unwrap();
        s.key("Backspace", Mods::default()).unwrap();
        s.key("Escape", Mods::default()).unwrap();
        let t = s.execute("text.get", &json!({"id": title})).unwrap();
        assert_eq!(t["text"], "Hi\nther");
        let r = s.key("b", Mods { cmd: true, ..Default::default() }).unwrap();
        assert_eq!(r["command"], "format.bold");
        s.key("Right", Mods::default()).unwrap();
        s.key("Delete", Mods::default()).unwrap();
        assert_eq!(s.doc().unwrap().current_slide().unwrap().shapes.len(), 1);
    }

    fn cmd() -> Mods {
        Mods { cmd: true, ..Default::default() }
    }
    fn with(shift: bool, alt: bool, cmd: bool) -> Mods {
        Mods { shift, alt, cmd, control: false }
    }
    fn session(mac: bool) -> Session {
        let mut s = Session::with_new();
        s.mac_keys = mac;
        s
    }
    fn n_slides(s: &Session) -> usize {
        s.doc().unwrap().doc.slides.len()
    }
    fn rect(s: &mut Session, x: f64) -> u64 {
        s.execute("shape.insert", &json!({"preset": "rect", "rect": [x, 100, 100, 50]})).unwrap()["id"].as_u64().unwrap()
    }
    fn xfrm(s: &Session, id: u64) -> deckcraft_geom::Xfrm {
        s.doc().unwrap().shape(deckcraft_model::ShapeId(id as u32)).unwrap().xfrm.unwrap()
    }
    fn caret(s: &Session) -> (usize, usize) {
        s.doc().unwrap().selection.text.as_ref().unwrap().caret
    }

    /// egui reports punctuation by name (`Period`, `OpenBracket`): those keys reach the registry's
    /// `Cmd+Shift+.` and `Cmd+[`.
    #[test]
    fn punctuation_key_names_reach_their_shortcuts() {
        assert_eq!(key_token("Period"), ".");
        assert_eq!(key_token("OpenBracket"), "[");
        assert_eq!(key_token("ArrowLeft"), "left");
        assert_eq!(key_token("Return"), "enter");
        let mut s = session(false);
        let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
        s.execute("edit.select", &json!({"ids": [title]})).unwrap();
        assert_eq!(s.key("Period", with(true, false, true)).unwrap()["command"], "format.grow");
        assert_eq!(s.key("Comma", with(true, false, true)).unwrap()["command"], "format.shrink");
        assert_eq!(s.key("Equals", cmd()).unwrap()["command"], "format.subscript");
        assert_eq!(s.key("Equals", with(true, false, true)).unwrap()["command"], "format.superscript");
        let a = rect(&mut s, 10.0);
        rect(&mut s, 20.0);
        s.execute("edit.select", &json!({"ids": [a]})).unwrap();
        assert_eq!(s.key("CloseBracket", cmd()).unwrap()["command"], "arrange.bringForward");
        assert_eq!(s.key("OpenBracket", with(true, false, true)).unwrap()["command"], "arrange.sendToBack");
    }

    /// Windows and Mac keys for the same commands: Ctrl+M / Cmd+Shift+N new slide, Ctrl+G /
    /// Cmd+Option+G group, Cmd+Shift+F / B front and back (Mac), Ctrl+Shift+D duplicate slide.
    #[test]
    fn platform_shortcuts_follow_powerpoint() {
        let mut win = session(false);
        win.key("m", cmd()).unwrap();
        assert_eq!(n_slides(&win), 2, "Ctrl+M adds a slide on Windows");
        win.key("d", with(true, false, true)).unwrap();
        assert_eq!(n_slides(&win), 3, "Ctrl+Shift+D duplicates the slide");
        let a = rect(&mut win, 10.0);
        let b = rect(&mut win, 200.0);
        win.execute("edit.select", &json!({"ids": [a, b]})).unwrap();
        assert_eq!(win.key("g", cmd()).unwrap()["command"], "arrange.group");
        assert_eq!(win.key("g", with(true, false, true)).unwrap()["command"], "arrange.ungroup");
        assert_eq!(win.key("j", with(true, false, true)).unwrap()["command"], "arrange.regroup");

        let mut mac = session(true);
        assert_eq!(mac.key("m", cmd()).unwrap()["handled"], false, "Cmd+M is the system's Minimize on a Mac");
        mac.key("n", with(true, false, true)).unwrap();
        assert_eq!(n_slides(&mac), 2);
        let a = rect(&mut mac, 10.0);
        let b = rect(&mut mac, 200.0);
        mac.execute("edit.select", &json!({"ids": [a]})).unwrap();
        assert_eq!(mac.key("f", with(true, false, true)).unwrap()["command"], "arrange.bringToFront");
        let order = |s: &Session| s.doc().unwrap().current_slide().unwrap().shapes.iter().map(|x| x.id.0 as u64).collect::<Vec<_>>();
        assert_eq!(order(&mac).last(), Some(&a));
        assert_eq!(mac.key("b", with(true, true, true)).unwrap()["command"], "arrange.sendBackward");
        assert_eq!(order(&mac).last(), Some(&b));
        assert_eq!(mac.key("b", with(true, false, true)).unwrap()["command"], "arrange.sendToBack");
        mac.execute("edit.select", &json!({"ids": [a, b]})).unwrap();
        assert_eq!(mac.key("g", with(false, true, true)).unwrap()["command"], "arrange.group");
        mac.key("z", cmd()).unwrap();
        assert_eq!(mac.key("z", with(true, false, true)).unwrap()["command"], "edit.redo", "Cmd+Shift+Z redoes on a Mac");
    }

    /// Arrows nudge (6 pt; 1 pt with Ctrl/Cmd or Option), Alt+Left/Right rotate 15°, Shift+arrows
    /// resize.
    #[test]
    fn arrows_nudge_rotate_and_resize_objects() {
        let mut s = session(false);
        let id = rect(&mut s, 100.0);
        s.execute("edit.select", &json!({"ids": [id]})).unwrap();
        s.key("ArrowRight", Mods::default()).unwrap();
        assert_eq!(xfrm(&s, id).x, 106.0);
        s.key("ArrowLeft", cmd()).unwrap();
        assert_eq!(xfrm(&s, id).x, 105.0);
        s.key("ArrowDown", with(false, true, false)).unwrap();
        assert_eq!(xfrm(&s, id).y, 101.0);
        s.key("ArrowRight", with(false, true, false)).unwrap();
        assert_eq!(xfrm(&s, id).rot, 15.0);
        s.key("ArrowLeft", with(false, true, false)).unwrap();
        s.key("ArrowLeft", with(false, true, false)).unwrap();
        assert_eq!(xfrm(&s, id).rot, 345.0);
        s.key("ArrowRight", with(true, false, false)).unwrap();
        s.key("ArrowDown", with(true, false, false)).unwrap();
        let x = xfrm(&s, id);
        assert_eq!((x.w, x.h), (106.0, 44.0));
        s.key("ArrowUp", with(true, false, true)).unwrap();
        assert_eq!(xfrm(&s, id).h, 45.0);
    }

    /// Ctrl+arrows move by words and paragraphs on Windows; Cmd+arrows go to line and text ends
    /// on a Mac. Ctrl/Cmd+Backspace deletes a word.
    #[test]
    fn text_keys_follow_each_platform() {
        for mac in [false, true] {
            let mut s = session(mac);
            let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
            s.execute("edit.select", &json!({"ids": [title]})).unwrap();
            s.type_text("one two").unwrap();
            s.key("Enter", Mods::default()).unwrap();
            s.type_text("three").unwrap();
            s.key("ArrowUp", cmd()).unwrap();
            assert_eq!(caret(&s), if mac { (0, 0) } else { (1, 0) });
            s.key("Home", cmd()).unwrap();
            assert_eq!(caret(&s), (0, 0));
            s.key("ArrowRight", cmd()).unwrap();
            assert_eq!(caret(&s), if mac { (0, 7) } else { (0, 4) });
            s.key("End", cmd()).unwrap();
            assert_eq!(caret(&s), (1, 5));
            s.key("Backspace", cmd()).unwrap();
            assert_eq!(s.execute("text.get", &json!({"id": title})).unwrap()["text"], "one two\n");
            if !mac {
                s.key("ArrowUp", cmd()).unwrap();
                s.key("ArrowDown", cmd()).unwrap();
                assert_eq!(caret(&s), (1, 0), "Ctrl+Down: the next paragraph");
            }
        }
    }

    /// Promote and demote: Alt+Shift+Left/Right on Windows, Cmd+[ / Cmd+] on a Mac.
    #[test]
    fn paragraph_levels_from_the_keyboard() {
        let level = |s: &mut Session| {
            let t = s.doc().unwrap().selection.text.clone().unwrap();
            s.doc().unwrap().shape(t.shape).unwrap().text.as_ref().unwrap().paragraphs[0].level
        };
        for mac in [false, true] {
            let mut s = session(mac);
            let body = s.doc().unwrap().current_slide().unwrap().shapes[1].id.0;
            s.execute("edit.select", &json!({"ids": [body]})).unwrap();
            s.type_text("point").unwrap();
            let (demote, promote) = if mac {
                (("CloseBracket", cmd()), ("OpenBracket", cmd()))
            } else {
                (("ArrowRight", with(true, true, false)), ("ArrowLeft", with(true, true, false)))
            };
            let before = level(&mut s);
            s.key(demote.0, demote.1).unwrap();
            assert_eq!(level(&mut s), before + 1, "mac {mac}");
            s.key(promote.0, promote.1).unwrap();
            assert_eq!(level(&mut s), before, "mac {mac}");
        }
    }

    /// Ctrl/Cmd+Up/Down move the slide; with Shift, to the start or end.
    #[test]
    fn slides_move_from_the_keyboard() {
        let mut s = session(true);
        for t in ["b", "c", "d"] {
            s.execute("slide.new", &json!({"title": t})).unwrap();
        }
        let id = |s: &Session, i: usize| s.doc().unwrap().doc.slides[i].id;
        let moved = id(&s, 3);
        s.key("ArrowUp", cmd()).unwrap();
        assert_eq!(id(&s, 2), moved);
        s.key("ArrowUp", with(true, false, true)).unwrap();
        assert_eq!(id(&s, 0), moved);
        s.key("ArrowDown", with(true, false, true)).unwrap();
        assert_eq!(id(&s, 3), moved);
        s.key("ArrowDown", cmd()).unwrap();
        assert_eq!(id(&s, 3), moved, "already last");
    }

    /// Windows Ctrl+Enter: the next placeholder, then a new slide after the last one.
    #[test]
    fn ctrl_enter_goes_to_the_next_placeholder() {
        let mut s = session(false);
        let shapes: Vec<u32> = s.doc().unwrap().current_slide().unwrap().shapes.iter().map(|x| x.id.0).collect();
        s.execute("edit.select", &json!({"ids": [shapes[0]]})).unwrap();
        s.key("Enter", cmd()).unwrap();
        assert_eq!(s.doc().unwrap().selection.text.as_ref().unwrap().shape.0, shapes[1]);
        s.key("Enter", cmd()).unwrap();
        assert_eq!(n_slides(&s), 2, "after the last placeholder, a new slide");
        assert!(s.doc().unwrap().selection.text.is_some());
        // Ctrl+Alt+5 selects the first object.
        s.key("5", with(false, true, true)).unwrap();
        let first = s.doc().unwrap().current_slide().unwrap().shapes[0].id;
        assert_eq!(s.doc().unwrap().selection.shapes, vec![first]);
    }
}
