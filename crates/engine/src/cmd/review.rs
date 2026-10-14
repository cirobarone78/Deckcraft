//! Review tab: comments, spelling, accessibility; Slide Show requests.

use std::sync::Arc;

use deckcraft_geom::Xfrm;
use deckcraft_model::{Comment, InkStroke, LaserSample, LaserTrace, MediaClip, Rgba, Shape, ShapeId, ShapeKind};
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session, UiRequest};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!("comment.add", "New Comment", ["Review", "Comments"], Some("Cmd+Alt+M"), "{text, x?, y?, id?: shape}", has_slide, add),
        cmd!("comment.reply", "Reply", ["Comments"], None, "{index, text}", has_slide, reply),
        cmd!("comment.delete", "Delete Comment", ["Review", "Comments"], None, "{index? | all?: bool}", has_slide, delete),
        cmd!("comment.resolve", "Resolve Thread", ["Comments"], None, "{index, resolved?: bool}", has_slide, resolve),
        cmd!(query "comment.list", "Comments", [], None, "{slide?} → comments", has_slide, list),
        cmd!(query "review.accessibility", "Check Accessibility", ["Review", "Accessibility"], None, "{} → [{slide, shape, issue}]", has_doc, accessibility),
        cmd!(query "review.spelling", "Spelling", ["Review", "Proofing"], Some("F7"), "{} → [{slide, shape, word}] (words not in the built-in list)", has_doc, spelling),
        cmd!(noundo "show.fromStart", "Play from Start", ["Slide Show", "Start Slide Show"], Some("Cmd+Shift+Enter"), "{}", has_slide, |s, _| start(s, 0)),
        cmd!(noundo "show.fromCurrent", "Play from Current Slide", ["Slide Show", "Start Slide Show"], Some("Cmd+Enter"), "{}", has_slide, |s, _| {
            let i = s.doc()?.selection.slide;
            start(s, i)
        }),
        cmd!(
            "show.setup",
            "Set Up Slide Show…",
            ["Slide Show", "Set Up"],
            None,
            "{type?: speaker|browsed|kiosk, loop?: bool, noNarration?, noAnimation?, useTimings?, from?, to?}",
            has_doc,
            setup
        ),
        cmd!(
            "show.customShow",
            "Custom Slide Show",
            ["Slide Show", "Start Slide Show"],
            None,
            "{name, slides: [index]} | {name, delete: true}",
            has_doc,
            custom_show
        ),
        cmd!(
            noundo "show.record",
            "Record Slide Show",
            ["Slide Show", "Record"],
            None,
            "{from?: index} → starts the show and keeps timings, narration, pen ink and laser",
            has_slide,
            record_show
        ),
        cmd!(
            "ink.stroke",
            "Record Ink",
            ["Slide Show", "Record"],
            None,
            "{points: [[x, y, pressure?]], index?: slide, width?, color?: [r, g, b], highlighter?}",
            has_slide,
            ink_stroke
        ),
        cmd!(
            "show.narration",
            "Record Narration",
            ["Slide Show", "Record"],
            None,
            "{data: base64 wav | path, name?, index?: slide} → replaces the slide's Narration audio",
            has_slide,
            narration
        ),
        cmd!(
            "show.laser",
            "Record Laser",
            ["Slide Show", "Record"],
            None,
            "{points: [[x, y, tMs]], index?: slide, color?: [r, g, b]} → replaces the slide's laser trace",
            has_slide,
            laser
        ),
    ]
}

fn now() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let (y, m, d) = super::design::civil((secs / 86_400) as i64);
        let t = secs % 86_400;
        format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", t / 3600, t / 60 % 60, t % 60)
    }
    #[cfg(target_arch = "wasm32")]
    {
        String::new()
    }
}

fn add(s: &mut Session, p: &Value) -> Result<Value> {
    let text = str_param(p, "text").ok_or_else(|| bad("comment.add", "missing `text`"))?.to_string();
    let author = s.prefs.author.clone();
    let shape = id_param(p, "id").or_else(|| s.active().and_then(|d| d.selection.shapes.first().copied()));
    let (x, y) = (f64_or(p, "x", 10.0), f64_or(p, "y", 10.0));
    s.edit(|doc, sel| {
        let sl = Arc::make_mut(doc.slides.get_mut(sel.slide).ok_or_else(|| bad("comment.add", "no slide"))?);
        let initials: String = author.split_whitespace().filter_map(|w| w.chars().next()).collect();
        sl.comments.push(Comment { author: author.clone(), initials, text, date: now(), x, y, resolved: false, replies: vec![], shape });
        Ok(json!({"index": sl.comments.len() - 1}))
    })
}

fn reply(s: &mut Session, p: &Value) -> Result<Value> {
    let i = usize_param(p, "index").ok_or_else(|| bad("comment.reply", "missing `index`"))?;
    let text = str_param(p, "text").unwrap_or("").to_string();
    let author = s.prefs.author.clone();
    s.edit(|doc, sel| {
        let sl = Arc::make_mut(doc.slides.get_mut(sel.slide).ok_or_else(|| bad("comment.reply", "no slide"))?);
        let c = sl.comments.get_mut(i).ok_or_else(|| bad("comment.reply", "no such comment"))?;
        c.replies.push(Comment { author, text, date: now(), ..Default::default() });
        Ok(())
    })?;
    ok()
}

fn delete(s: &mut Session, p: &Value) -> Result<Value> {
    let all = bool_or(p, "all", false);
    let i = usize_param(p, "index");
    s.edit(|doc, sel| {
        if all {
            for sl in &mut doc.slides {
                Arc::make_mut(sl).comments.clear();
            }
            return Ok(());
        }
        let sl = Arc::make_mut(doc.slides.get_mut(sel.slide).ok_or_else(|| bad("comment.delete", "no slide"))?);
        match i {
            Some(i) if i < sl.comments.len() => {
                sl.comments.remove(i);
            }
            None => sl.comments.clear(),
            _ => return Err(bad("comment.delete", "no such comment")),
        }
        Ok(())
    })?;
    ok()
}

fn resolve(s: &mut Session, p: &Value) -> Result<Value> {
    let i = usize_param(p, "index").ok_or_else(|| bad("comment.resolve", "missing `index`"))?;
    let v = bool_or(p, "resolved", true);
    s.edit(|doc, sel| {
        let sl = Arc::make_mut(doc.slides.get_mut(sel.slide).ok_or_else(|| bad("comment.resolve", "no slide"))?);
        sl.comments.get_mut(i).ok_or_else(|| bad("comment.resolve", "no such comment"))?.resolved = v;
        Ok(())
    })?;
    ok()
}

fn list(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let i = usize_param(p, "slide").unwrap_or(st.selection.slide);
    Ok(serde_json::to_value(st.doc.slides.get(i).map(|x| x.comments.clone()).unwrap_or_default()).unwrap_or_default())
}

fn accessibility(s: &mut Session, _p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let mut out = vec![];
    for (i, sl) in st.doc.slides.iter().enumerate() {
        if sl.title().trim().is_empty() {
            out.push(json!({"slide": i, "issue": "Missing slide title"}));
        }
        deckcraft_model::walk(&sl.shapes, &mut |sh, _| {
            let needs_alt = matches!(
                sh.kind,
                deckcraft_model::ShapeKind::Picture { .. }
                    | deckcraft_model::ShapeKind::Chart(_)
                    | deckcraft_model::ShapeKind::Media(_)
                    | deckcraft_model::ShapeKind::Group { .. }
            );
            if needs_alt && sh.descr.trim().is_empty() && !sh.decorative {
                out.push(json!({"slide": i, "shape": sh.id, "name": sh.name, "issue": "Missing alternative text"}));
            }
        });
        // Low-contrast text against the slide background.
        if let Some(ctx) = deckcraft_model::resolve::Ctx::for_slide(&st.doc, sl) {
            let bg = match deckcraft_model::resolve::background(&ctx, Some(sl)) {
                (deckcraft_model::Fill::Solid { color }, ph) => Some(ctx.color(&color, ph)),
                _ => None,
            };
            if let Some(bg) = bg {
                for sh in &sl.shapes {
                    let Some(t) = &sh.text else { continue };
                    let Some(para) = t.paragraphs.iter().find(|p| !p.is_empty()) else { continue };
                    let rp = deckcraft_model::resolve::run(&ctx, sh, para, &para.runs.first().map(|r| r.props.clone()).unwrap_or_default());
                    let (fill, _) = deckcraft_model::resolve::fill(&ctx, sh);
                    if fill.as_ref().is_some_and(|f| !f.is_none()) {
                        continue;
                    }
                    let fg = deckcraft_model::resolve::text_color(&ctx, &rp);
                    if fg.contrast(bg) < 3.0 {
                        out.push(json!({"slide": i, "shape": sh.id, "name": sh.name, "issue": format!("Hard-to-read text contrast ({:.1}:1)", fg.contrast(bg))}));
                    }
                }
            }
        }
    }
    Ok(Value::Array(out))
}

fn spelling(s: &mut Session, _p: &Value) -> Result<Value> {
    // A small built-in list of frequent misspellings (full dictionaries come later).
    const COMMON: &[&str] = &[
        "teh",
        "recieve",
        "seperate",
        "occured",
        "untill",
        "wich",
        "becuase",
        "definately",
        "thier",
        "alot",
        "accomodate",
        "acheive",
        "adress",
        "begining",
        "beleive",
        "calender",
        "commited",
        "enviroment",
        "goverment",
        "independant",
        "neccessary",
        "occassion",
        "publically",
        "reccomend",
        "tommorow",
        "wierd",
    ];
    let st = s.doc()?;
    let mut out = vec![];
    for (i, sl) in st.doc.slides.iter().enumerate() {
        deckcraft_model::walk(&sl.shapes, &mut |sh, _| {
            if let Some(t) = &sh.text {
                for w in t.text().split(|c: char| !c.is_alphanumeric() && c != '\'') {
                    if COMMON.contains(&w.to_lowercase().as_str()) {
                        out.push(json!({"slide": i, "shape": sh.id, "word": w}));
                    }
                }
            }
        });
    }
    Ok(Value::Array(out))
}

fn start(s: &mut Session, from: usize) -> Result<Value> {
    s.ui_requests.push(UiRequest::StartShow { from, record: false });
    Ok(json!({"from": from}))
}

fn record_show(s: &mut Session, p: &Value) -> Result<Value> {
    let n = s.doc()?.doc.slides.len();
    if n == 0 {
        return Err(bad("show.record", "no slides"));
    }
    let from = usize_param(p, "from").unwrap_or(0).min(n - 1);
    s.ui_requests.push(UiRequest::StartShow { from, record: true });
    Ok(json!({"from": from, "record": true}))
}

fn setup(s: &mut Session, p: &Value) -> Result<Value> {
    s.edit(|doc, _| {
        let sh = &mut doc.show;
        if let Some(t) = str_param(p, "type") {
            sh.show_type = t.to_string();
            if t == "kiosk" {
                sh.loop_until_esc = true;
            }
        }
        if let Some(v) = bool_param(p, "loop") {
            sh.loop_until_esc = v;
        }
        if let Some(v) = bool_param(p, "noNarration") {
            sh.without_narration = v;
        }
        if let Some(v) = bool_param(p, "noAnimation") {
            sh.without_animation = v;
        }
        if let Some(v) = bool_param(p, "useTimings") {
            sh.use_timings = v;
        }
        match (p.get("from").and_then(Value::as_u64), p.get("to").and_then(Value::as_u64)) {
            (Some(a), Some(b)) => sh.range = Some((a.min(u32::MAX as u64) as u32, b.min(u32::MAX as u64) as u32)),
            (None, None) if p.get("all").is_some() => sh.range = None,
            _ => {}
        }
        Ok(serde_json::to_value(&*sh).unwrap_or_default())
    })
}

fn custom_show(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").ok_or_else(|| bad("show.customShow", "missing `name`"))?.to_string();
    let delete = bool_or(p, "delete", false);
    let idx: Vec<usize> =
        p.get("slides").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(|v| v as usize).collect()).unwrap_or_default();
    s.edit(|doc, _| {
        doc.custom_shows.retain(|c| c.name != name);
        if !delete {
            let slides = idx.iter().filter_map(|i| doc.slides.get(*i).map(|x| x.id)).collect();
            doc.custom_shows.push(deckcraft_model::CustomShow { name: name.clone(), slides });
        }
        Ok(())
    })?;
    ok()
}

fn ink_stroke(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let index = usize_param(p, "index").unwrap_or(st.selection.slide);
    if index >= st.doc.slides.len() {
        return Err(bad("ink.stroke", format!("no slide {index}")));
    }
    let points = ink_points(p)?;
    if points.len() < 2 {
        return Err(bad("ink.stroke", "need at least two points"));
    }
    let width = p.get("width").and_then(Value::as_f64).filter(|w| w.is_finite()).unwrap_or(2.25).clamp(0.25, 40.0);
    let color = ink_color(p).unwrap_or(st.doc.show.pen_color);
    let highlighter = bool_or(p, "highlighter", false);
    let n = points.len();
    let stroke = InkStroke { points, color, width, highlighter };
    s.edit(|doc, _| {
        let Some(sl) = doc.slides.get_mut(index) else {
            return Err(bad("ink.stroke", format!("no slide {index}")));
        };
        let existing = Arc::make_mut(sl).shapes.iter().rev().find(|sh| matches!(sh.kind, ShapeKind::Ink { .. })).map(|sh| sh.id);
        let id = if let Some(id) = existing {
            id
        } else {
            let id = ShapeId(doc.alloc_id());
            let size = doc.slide_size;
            let Some(sl) = doc.slides.get_mut(index) else {
                return Err(bad("ink.stroke", format!("no slide {index}")));
            };
            Arc::make_mut(sl).shapes.push(Shape {
                id,
                name: "Ink".into(),
                xfrm: Some(Xfrm::new(0.0, 0.0, size.width, size.height)),
                kind: ShapeKind::Ink { strokes: vec![] },
                ..Default::default()
            });
            id
        };
        let Some(sl) = doc.slides.get_mut(index) else {
            return Err(bad("ink.stroke", format!("no slide {index}")));
        };
        if let Some(sh) = deckcraft_model::find_shape_mut(&mut Arc::make_mut(sl).shapes, id)
            && let ShapeKind::Ink { strokes } = &mut sh.kind
        {
            strokes.push(stroke);
        }
        Ok(())
    })?;
    Ok(json!({"index": index, "points": n}))
}

/// Up to 4096 finite points. A missing pressure is 0.5.
fn ink_points(p: &Value) -> Result<Vec<(f64, f64, f32)>> {
    let raw = p.get("points").and_then(Value::as_array).ok_or_else(|| bad("ink.stroke", "missing `points`"))?;
    let mut out = Vec::new();
    for pt in raw.iter().take(4096) {
        let Some(a) = pt.as_array() else { continue };
        let (Some(x), Some(y)) = (a.first().and_then(Value::as_f64), a.get(1).and_then(Value::as_f64)) else { continue };
        if !x.is_finite() || !y.is_finite() || x.abs() > 1.0e7 || y.abs() > 1.0e7 {
            continue;
        }
        let pressure = a.get(2).and_then(Value::as_f64).filter(|v| v.is_finite()).unwrap_or(0.5).clamp(0.0, 1.0);
        out.push((x, y, pressure as f32));
    }
    Ok(out)
}

fn ink_color(p: &Value) -> Option<Rgba> {
    let a = p.get("color")?.as_array()?;
    let byte = |i: usize| -> Option<u8> {
        let v = a.get(i)?;
        let n = v.as_u64().or_else(|| v.as_f64().filter(|x| x.is_finite()).map(|x| x.round() as u64))?;
        Some(n.min(255) as u8)
    };
    Some(Rgba::rgb(byte(0)?, byte(1)?, byte(2)?))
}

/// Embed a WAV as the slide's narration. A later recording replaces the shape named Narration.
fn narration(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let index = usize_param(p, "index").unwrap_or(st.selection.slide);
    if index >= st.doc.slides.len() {
        return Err(bad("show.narration", format!("no slide {index}")));
    }
    let (name, bytes) = super::insert::media_bytes(p, "show.narration")?;
    if bytes.len() > 48_000_000 {
        return Err(bad("show.narration", "audio is too long"));
    }
    if bytes.len() < 44 {
        return Err(bad("show.narration", "audio is too short"));
    }
    let name = if name.is_empty() || name == "media" { "narration.wav".to_string() } else { name };
    let ct = super::insert::content_type(&name, &bytes);
    if !ct.starts_with("audio/") {
        return Err(bad("show.narration", format!("{name} is not an audio file")));
    }
    let owned = Arc::new(bytes);
    let info = deckcraft_media::probe(&owned).map_err(|e| bad("show.narration", e.to_string()))?;
    if info.audio.is_none() {
        return Err(bad("show.narration", format!("{name} has no audio")));
    }
    let duration_ms = info.duration_ms.min(u32::MAX as u64) as u32;
    if duration_ms < 200 {
        return Err(bad("show.narration", "shorter than 0.2 s"));
    }
    let data = Arc::try_unwrap(owned).unwrap_or_else(|a| (*a).clone());
    let bytes_len = data.len();
    let id = s.edit(|doc, _| {
        let media = doc.add_media(&name, ct, data);
        let size = doc.slide_size;
        let rect = Xfrm::new((size.width - 36.0).max(0.0), (size.height - 36.0).max(0.0), 32.0, 32.0);
        let clip = MediaClip {
            media,
            video: false,
            volume: 1.0,
            autoplay: true,
            narration: true,
            hide_while_not_playing: true,
            duration_ms,
            ..Default::default()
        };
        let existing = doc.slides.get(index).and_then(|sl| sl.shapes.iter().find(|sh| sh.name == "Narration").map(|sh| sh.id));
        let id = if let Some(id) = existing {
            id
        } else {
            let id = ShapeId(doc.alloc_id());
            let Some(sl) = doc.slides.get_mut(index) else {
                return Err(bad("show.narration", format!("no slide {index}")));
            };
            Arc::make_mut(sl).shapes.push(Shape {
                id,
                name: "Narration".into(),
                xfrm: Some(rect),
                kind: ShapeKind::Media(MediaClip::default()),
                hidden: true,
                ..Default::default()
            });
            id
        };
        let Some(sl) = doc.slides.get_mut(index) else {
            return Err(bad("show.narration", format!("no slide {index}")));
        };
        if let Some(sh) = deckcraft_model::find_shape_mut(&mut Arc::make_mut(sl).shapes, id) {
            sh.name = "Narration".into();
            sh.hidden = true;
            sh.xfrm = Some(rect);
            sh.kind = ShapeKind::Media(clip);
        }
        Ok(id.0)
    })?;
    Ok(json!({"index": index, "id": id, "bytes": bytes_len, "durationMs": duration_ms}))
}

/// Replace the slide's recorded laser trace.
fn laser(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let index = usize_param(p, "index").unwrap_or(st.selection.slide);
    if index >= st.doc.slides.len() {
        return Err(bad("show.laser", format!("no slide {index}")));
    }
    let points = laser_points(p)?;
    if points.is_empty() {
        return Err(bad("show.laser", "need at least one point"));
    }
    let color = ink_color(p).unwrap_or(st.doc.show.laser_color);
    let n = points.len();
    let trace = LaserTrace { color, points };
    s.edit(|doc, _| {
        let Some(sl) = doc.slides.get_mut(index) else {
            return Err(bad("show.laser", format!("no slide {index}")));
        };
        Arc::make_mut(sl).laser = Some(trace);
        Ok(())
    })?;
    Ok(json!({"index": index, "points": n}))
}

fn laser_points(p: &Value) -> Result<Vec<LaserSample>> {
    let raw = p.get("points").and_then(Value::as_array).ok_or_else(|| bad("show.laser", "missing `points`"))?;
    let mut out = Vec::new();
    for pt in raw.iter().take(4096) {
        let Some(a) = pt.as_array() else { continue };
        let (Some(x), Some(y)) = (a.first().and_then(Value::as_f64), a.get(1).and_then(Value::as_f64)) else { continue };
        if !x.is_finite() || !y.is_finite() || x.abs() > 1.0e7 || y.abs() > 1.0e7 {
            continue;
        }
        let t = a
            .get(2)
            .and_then(|v| v.as_u64().or_else(|| v.as_f64().filter(|x| x.is_finite() && *x >= 0.0).map(|x| x.round().clamp(0.0, 3_600_000.0) as u64)))
            .unwrap_or(0);
        out.push(LaserSample { x, y, t_ms: t.min(3_600_000) as u32 });
    }
    out.sort_by_key(|p| p.t_ms);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use crate::sample::open_sample;

    #[test]
    fn record_requests_a_recording_show() {
        let mut s = Session::new();
        open_sample(&mut s).unwrap();
        let v = s.execute("show.record", &json!({"from": 2})).unwrap();
        assert_eq!(v["from"], 2);
        assert_eq!(v["record"], true);
        assert!(matches!(s.ui_requests.last(), Some(UiRequest::StartShow { from: 2, record: true })));
    }

    #[test]
    fn recorded_ink_joins_one_layer_and_undoes() {
        let mut s = Session::new();
        open_sample(&mut s).unwrap();
        s.execute(
            "ink.stroke",
            &json!({"index": 0, "points": [[10.0, 20.0, 0.8], [30.0, 40.0, 0.2], [null, 1.0], [1.0e20, 0.0]], "width": 2.0, "color": [200, 30, 30]}),
        )
        .unwrap();
        s.execute("ink.stroke", &json!({"index": 0, "points": [[50.0, 60.0], [70.0, 80.0]]})).unwrap();
        let strokes = ink_on(&s, 0);
        assert_eq!(strokes.len(), 2);
        assert_eq!(strokes[0].points, vec![(10.0, 20.0, 0.8), (30.0, 40.0, 0.2)]);
        assert_eq!(strokes[0].color, Rgba::rgb(200, 30, 30));
        assert!((strokes[0].width - 2.0).abs() < 1e-9);
        assert_eq!(strokes[1].points[0].2, 0.5);
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(ink_on(&s, 0).len(), 1);
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(ink_on(&s, 0).is_empty());
    }

    #[test]
    fn recorded_ink_rejects_a_short_or_missing_stroke() {
        let mut s = Session::new();
        open_sample(&mut s).unwrap();
        assert!(s.execute("ink.stroke", &json!({"points": [[1.0, 2.0]]})).is_err());
        assert!(s.execute("ink.stroke", &json!({"points": [[1.0e20, 0.0], ["no", 2.0]]})).is_err());
        assert!(s.execute("ink.stroke", &json!({"index": 99, "points": [[0.0, 0.0], [1.0, 1.0]]})).is_err());
    }

    fn ink_on(s: &Session, slide: usize) -> Vec<InkStroke> {
        let st = s.doc().unwrap();
        st.doc.slides[slide]
            .shapes
            .iter()
            .find_map(|sh| match &sh.kind {
                ShapeKind::Ink { strokes } => Some(strokes.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn tone(n: usize) -> Vec<u8> {
        let samples: Vec<f32> = (0..n).map(|i| (i as f32 * 0.2).sin() * 0.3).collect();
        deckcraft_media::wav_mono(8000, &samples).unwrap()
    }

    fn narrations(s: &Session, slide: usize) -> Vec<MediaClip> {
        s.doc().unwrap().doc.slides[slide]
            .shapes
            .iter()
            .filter(|sh| sh.name == "Narration" && sh.hidden)
            .filter_map(|sh| match &sh.kind {
                ShapeKind::Media(m) => Some(m.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn narration_embeds_replaces_and_undoes() {
        let mut s = Session::new();
        open_sample(&mut s).unwrap();
        let wav = tone(4000);
        let v = s.execute("show.narration", &json!({"index": 0, "name": "narration.wav", "data": base64_encode(&wav)})).unwrap();
        assert_eq!(v["durationMs"], 500);
        let clips = narrations(&s, 0);
        assert_eq!(clips.len(), 1);
        assert!(clips[0].autoplay && clips[0].narration && !clips[0].video);
        assert!((clips[0].volume - 1.0).abs() < 1e-9);
        let longer = tone(8000);
        s.execute("show.narration", &json!({"index": 0, "name": "narration.wav", "data": base64_encode(&longer)})).unwrap();
        let clips = narrations(&s, 0);
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].duration_ms, 1000);
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(narrations(&s, 0)[0].duration_ms, 500);
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(narrations(&s, 0).is_empty());
    }

    #[test]
    fn narration_rejects_short_junk_and_a_missing_slide() {
        let mut s = Session::new();
        open_sample(&mut s).unwrap();
        assert!(s.execute("show.narration", &json!({})).is_err());
        assert!(s.execute("show.narration", &json!({"data": base64_encode(b"hello"), "name": "x.wav"})).is_err());
        let short = tone(100);
        assert!(s.execute("show.narration", &json!({"index": 0, "name": "narration.wav", "data": base64_encode(&short)})).is_err());
        assert!(s.execute("show.narration", &json!({"index": 99, "name": "narration.wav", "data": base64_encode(&tone(4000))})).is_err());
        assert!(narrations(&s, 0).is_empty());
    }

    #[test]
    fn narration_and_laser_round_trip_through_pptx() {
        let mut s = Session::new();
        open_sample(&mut s).unwrap();
        let before = s.doc().unwrap().doc.slides[0].animations.len();
        let wav = tone(4000);
        s.execute("show.narration", &json!({"index": 0, "name": "narration.wav", "data": base64_encode(&wav)})).unwrap();
        s.execute("show.laser", &json!({"index": 0, "points": [[30.0, 40.0, 500], [10.0, 20.0, 0], [1.0e20, 0.0, 1]], "color": [0, 180, 40]}))
            .unwrap();
        let inserted = s.execute("insert.audio", &json!({"name": "tone.wav", "data": base64_encode(&wav)})).unwrap();
        let id = inserted["id"].as_u64().unwrap();
        s.execute("media.options", &json!({"autoplay": true, "ids": [id]})).unwrap();
        let bytes = deckcraft_pptx::export(&s.doc().unwrap().doc).unwrap();
        let q = deckcraft_pptx::import(&bytes).unwrap();
        assert_eq!(q.slides[0].animations.len(), before);
        let narr = q.slides[0].shapes.iter().find(|sh| sh.name == "Narration").unwrap();
        let m = match &narr.kind {
            ShapeKind::Media(m) => m,
            _ => {
                assert!(narr.name.is_empty(), "narration shape is media");
                return;
            }
        };
        assert!(m.autoplay && m.narration);
        let item = q.media(m.media).unwrap();
        assert_eq!(&item.data[..], &wav[..]);
        let laser = q.slides[0].laser.as_ref().unwrap();
        assert_eq!(laser.color, Rgba::rgb(0, 180, 40));
        assert_eq!(laser.points.len(), 2);
        assert_eq!(laser.points[0].t_ms, 0);
        assert!((laser.points[0].x - 10.0).abs() < 0.01);
        assert_eq!(laser.points[1].t_ms, 500);
        let auto = q.slides[0]
            .shapes
            .iter()
            .find_map(|sh| match &sh.kind {
                ShapeKind::Media(c) if !c.narration && c.autoplay => Some(c),
                _ => None,
            })
            .unwrap();
        assert!(!auto.narration && auto.autoplay);
    }

    #[test]
    fn laser_replaces_and_undoes() {
        let mut s = Session::new();
        open_sample(&mut s).unwrap();
        s.execute("show.laser", &json!({"index": 0, "points": [[1.0, 2.0, 10], [3.0, 4.0, 20]]})).unwrap();
        s.execute("show.laser", &json!({"index": 0, "points": [[8.0, 9.0, 30]]})).unwrap();
        let trace = s.doc().unwrap().doc.slides[0].laser.clone().unwrap();
        assert_eq!(trace.points.len(), 1);
        assert!((trace.points[0].x - 8.0).abs() < 1e-9);
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(s.doc().unwrap().doc.slides[0].laser.as_ref().unwrap().points.len(), 2);
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(s.doc().unwrap().doc.slides[0].laser.is_none());
        assert!(s.execute("show.laser", &json!({"points": []})).is_err());
        assert!(s.execute("show.laser", &json!({"index": 99, "points": [[0.0, 0.0, 0]]})).is_err());
    }
}
