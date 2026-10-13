//! File menu: new, open, save, export, close, properties.

use deckcraft_model::{Presentation, defaults};
use serde_json::{Value, json};

use super::*;
use crate::{DocState, EngineError, Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(noundo "file.new", "New Presentation", ["File"], Some("Cmd+N"), "{theme?: name, size?: [w,h] pt, blank?: bool}", always, new),
        cmd!(noundo "file.open", "Open…", ["File"], Some("Cmd+O"), "{path, password?}", always, open),
        cmd!(noundo "file.openBytes", "Open Data", [], None, "{name, data: base64, password?}", always, open_bytes),
        cmd!(noundo "file.save", "Save", ["File"], Some("Cmd+S"), "{path?, format?: deckcraft|pptx}", has_doc, save),
        cmd!(noundo "file.saveAs", "Save As…", ["File"], Some("Cmd+Shift+S"), "{path, format?}", has_doc, save_as),
        cmd!(noundo "file.saveTemplate", "Save as Template…", ["File"], None, "{path}", has_doc, save_as),
        cmd!(query "file.saveBytes", "Save to Data", [], None, "{format?: deckcraft|pptx} → {data: base64}", has_doc, save_bytes_cmd),
        cmd!(noundo "file.export", "Export…", ["File"], None, "{path, format: png|jpeg|pptx|deckcraft|outline|pdf, slide?: index, all?: bool, scale?: px per pt; pdf: layout?: slides|notes|handouts, perPage?: 1|2|3|4|6|9, dpi?, slides?: [index] | \"1-3,5\", includeHidden?, textLayer?, frame?}", has_doc, export),
        cmd!(
            query "file.info",
            "Info",
            ["File"],
            None,
            "{} → {title, path, format, dirty, slides, hiddenSlides, sections, bytes?, properties}",
            has_doc,
            info
        ),
        cmd!(query "file.render", "Render Slide", [], None, "{slide?: index, scale?, edit?: bool} → {png: base64, width, height}", has_doc, render),
        cmd!(noundo "file.close", "Close", ["File"], Some("Cmd+W"), "{}", has_doc, close),
        cmd!(
            "file.properties",
            "Properties",
            ["File"],
            None,
            "{title?, subject?, author?, keywords?, comments?, category?, company?} → properties",
            has_doc,
            properties
        ),
        cmd!(noundo "file.recovery.save", "Save AutoRecover Information", [], None, "{} → {written}", has_doc, recovery_save),
        cmd!(query "file.recovery.list", "Recovered Presentations", [], None, "{} → [{uid, path, title, saved}]", always, recovery_list),
        cmd!(noundo "file.recovery.open", "Open Recovered Presentations", [], None, "{} → {opened: [document index]}", always, recovery_open),
        cmd!(noundo "file.recovery.discard", "Discard Recovered Presentations", [], None, "{uid?: one entry, else all}", always, recovery_discard),
        cmd!(noundo "file.revert", "Revert", [], None, "{}", has_doc, revert),
        cmd!(noundo "window.next", "Next Window", ["Window"], Some("Cmd+`"), "{index?}", has_doc, next_window),
    ]
}

fn theme_named(name: &str) -> Option<deckcraft_model::Theme> {
    deckcraft_model::theme::builtin_themes().into_iter().find(|t| t.name.eq_ignore_ascii_case(name))
}

fn new(s: &mut Session, p: &Value) -> Result<Value> {
    let theme = str_param(p, "theme").and_then(theme_named).unwrap_or_default();
    let size = p
        .get("size")
        .and_then(Value::as_array)
        .and_then(|a| Some(deckcraft_geom::Size::new(a.first()?.as_f64()?, a.get(1)?.as_f64()?)))
        .filter(|s| s.width >= 1.0 && s.height >= 1.0 && s.width < 5000.0 && s.height < 5000.0)
        .unwrap_or(defaults::WIDE);
    let doc = defaults::blank_presentation(size, theme, !bool_or(p, "blank", false));
    let title = s.next_untitled();
    let i = s.add_document(DocState::new(doc, None, title));
    Ok(json!({"document": i}))
}

/// Recognise and read presentation bytes (native, PPTX), optionally with a password.
pub fn open_presentation_with_password(name: &str, bytes: &[u8], password: Option<&str>) -> Result<Presentation> {
    if deckcraft_format::sniff(bytes) {
        return deckcraft_format::load(bytes).map_err(|e| EngineError::Other(e.to_string()));
    }
    if deckcraft_pptx::sniff(bytes) || deckcraft_pptx::is_encrypted(bytes) {
        let mut p = match deckcraft_pptx::import_with_password(bytes, password) {
            Ok(p) => p,
            Err(deckcraft_pptx::PptxError::PasswordRequired) => {
                return Err(EngineError::PasswordRequired(name.to_string()));
            }
            Err(deckcraft_pptx::PptxError::WrongPassword) => {
                return Err(EngineError::WrongPassword(name.to_string()));
            }
            Err(e) => return Err(EngineError::Other(e.to_string())),
        };
        deckcraft_format::repair(&mut p);
        return Ok(p);
    }
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".txt") || lower.ends_with(".md") {
        let text = String::from_utf8_lossy(bytes);
        let mut p = defaults::blank_presentation(defaults::WIDE, Default::default(), false);
        deckcraft_format::outline_to_slides(&mut p, &text);
        if p.slides.is_empty() {
            p = defaults::new_presentation(None);
        }
        return Ok(p);
    }
    if is_cfb(bytes) {
        // An OLE/CFB compound file that carries no `/EncryptionInfo` stream is a legacy binary
        // `.ppt` (or another application's document). It is not encrypted, so it must not be
        // answered with a password prompt.
        return Err(EngineError::Other(format!(
            "{name}: legacy binary PowerPoint (.ppt) files and other OLE compound documents are not supported — save as .pptx"
        )));
    }
    Err(EngineError::Other(format!("{name}: not a presentation DeckCraft can read")))
}

pub fn open_presentation(name: &str, bytes: &[u8]) -> Result<Presentation> {
    open_presentation_with_password(name, bytes, None)
}

/// Does this look like an OLE/CFB Compound File?  An encrypted Office document and a legacy
/// `.ppt` share this container, so the eight magic bytes alone say nothing about encryption.
pub fn is_cfb(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1])
}

/// Ask the UI to prompt for a password. Only raised when none was supplied: a password that was
/// just rejected is reported to the caller, which already has an open dialog holding the text the
/// user typed — pushing a fresh prompt would discard it.
fn password_prompt(s: &mut Session, name: &str, params: Value) -> Result<Value> {
    s.ui_requests.push(crate::UiRequest::Dialog { id: "password".into(), params });
    Err(EngineError::PasswordRequired(name.to_string()))
}

/// DeckCraft can read an encrypted presentation but not write one back, so a save would quietly
/// drop the protection the user relies on. Say so as the file opens, while it still matters.
fn warn_not_encrypted_on_save(s: &mut Session, name: &str) {
    let label = std::path::Path::new(name).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| name.to_string());
    s.ui_requests.push(crate::UiRequest::Message {
        text: format!("{label}: opened decrypted — DeckCraft can't encrypt on save yet, so saving it would drop the password"),
    });
}

pub fn add_opened(s: &mut Session, name: &str, path: Option<String>, p: Presentation) -> usize {
    let title = std::path::Path::new(name).file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| name.to_string());
    let missing_fonts = crate::fonts::missing_fonts(&p);
    let mut st = DocState::new(p, path, title);
    st.missing_fonts = missing_fonts;
    s.add_document(st)
}

#[cfg(not(target_arch = "wasm32"))]
fn open(s: &mut Session, p: &Value) -> Result<Value> {
    let path = str_param(p, "path").ok_or_else(|| bad("file.open", "missing `path`"))?;
    let password = str_param(p, "password");
    let bytes = std::fs::read(path).map_err(|e| EngineError::Other(format!("{path}: {e}")))?;
    let doc = match open_presentation_with_password(path, &bytes, password) {
        Ok(doc) => doc,
        Err(EngineError::PasswordRequired(name)) => {
            return password_prompt(s, &name, json!({"name": name, "path": path}));
        }
        Err(e) => return Err(e),
    };
    let native = deckcraft_format::sniff(&bytes) || deckcraft_pptx::sniff(&bytes) || deckcraft_pptx::is_encrypted(&bytes);
    if deckcraft_pptx::is_encrypted(&bytes) {
        warn_not_encrypted_on_save(s, path);
    }
    let i = add_opened(s, path, native.then(|| path.to_string()), doc);
    let st = s.doc()?;
    Ok(json!({"document": i, "slides": st.doc.slides.len(), "missingFonts": st.missing_fonts}))
}
#[cfg(target_arch = "wasm32")]
fn open(_s: &mut Session, _p: &Value) -> Result<Value> {
    Err(EngineError::Other("use file.openBytes in the browser".into()))
}

fn open_bytes(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").unwrap_or("Presentation");
    let password = str_param(p, "password");
    let data_str = str_param(p, "data").ok_or_else(|| bad("file.openBytes", "missing base64 `data`"))?;
    let data = base64_decode(data_str).ok_or_else(|| bad("file.openBytes", "invalid base64 `data`"))?;
    let doc = match open_presentation_with_password(name, &data, password) {
        Ok(doc) => doc,
        Err(EngineError::PasswordRequired(n)) => {
            return password_prompt(s, &n, json!({"name": n, "data": data_str}));
        }
        Err(e) => return Err(e),
    };
    let i = add_opened(s, name, None, doc);
    if deckcraft_pptx::is_encrypted(&data) {
        warn_not_encrypted_on_save(s, name);
    }
    Ok(json!({"document": i, "missingFonts": s.doc()?.missing_fonts}))
}

/// Encode the presentation in `format` (`deckcraft`, `pptx`, `outline`).
pub fn save_bytes(doc: &Presentation, format: &str) -> Result<Vec<u8>> {
    match format {
        "pptx" | "potx" | "ppsx" => deckcraft_pptx::export(doc).map_err(|e| EngineError::Other(e.to_string())),
        "outline" | "txt" => Ok(deckcraft_format::slides_to_outline(doc).into_bytes()),
        "pdf" => deckcraft_pdf::export(doc, &Default::default()).map_err(|e| EngineError::Other(e.to_string())),
        _ => deckcraft_format::save(doc).map_err(|e| EngineError::Other(e.to_string())),
    }
}

/// PDF options from command params: `{layout: slides|notes|handouts, perPage, dpi, slides,
/// includeHidden, textLayer, frame}` for a presentation of `count` slides. `slides` is a list of
/// 0-based indices or a range as typed in PowerPoint's Print page (`"1-3, 5"`, 1-based).
pub fn pdf_options(p: &Value, count: usize) -> Result<deckcraft_pdf::PdfOptions> {
    let mut o = deckcraft_pdf::PdfOptions { layout: deckcraft_pdf::PageLayout::Slides, ..Default::default() };
    o.layout = match str_param(p, "layout").unwrap_or("slides") {
        "notes" => deckcraft_pdf::PageLayout::Notes,
        "handouts" => deckcraft_pdf::PageLayout::Handouts { per_page: usize_param(p, "perPage").unwrap_or(6).clamp(1, 9) as u8 },
        _ => deckcraft_pdf::PageLayout::Slides,
    };
    o.dpi = f64_or(p, "dpi", o.dpi).clamp(36.0, 600.0);
    o.slides = match p.get("slides") {
        Some(Value::Array(a)) => Some(a.iter().filter_map(|v| v.as_u64().map(|x| x as usize)).filter(|i| *i < count).collect()),
        Some(Value::String(text)) if !text.trim().is_empty() => Some(parse_slide_range(text, count)?),
        _ => None,
    };
    o.include_hidden = bool_or(p, "includeHidden", false);
    o.text_layer = bool_or(p, "textLayer", true);
    o.frame_slides = bool_or(p, "frame", true);
    Ok(o)
}

/// A slide range as typed in the Print page's Slides box (`"1-3, 5"`; 1-based, either separator
/// `,` or `;`, ranges either way round) → 0-based indices in order, without repeats. Numbers past
/// the last slide or a range that isn't numbers is an error, as in PowerPoint.
pub fn parse_slide_range(text: &str, count: usize) -> Result<Vec<usize>> {
    let invalid = || bad("file.export", format!("“{text}” isn't a valid slide range: use numbers and ranges such as 1-3, 5"));
    let mut out: Vec<usize> = vec![];
    for part in text.split([',', ';']).map(str::trim).filter(|p| !p.is_empty()) {
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => (a.trim(), b.trim()),
            None => (part, part),
        };
        let (a, b) = (a.parse::<usize>().map_err(|_| invalid())?, b.parse::<usize>().map_err(|_| invalid())?);
        let (lo, hi) = (a.min(b), a.max(b));
        if lo == 0 || hi > count {
            return Err(bad("file.export", format!("the presentation has {count} slides; “{part}” is outside them")));
        }
        for i in lo - 1..hi {
            if !out.contains(&i) {
                out.push(i);
            }
        }
    }
    if out.is_empty() {
        return Err(invalid());
    }
    Ok(out)
}

/// PDF bytes for `doc` with [`pdf_options`] from `p`.
pub fn pdf_bytes(doc: &Presentation, p: &Value) -> Result<Vec<u8>> {
    deckcraft_pdf::export(doc, &pdf_options(p, doc.slides.len())?).map_err(|e| EngineError::Other(e.to_string()))
}

pub fn format_for_path(path: &str) -> &'static str {
    let l = path.to_ascii_lowercase();
    if l.ends_with(".pptx") || l.ends_with(".potx") || l.ends_with(".ppsx") {
        "pptx"
    } else if l.ends_with(".png") {
        "png"
    } else if l.ends_with(".jpg") || l.ends_with(".jpeg") {
        "jpeg"
    } else if l.ends_with(".pdf") {
        "pdf"
    } else if l.ends_with(".txt") || l.ends_with(".md") {
        "outline"
    } else {
        "deckcraft"
    }
}

fn mark_saved(s: &mut Session, path: Option<String>) -> Result<()> {
    let st = s.doc_mut()?;
    st.saved_doc = st.doc.clone();
    if path.is_some() {
        st.path = path;
    }
    st.revision += 1;
    let uid = st.uid;
    if let Some(dir) = &s.recovery_dir {
        crate::recovery::discard(dir, uid);
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn write_file(path: &str, bytes: &[u8]) -> Result<()> {
    // Write to a temporary sibling then rename, so a failed save never truncates the old file.
    let tmp = format!("{path}.saving");
    std::fs::write(&tmp, bytes).map_err(|e| EngineError::Other(format!("{path}: {e}")))?;
    std::fs::rename(&tmp, path).map_err(|e| EngineError::Other(format!("{path}: {e}")))
}
#[cfg(target_arch = "wasm32")]
fn write_file(_path: &str, _bytes: &[u8]) -> Result<()> {
    Err(EngineError::Other("saving to a path is not available in the browser".into()))
}

fn save(s: &mut Session, p: &Value) -> Result<Value> {
    let path = str_param(p, "path").map(String::from).or_else(|| s.active().and_then(|d| d.path.clone()));
    let Some(path) = path else {
        s.ui_requests.push(crate::UiRequest::PickFile { purpose: "saveAs".into() });
        return Ok(json!({"needsPath": true}));
    };
    let format = str_param(p, "format").unwrap_or(format_for_path(&path));
    let bytes = save_bytes(&s.doc()?.doc, format)?;
    write_file(&path, &bytes)?;
    mark_saved(s, Some(path.clone()))?;
    Ok(json!({"path": path, "bytes": bytes.len()}))
}

fn save_as(s: &mut Session, p: &Value) -> Result<Value> {
    if str_param(p, "path").is_none() {
        s.ui_requests.push(crate::UiRequest::PickFile { purpose: "saveAs".into() });
        return Ok(json!({"needsPath": true}));
    }
    save(s, p)
}

fn save_bytes_cmd(s: &mut Session, p: &Value) -> Result<Value> {
    let format = str_param(p, "format").unwrap_or("deckcraft");
    let bytes = save_bytes(&s.doc()?.doc, format)?;
    Ok(json!({"data": base64_encode(&bytes), "bytes": bytes.len()}))
}

/// Render slide `index` (current when absent) to PNG bytes.
pub fn render_png(doc: &Presentation, index: usize, scale: f64, edit: bool) -> (Vec<u8>, u32, u32) {
    let img =
        deckcraft_render::render_slide(doc, index, &deckcraft_render::RenderOpts { scale: scale.clamp(0.01, 16.0), edit, ..Default::default() });
    (img.to_png(), img.width, img.height)
}

fn render(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let i = usize_param(p, "slide").unwrap_or(st.selection.slide);
    if i >= st.doc.slides.len() {
        return Err(bad("file.render", format!("no slide {i}")));
    }
    let (png, w, h) = render_png(&st.doc, i, f64_or(p, "scale", 1.0), bool_or(p, "edit", false));
    Ok(json!({"png": base64_encode(&png), "width": w, "height": h}))
}

fn export(s: &mut Session, p: &Value) -> Result<Value> {
    let path = str_param(p, "path").ok_or_else(|| bad("file.export", "missing `path`"))?.to_string();
    let format = str_param(p, "format").unwrap_or(format_for_path(&path)).to_string();
    let st = s.doc()?;
    let scale = f64_or(p, "scale", 2.0);
    match format.as_str() {
        "png" | "jpeg" | "jpg" => {
            let slides: Vec<usize> = match usize_param(p, "slide") {
                Some(i) => vec![i],
                None if bool_or(p, "all", false) => (0..st.doc.slides.len()).collect(),
                None => vec![st.selection.slide],
            };
            let mut written = vec![];
            for (k, i) in slides.iter().enumerate() {
                if *i >= st.doc.slides.len() {
                    return Err(bad("file.export", format!("no slide {i}")));
                }
                let img = deckcraft_render::render_slide(
                    &st.doc,
                    *i,
                    &deckcraft_render::RenderOpts { scale: scale.clamp(0.01, 16.0), ..Default::default() },
                );
                let bytes = if format == "png" { img.to_png() } else { img.to_jpeg(92) };
                let out = if slides.len() == 1 {
                    path.clone()
                } else {
                    let pth = std::path::Path::new(&path);
                    let stem = pth.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "Slide".into());
                    let ext = pth.extension().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| format.clone());
                    pth.with_file_name(format!("{stem}{}.{ext}", k + 1)).to_string_lossy().to_string()
                };
                write_file(&out, &bytes)?;
                written.push(out);
            }
            Ok(json!({"files": written}))
        }
        "pdf" => {
            let bytes = pdf_bytes(&st.doc, p)?;
            write_file(&path, &bytes)?;
            Ok(json!({"path": path, "bytes": bytes.len()}))
        }
        other => {
            let bytes = save_bytes(&st.doc, other)?;
            write_file(&path, &bytes)?;
            Ok(json!({"path": path, "bytes": bytes.len()}))
        }
    }
}

/// File › Info: what the Info page shows about the active presentation.
fn info(s: &mut Session, _p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let doc = &st.doc;
    #[cfg(not(target_arch = "wasm32"))]
    let bytes = st.path.as_ref().and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len());
    #[cfg(target_arch = "wasm32")]
    let bytes: Option<u64> = None;
    Ok(json!({
        "title": st.title(),
        "path": st.path,
        "format": st.path.as_deref().map(format_for_path),
        "dirty": st.is_dirty(),
        "slides": doc.slides.len(),
        "hiddenSlides": doc.slides.iter().filter(|sl| sl.hidden).count(),
        "sections": doc.sections.len(),
        "bytes": bytes,
        "properties": serde_json::to_value(&doc.props).unwrap_or_default(),
    }))
}

fn close(s: &mut Session, _p: &Value) -> Result<Value> {
    if let Some(i) = s.active_index() {
        s.close_document(i);
    }
    ok()
}

fn properties(s: &mut Session, p: &Value) -> Result<Value> {
    let fields = ["title", "subject", "author", "keywords", "comments", "category", "company"];
    if fields.iter().any(|f| p.get(*f).is_some()) {
        s.edit(|doc, _| {
            let pr = &mut doc.props;
            for f in fields {
                if let Some(v) = str_param(p, f) {
                    let slot = match f {
                        "title" => &mut pr.title,
                        "subject" => &mut pr.subject,
                        "author" => &mut pr.author,
                        "keywords" => &mut pr.keywords,
                        "comments" => &mut pr.comments,
                        "category" => &mut pr.category,
                        _ => &mut pr.company,
                    };
                    *slot = v.to_string();
                }
            }
            Ok(())
        })?;
    }
    Ok(serde_json::to_value(&s.doc()?.doc.props).unwrap_or_default())
}

fn revert(s: &mut Session, _p: &Value) -> Result<Value> {
    let st = s.doc_mut()?;
    st.doc = st.saved_doc.clone();
    st.history = crate::History { limit: st.history.limit, ..Default::default() };
    st.revision += 1;
    ok()
}

fn next_window(s: &mut Session, p: &Value) -> Result<Value> {
    let n = s.documents().len();
    if n == 0 {
        return ok();
    }
    let i = usize_param(p, "index").unwrap_or_else(|| (s.active_index().unwrap_or(0) + 1) % n);
    s.set_active(i.min(n - 1));
    ok()
}

fn recovery_dir(s: &Session) -> Result<std::path::PathBuf> {
    s.recovery_dir.clone().ok_or_else(|| EngineError::Other("no recovery folder is set".into()))
}

fn recovery_save(s: &mut Session, _p: &Value) -> Result<Value> {
    let dir = recovery_dir(s)?;
    Ok(json!({"written": crate::recovery::save(s, &dir)?}))
}

fn recovery_list(s: &mut Session, _p: &Value) -> Result<Value> {
    let dir = recovery_dir(s)?;
    Ok(Value::Array(
        crate::recovery::list(&dir)
            .into_iter()
            .map(|(uid, mut m)| {
                if let Some(o) = m.as_object_mut() {
                    o.insert("uid".into(), json!(uid));
                }
                m
            })
            .collect(),
    ))
}

fn recovery_open(s: &mut Session, _p: &Value) -> Result<Value> {
    let dir = recovery_dir(s)?;
    Ok(json!({"opened": crate::recovery::open(s, &dir)?}))
}

fn recovery_discard(s: &mut Session, p: &Value) -> Result<Value> {
    let dir = recovery_dir(s)?;
    match p.get("uid").and_then(Value::as_u64) {
        Some(uid) => crate::recovery::discard(&dir, uid),
        None => {
            for (uid, _) in crate::recovery::list(&dir) {
                crate::recovery::discard(&dir, uid);
            }
        }
    }
    ok()
}
