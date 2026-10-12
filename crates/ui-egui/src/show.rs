//! Slide Show: full-screen playback with transitions and animations (`deckcraft-anim`),
//! keyboard/mouse navigation, pen, black/white screens, presenter view and rehearsed timings.

use deckcraft_anim::{Layer, LinkJump, ShowAction, ShowState, Source, Timeline};
use deckcraft_model::text::Hyperlink;
use deckcraft_model::{Presentation, ShapeId};
use deckcraft_render::{ParaState, RenderOpts, ShapeState};
use egui::{Align2, Color32, CornerRadius, Mesh, Pos2, Rect, Sense, Stroke, TextureHandle, Ui, pos2, vec2};
use serde_json::{Value, json};

use crate::SlideApp;
use crate::icons::Icon;
use crate::show_ink::{PALETTE, ShowInk, ShowStroke, ShowTool, smooth_stroke};
use crate::theme;

/// The select tool's active drag gesture (normalized slide coordinates). Every gesture is
/// anchored to the slide where it started: navigating away mid-drag ends/abandons it.
enum SelectDrag {
    /// Moving the selection: the slide, the last pointer position (deltas apply per frame).
    Move(usize, f32, f32),
    /// Marquee rectangle: the slide, start corner, current corner, and shift (add to
    /// selection) as sampled at press.
    Marquee(usize, (f32, f32), (f32, f32), bool),
}

pub struct Show {
    pub state: ShowState,
    pub reading: bool,
    pub presenter: bool,
    /// Preview from the Transitions/Animations tab: exits after the current slide.
    pub preview_only: bool,
    /// Play every step automatically (animation preview).
    pub autoplay: bool,
    pub rehearse: bool,
    timeline: Option<(usize, Timeline)>,
    step_start: f64,
    slide_start: f64,
    step_end: f64,
    trans: Option<Transition>,
    tex: Option<(Key, TextureHandle)>,
    next_tex: Option<(usize, TextureHandle)>,
    blank: Option<Color32>,
    ink: ShowInk,
    /// The slide an erase gesture started on (the gesture only erases there).
    erase_slide: Option<usize>,
    /// The select tool's active drag gesture (normalized slide coordinates).
    select_drag: Option<SelectDrag>,
    /// The keep-ink prompt is up (show stays alive until answered).
    ask_keep: bool,
    /// A scripted answer waiting for the prompt (`show.end {keep}`).
    pending_keep: Option<bool>,
    /// Laser pointer trail: (screen pos, timestamp), newest last.
    laser_trail: Vec<(Pos2, f64)>,
    /// The on-screen control bar's rect while visible (for click hit-testing).
    bar_rect: Option<Rect>,
    /// The bar's measured content width (re-measured every frame it is drawn).
    bar_measured: Option<f32>,
    /// The bar's measured width with ALL extras (swatches + dots) drawn; decides whether the
    /// window is wide enough for them (never fed back into itself, so it can't oscillate).
    bar_full: Option<f32>,
    /// When the keep-ink prompt opened (rehearse timing must not absorb the dwell).
    ask_since: f64,
    goto: String,
    started: bool,
    show_start: f64,
    timings: Vec<(usize, f64)>,
    last_move: f64,
    pub ended: bool,
    /// The slide whose media was last started (auto-play runs once per visit).
    media_slide: Option<usize>,
    /// The slide shown before the current one (the Last Slide Viewed action).
    last_viewed: Option<usize>,
    /// Click areas of hyperlinks and shape actions on slide `.0`, in slide points.
    links: Option<(usize, Vec<deckcraft_render::PlacedLink>)>,
}

#[derive(Clone, PartialEq)]
struct Key {
    slide: usize,
    step: usize,
    size: (u32, u32),
    doc: usize,
    /// Media hidden while not playing.
    hidden: Vec<ShapeId>,
    /// The frames animated GIFs show.
    gif: u64,
}

struct Transition {
    old: Option<TextureHandle>,
    /// The slide left behind (Morph matches its shapes) and the Morph frame texture.
    from: Option<usize>,
    morph: Option<TextureHandle>,
    start: f64,
    kind: String,
    option: String,
    dur: f64,
}

impl Show {
    pub fn new(doc: &Presentation, from: usize, reading: bool, presenter: bool) -> Self {
        Show {
            state: ShowState::start_at(doc, from),
            reading,
            presenter,
            preview_only: false,
            autoplay: false,
            rehearse: false,
            timeline: None,
            step_start: 0.0,
            slide_start: 0.0,
            step_end: 0.0,
            trans: None,
            tex: None,
            next_tex: None,
            blank: None,
            ink: ShowInk::new(doc.show.pen_color),
            erase_slide: None,
            select_drag: None,
            ask_keep: false,
            pending_keep: None,
            laser_trail: vec![],
            bar_rect: None,
            bar_measured: None,
            bar_full: None,
            ask_since: 0.0,
            goto: String::new(),
            started: false,
            show_start: 0.0,
            timings: vec![],
            last_move: 0.0,
            ended: false,
            media_slide: None,
            last_viewed: None,
            links: None,
        }
    }

    fn timeline(&mut self, doc: &Presentation) -> &Timeline {
        let i = self.state.slide;
        if self.timeline.as_ref().is_none_or(|(k, _)| *k != i) {
            self.timeline = Some((i, ShowState::timeline(doc, i)));
        }
        match &self.timeline {
            Some((_, t)) => t,
            None => {
                static EMPTY: std::sync::OnceLock<Timeline> = std::sync::OnceLock::new();
                EMPTY.get_or_init(Timeline::default)
            }
        }
    }

    fn apply(&mut self, a: ShowAction, doc: &Presentation, now: f64) -> bool {
        match a {
            ShowAction::PlayStep(_) => {
                self.step_start = now;
            }
            ShowAction::FinishStep(_) | ShowAction::StepBack(_) => {
                self.step_end = now;
            }
            ShowAction::Slide(i) => {
                self.ended = false;
                if self.rehearse {
                    self.timings.push((self.timeline.as_ref().map(|t| t.0).unwrap_or(i), now - self.slide_start));
                }
                let (kind, option, dur) = doc
                    .slides
                    .get(i)
                    .and_then(|s| s.transition.as_ref())
                    .map(|t| (t.kind.clone(), t.option.clone(), t.duration_ms as f64 / 1000.0))
                    .unwrap_or(("none".into(), String::new(), 0.0));
                // Take the old slide's texture rather than cloning its handle: the new slide is
                // otherwise rendered into the same texture, and the transition blends the new
                // slide with itself (#33).
                let (from, old) = self.tex.take().map(|(k, t)| (k.slide, t)).unzip();
                if from != Some(i) {
                    self.last_viewed = from;
                }
                self.trans = if kind != "none" && dur > 0.0 && !doc.show.without_animation {
                    Some(Transition { old, from, morph: None, start: now, kind, option, dur })
                } else {
                    None
                };
                self.slide_start = now;
                self.step_start = now;
                self.step_end = now;
                // A stroke in progress ends at the slide boundary (like PowerPoint).
                self.ink.end();
                self.ink.end_erase();
                self.erase_slide = None;
                // The selection belongs to the slide that was visible when it was made;
                // syncing here (the one navigation choke point) means no caller can act on a
                // stale selection — not even a bar click followed by a key next frame.
                self.ink.sync_slide(self.state.slide);
                if self.preview_only {
                    return false;
                }
                self.after_enter(doc, now);
            }
            ShowAction::End => {
                self.ended = true;
                self.ink.end();
                self.ink.end_erase();
                self.erase_slide = None;
                self.ink.sync_slide(self.state.slide);
                if self.preview_only {
                    return false;
                }
            }
            ShowAction::Exit => return false,
            ShowAction::None => {}
        }
        true
    }

    /// On entering a slide: auto-play its first step when it begins With/After Previous.
    fn after_enter(&mut self, doc: &Presentation, now: f64) {
        let auto = self.timeline(doc).step_is_auto(0);
        if auto || self.autoplay {
            let a = self.state.next(doc);
            if matches!(a, ShowAction::PlayStep(_)) {
                self.step_start = now;
            }
        }
    }
}

fn state_for(tl: &Timeline, id: ShapeId, step: usize, t: f64) -> ShapeState {
    let a = tl.state(id, None, step, t);
    ShapeState {
        visible: a.visible,
        opacity: a.opacity,
        offset: deckcraft_geom::Vec2::new(a.offset_x, a.offset_y),
        scale: (a.scale_x, a.scale_y),
        rotate: a.rotate,
        clip: a.clip,
        tint: a.tint,
        paras: tl
            .paragraph_targets(id)
            .into_iter()
            .map(|p| {
                let a = tl.state(id, Some(p), step, t);
                (p, ParaState { visible: a.visible, opacity: a.opacity, offset: deckcraft_geom::Vec2::new(a.offset_x, a.offset_y) })
            })
            .collect(),
    }
}

fn fit(avail: Rect, doc: &Presentation) -> Rect {
    let (sw, sh) = (doc.slide_size.width as f32, doc.slide_size.height as f32);
    let s = (avail.width() / sw).min(avail.height() / sh);
    Rect::from_center_size(avail.center(), vec2(sw * s, sh * s))
}

fn quad_mesh(tex: Option<egui::TextureId>, rect: Rect, l: &Layer, color: Color32) -> Mesh {
    let mut m = match tex {
        Some(id) => Mesh::with_texture(id),
        None => Mesh::default(),
    };
    let a = (l.alpha.clamp(0.0, 1.0) * 255.0) as u8;
    let col = if tex.is_some() { Color32::from_white_alpha(a) } else { color.gamma_multiply(l.alpha.clamp(0.0, 1.0) as f32) };
    for k in 0..4 {
        let q = l.quad.get(k).copied().unwrap_or([0.0, 0.0]);
        let uv = l.uv.get(k).copied().unwrap_or([0.0, 0.0]);
        let p = pos2(rect.min.x + q[0] as f32 * rect.width(), rect.min.y + q[1] as f32 * rect.height());
        m.vertices.push(egui::epaint::Vertex { pos: p, uv: pos2(uv[0] as f32, uv[1] as f32), color: col });
    }
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    m
}

/// Width of the bar's extras section (separator + 8 swatches + 4 width dots + spacing) for
/// the compact-mode estimate of the full bar width.
const EXTRAS_W: f32 = 300.0;

pub fn ui(app: &mut SlideApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let Some(doc) = app.session.active().map(|d| d.doc.clone()) else {
        app.show = None;
        return;
    };
    let now = ctx.input(|i| i.time);
    let Some(mut show) = app.show.take() else { return };
    if !show.started {
        show.started = true;
        show.slide_start = now;
        show.step_start = now;
        show.show_start = now;
        show.last_move = now;
        if !show.reading {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        }
        show.after_enter(&doc, now);
    }
    let mut keep = true;
    // Input.
    let (keys, clicked, secondary, moved) = ctx.input(|i| {
        let keys: Vec<(egui::Key, egui::Modifiers)> = i
            .events
            .iter()
            .filter_map(|e| if let egui::Event::Key { key, pressed: true, modifiers, .. } = e { Some((*key, *modifiers)) } else { None })
            .collect();
        (keys, i.pointer.primary_clicked(), i.pointer.secondary_clicked(), i.pointer.delta().length() > 0.0)
    });
    if moved {
        show.last_move = now;
    }
    let mut keep_answer: Option<bool> = None;
    // While the keep-ink prompt is up, clicks and keys go only to the dialog.
    let (clicked, secondary) = if show.ask_keep { (false, false) } else { (clicked, secondary) };
    let mut actions: Vec<ShowAction> = vec![];
    for (k, m) in keys {
        use egui::Key::*;
        if show.ask_keep {
            match k {
                Enter | Y => keep_answer = Some(true),
                Escape | N => keep_answer = Some(false),
                _ => {}
            }
            continue;
        }
        // Navigation keys mutate state.slide at once (their action is applied below); keep the
        // selection on the visible slide even when a later key in the same frame reads it.
        show.ink.sync_slide(show.state.slide);
        match k {
            Escape | Minus => keep = false,
            ArrowRight | ArrowDown | Space | PageDown | N => {
                if show.ended {
                    keep = false;
                } else {
                    actions.push(show.state.next(&doc));
                }
            }
            Enter => {
                if let Ok(n) = show.goto.parse::<usize>() {
                    actions.push(show.state.goto(&doc, n.saturating_sub(1)));
                    show.goto.clear();
                } else if show.ended {
                    keep = false;
                } else {
                    actions.push(show.state.next(&doc));
                }
            }
            ArrowLeft | ArrowUp | Backspace | PageUp | P if !(k == P && m.command) => {
                actions.push(show.state.prev(&doc));
            }
            P if m.command => show.ink.toggle(ShowTool::Pen),
            I if m.command => show.ink.toggle(ShowTool::Highlighter),
            E if m.command => show.ink.toggle(ShowTool::Eraser),
            L if m.command => show.ink.toggle(ShowTool::Laser),
            A if m.command => show.ink.set_tool(ShowTool::Pointer),
            V if m.is_none() => show.ink.set_tool(ShowTool::Select),
            Z if m.command && !m.shift => {
                show.ink.undo();
            }
            // Selection editing (select tool only, never on the end screen or mid-drag;
            // Backspace stays "previous slide").
            Delete if show.ink.tool() == ShowTool::Select && !show.ended && show.select_drag.is_none() => {
                show.ink.delete_selection();
            }
            D if m.command && show.ink.tool() == ShowTool::Select && !show.ended && show.select_drag.is_none() => {
                show.ink.duplicate_selection();
            }
            C if m.command && show.ink.tool() == ShowTool::Select && !show.ended => {
                show.ink.copy_selection();
            }
            V if m.command && show.ink.tool() == ShowTool::Select && !show.ended && show.select_drag.is_none() => {
                show.ink.paste(show.state.slide);
            }
            Home => actions.push(show.state.goto(&doc, 0)),
            End => actions.push(show.state.goto(&doc, doc.slides.len().saturating_sub(1))),
            B | Period => show.blank = if show.blank == Some(Color32::BLACK) { None } else { Some(Color32::BLACK) },
            W | Comma => show.blank = if show.blank == Some(Color32::WHITE) { None } else { Some(Color32::WHITE) },
            E if !m.command => show.ink.clear_slide(show.state.slide),
            C if m.is_none() => show.ink.cycle_color(),
            OpenBracket if m.is_none() => show.ink.step_width(false),
            CloseBracket if m.is_none() => show.ink.step_width(true),
            Num0 | Num1 | Num2 | Num3 | Num4 | Num5 | Num6 | Num7 | Num8 | Num9 => {
                show.goto.push_str(k.name());
            }
            _ => {}
        }
    }
    // The control bar's rect for this frame, computed before input so hit-testing below never
    // works from a stale frame. The bar is always visible during the show (all annotation
    // tools at hand), except on the end-of-show screen and under the keep prompt.
    let full = ui.max_rect();
    show.bar_rect = if !show.reading && !show.ask_keep && !show.ended {
        Some(Rect::from_min_size(pos2(full.min.x + 20.0, full.max.y - 50.0), vec2(show.bar_width(full.width()), 34.0)))
    } else {
        None
    };
    // Laser clicks still advance the show (the laser is a pointer, not an ink tool).
    let pointer = matches!(show.ink.tool(), ShowTool::Pointer | ShowTool::Laser);
    let bar_hit = show.reading_bar_hit(&ctx);
    let on_slide = pointer && !show.ended && show.blank.is_none() && !bar_hit;
    let gif_click = if clicked && on_slide { gif_hit(&ctx, ui, &show, &doc, app) } else { None };
    let media_click = if clicked && on_slide && gif_click.is_none() { media_hit(&ctx, ui, &show, &doc) } else { None };
    let link = if on_slide && gif_click.is_none() && media_click.is_none() { link_hit(&ctx, ui, &mut show, &doc) } else { None };
    if let Some(id) = gif_click {
        let _ = app.run("media.gifPlay", json!({"id": id}));
    } else if let Some((id, clip)) = media_click {
        app.media.toggle(&doc, id, &clip, crate::media::Owner::Show);
    } else if clicked && let Some(l) = &link {
        let last = show.last_viewed;
        match show.state.follow(&doc, &l.action, last) {
            LinkJump::Show(a) => actions.push(a),
            LinkJump::Open(url) => ctx.open_url(egui::OpenUrl::new_tab(url)),
            LinkJump::Exit => keep = false,
            LinkJump::None => {}
        }
    } else if clicked && !bar_hit && (pointer || show.ended) {
        // The end screen exits on any click, whatever tool is active.
        if show.ended {
            keep = false;
        } else if show.blank.is_some() {
            show.blank = None;
        } else {
            actions.push(show.state.next(&doc));
        }
    }
    if secondary && !show.ended && !bar_hit {
        actions.push(show.state.prev(&doc));
    }
    // Apply input first: a step started by this click must restart the clock before it is read.
    for a in actions.drain(..) {
        if !show.apply(a, &doc, now) {
            keep = false;
        }
    }
    // Step clock (frozen while the keep-ink prompt is up).
    let (step, playing) = (show.state.step, show.state.playing);
    let t_step = if playing { now - show.step_start } else { 0.0 };
    if show.ask_keep {
        // Modal: no auto-advance, no media churn behind the dialog.
    } else if playing {
        let dur = show.timeline(&doc).step_duration(step);
        if t_step >= dur {
            actions.push(show.state.step_done(&doc));
            show.step_end = now;
        }
    } else if show.trans.is_none() {
        // Auto-advance (After: timings) or autoplay preview.
        let idle = now - show.step_end.max(show.slide_start);
        let cur_step = show.state.step;
        let next_auto = show.timeline(&doc).step_is_auto(cur_step);
        let n_steps = show.timeline(&doc).steps();
        let step_due = cur_step < n_steps && ((show.autoplay && idle > 0.3) || next_auto);
        if step_due || (!show.rehearse && show.state.auto_advance_due(&doc, idle) && !show.ended) {
            actions.push(show.state.next(&doc));
        } else if show.preview_only && cur_step >= n_steps && idle > 0.8 {
            keep = false;
        }
    }
    for a in actions {
        if !show.apply(a, &doc, now) {
            keep = false;
        }
    }
    if !show.ask_keep {
        show_media(app, &mut show, &doc);
    }
    // Draw (`full` and `bar_rect` were computed before input handling).
    let painter = ui.painter_at(full);
    painter.rect_filled(full, CornerRadius::ZERO, Color32::BLACK);
    let avail = if show.reading { Rect::from_min_max(full.min, pos2(full.max.x, full.max.y - 34.0)) } else { full };
    let srect = fit(avail, &doc);
    let ppp = ctx.pixels_per_point();
    let size = ((srect.width() * ppp).round().max(1.0) as u32, (srect.height() * ppp).round().max(1.0) as u32);
    if show.ended {
        painter.text(
            pos2(full.center().x, full.min.y + 40.0),
            Align2::CENTER_CENTER,
            "End of slide show, click to exit.",
            theme::font(18.0),
            Color32::from_gray(200),
        );
    } else {
        let idx = show.state.slide;
        let step = show.state.step;
        let animating = show.state.playing;
        let t = if animating { now - show.step_start } else { 0.0 };
        let media = crate::media::slide_media(&doc, idx);
        let hidden: Vec<ShapeId> = media
            .iter()
            .filter(|(id, _, clip)| {
                clip.hide_while_not_playing && !app.media.status(*id).is_some_and(|s| s.state != deckcraft_media::PlayState::Ended)
            })
            .map(|m| m.0)
            .collect();
        let gifs = app.media.gifs(&doc, idx, now, &app.session.gif_paused);
        let times: Vec<_> = gifs.iter().map(|g| (g.id, g.t)).collect();
        crate::media::gif_repaint(&ctx, &gifs);
        let gif = crate::media::gif_key(&gifs);
        let key = Key { slide: idx, step, size, doc: std::sync::Arc::as_ptr(&doc) as usize, hidden: hidden.clone(), gif };
        let tex = if !animating && show.tex.as_ref().is_some_and(|(k, _)| *k == key) {
            show.tex.as_ref().map(|(_, t)| t.clone())
        } else {
            let tl = show.timeline(&doc).clone();
            let hide = hidden.clone();
            let f = move |id: ShapeId| {
                let mut st = state_for(&tl, id, step, t);
                if hide.contains(&id) {
                    st.visible = false;
                }
                st
            };
            let threads =
                if cfg!(target_arch = "wasm32") { 0 } else { std::thread::available_parallelism().map(|n| n.get().min(8) as u16).unwrap_or(0) };
            let img = deckcraft_render::render_slide(
                &doc,
                idx,
                &RenderOpts {
                    scale: size.0 as f64 / doc.slide_size.width.max(1.0),
                    size: Some(size),
                    state: Some(&f),
                    gif_times: &times,
                    threads,
                    ..Default::default()
                },
            );
            let ci = crate::textures::to_color_image(&img, false);
            let tex = match show.tex.take() {
                Some((_, mut h)) => {
                    h.set(ci, egui::TextureOptions::LINEAR);
                    h
                }
                None => ctx.load_texture("show", ci, egui::TextureOptions::LINEAR),
            };
            show.tex = Some((key, tex.clone()));
            Some(tex)
        };
        let mut drew = false;
        let morphing = show.trans.as_ref().is_some_and(|tr| tr.kind == "morph" && tr.from.is_some() && now - tr.start < tr.dur);
        let tl = if morphing { Some(show.timeline(&doc).clone()) } else { None };
        let morph = match (&show.trans, tl) {
            (Some(tr), Some(tl)) => {
                let p = ((now - tr.start) / tr.dur.max(0.01)).clamp(0.0, 1.0);
                let hidden_b = |id: ShapeId| {
                    let st = state_for(&tl, id, 0, 0.0);
                    !st.visible || st.opacity <= 0.0 || hidden.contains(&id)
                };
                tr.from
                    .and_then(|i| doc.slides.get(i).map(|a| (i, a)))
                    .zip(doc.slides.get(idx))
                    .map(|((i, a), b)| (i, deckcraft_anim::morph_frame(&doc, a, b, p, &hidden_b)))
            }
            _ => None,
        };
        if let (Some((from, m)), Some(tr)) = (morph, show.trans.as_mut()) {
            let f = |id: ShapeId| ShapeState { opacity: m.opacity_of(id), ..Default::default() };
            let threads =
                if cfg!(target_arch = "wasm32") { 0 } else { std::thread::available_parallelism().map(|n| n.get().min(8) as u16).unwrap_or(0) };
            let opts =
                RenderOpts { scale: size.0 as f64 / doc.slide_size.width.max(1.0), size: Some(size), state: Some(&f), threads, ..Default::default() };
            let text: Vec<deckcraft_render::TextMorph> = m
                .text
                .iter()
                .map(|x| deckcraft_render::TextMorph { old: x.old, new: x.new, from: x.from, to: x.to, chars: x.chars, t: m.mix })
                .collect();
            let paths: Vec<deckcraft_render::PathMorph> = m
                .paths
                .iter()
                .map(|x| deckcraft_render::PathMorph { id: x.id, from: x.from.clone(), from_box: x.from_box, to_box: x.to_box, t: m.mix })
                .collect();
            let img = deckcraft_render::render_blend(&doc, from, idx, m.mix, &m.shapes, &text, &paths, &opts);
            let ci = crate::textures::to_color_image(&img, false);
            let frame = match tr.morph.take() {
                Some(mut h) => {
                    h.set(ci, egui::TextureOptions::LINEAR);
                    h
                }
                None => ctx.load_texture("morph", ci, egui::TextureOptions::LINEAR),
            };
            painter.image(frame.id(), srect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            tr.morph = Some(frame);
            drew = true;
            ctx.request_repaint();
        } else if let (Some(tr), Some(new)) = (&show.trans, &tex) {
            let p = ((now - tr.start) / tr.dur.max(0.01)).clamp(0.0, 1.0);
            if p < 1.0 {
                for l in deckcraft_anim::transition_layers(&tr.kind, &tr.option, p) {
                    let mesh = match l.source {
                        Source::Old => match &tr.old {
                            Some(o) => quad_mesh(Some(o.id()), srect, &l, Color32::WHITE),
                            None => quad_mesh(None, srect, &l, Color32::BLACK),
                        },
                        Source::New => quad_mesh(Some(new.id()), srect, &l, Color32::WHITE),
                        Source::Black => quad_mesh(None, srect, &l, Color32::BLACK),
                        Source::White => quad_mesh(None, srect, &l, Color32::WHITE),
                    };
                    painter.add(egui::Shape::mesh(mesh));
                }
                drew = true;
                ctx.request_repaint();
            }
        }
        if !drew {
            show.trans = None;
            if let Some(tex) = &tex {
                painter.image(tex.id(), srect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
            // Playing video: the current frame in its box, or full screen.
            let k = srect.width() / doc.slide_size.width.max(1.0) as f32;
            for (id, x, clip) in &media {
                if !clip.video || hidden.contains(id) {
                    continue;
                }
                let r = if clip.full_screen && app.media.is_playing(*id) {
                    full
                } else {
                    Rect::from_min_size(pos2(srect.min.x + x.x as f32 * k, srect.min.y + x.y as f32 * k), vec2(x.w as f32 * k, x.h as f32 * k))
                };
                crate::media::paint_frame(&painter, &app.media, *id, r);
            }
            // Play/pause on the animated GIF under the pointer.
            if let Some(p) = ctx.input(|i| i.pointer.hover_pos()).filter(|_| pointer && now - show.last_move < 2.5) {
                for g in &gifs {
                    let r = on_screen(srect, k, &g.rect);
                    if r.contains(p) {
                        let b = crate::media::gif_button_rect(r);
                        crate::media::paint_gif_button(&painter, b, g.paused, b.contains(p));
                    }
                }
            }
        }
        if animating {
            ctx.request_repaint();
        }
    }
    if let Some(c) = show.blank {
        painter.rect_filled(full, CornerRadius::ZERO, c);
    }
    // Pen / highlighter / eraser / laser / select. No ink input while a transition is
    // running: the paint gate hides ink then, so drawing would be invisible.
    let drawing = matches!(show.ink.tool(), ShowTool::Pen | ShowTool::Highlighter) && !show.ended && show.trans.is_none() && !show.ask_keep;
    let erasing = show.ink.tool() == ShowTool::Eraser && !show.ended && show.trans.is_none() && !show.ask_keep;
    let lasering = show.ink.tool() == ShowTool::Laser && !show.ended && !show.ask_keep;
    let selecting = show.ink.tool() == ShowTool::Select && !show.ended && show.trans.is_none() && !show.ask_keep;
    if !selecting && show.select_drag.is_some() {
        // The tool changed mid-gesture: commit the move, drop the marquee.
        show.select_drag = None;
        show.ink.end_move();
    }
    let mut eraser_pos: Option<Pos2> = None;
    let resp = ui.interact(full, ui.id().with("show_pen"), if drawing || erasing || selecting { Sense::click_and_drag() } else { Sense::hover() });
    let norm = |p: Pos2| ((p.x - srect.min.x) / srect.width().max(1.0), (p.y - srect.min.y) / srect.height().max(1.0));
    if drawing {
        let pressed = ctx.input(|i| i.pointer.primary_pressed());
        if resp.is_pointer_button_down_on()
            && let Some(p) = resp.interact_pointer_pos()
        {
            let (x, y) = norm(p);
            if show.ink.open() {
                show.ink.push(show.state.slide, x, y);
            } else if pressed && !bar_hit {
                // Begin on the press itself so a stationary click still draws a dot — but
                // never from a press on the control bar (egui reports both hits there).
                show.ink.begin(show.state.slide, x, y);
            }
        }
        if !resp.is_pointer_button_down_on() {
            show.ink.end();
        }
        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
    } else if erasing {
        let pressed = ctx.input(|i| i.pointer.primary_pressed());
        if resp.is_pointer_button_down_on() {
            // The erase gesture stays on the slide where it started: a slide change mid-drag
            // must not erase ink on the slide just landed on. Presses on the control bar
            // start no gesture.
            if show.erase_slide.is_none() && pressed && !bar_hit {
                show.erase_slide = Some(show.state.slide);
                show.ink.start_erase();
            }
            if let (Some(s), Some(p)) = (show.erase_slide, resp.interact_pointer_pos()) {
                let (x, y) = norm(p);
                let ink_k = srect.width() / doc.slide_size.width.max(1.0) as f32;
                // 10px tolerance in slide units (points).
                show.ink.erase_at(s, x, y, 10.0 / ink_k.max(0.01), doc.slide_size.width, doc.slide_size.height);
            }
        } else {
            show.erase_slide = None;
            show.ink.end_erase();
        }
        eraser_pos = resp.interact_pointer_pos().or_else(|| ctx.input(|i| i.pointer.hover_pos()));
        ctx.set_cursor_icon(egui::CursorIcon::None);
    } else if selecting {
        let pressed = ctx.input(|i| i.pointer.primary_pressed());
        if resp.is_pointer_button_down_on()
            && let Some(p) = resp.interact_pointer_pos()
        {
            let (x, y) = norm(p);
            let slide = show.state.slide;
            let ink_k = srect.width() / doc.slide_size.width.max(1.0) as f32;
            // A gesture from a slide we just left is over: sync_slide already journaled the
            // partial move; the marquee is simply abandoned.
            let stale = matches!(show.select_drag.as_ref(), Some(SelectDrag::Move(s, ..) | SelectDrag::Marquee(s, ..)) if *s != slide);
            if stale {
                show.select_drag = None;
                show.ink.end_move();
            }
            if pressed && !bar_hit && show.select_drag.is_none() {
                // Press: on a stroke → (maybe re-select and) start a move; on empty space →
                // marquee. Shift toggles/adds instead of replacing.
                let shift = ctx.input(|i| i.modifiers.shift);
                match show.ink.hit_test(slide, x, y, 10.0 / ink_k.max(0.01), doc.slide_size.width, doc.slide_size.height) {
                    Some(id) => {
                        if shift {
                            show.ink.toggle_selected(slide, id);
                        } else {
                            if !show.ink.is_selected(id) {
                                show.ink.select_only(slide, id);
                            }
                            show.ink.begin_move();
                            show.select_drag = Some(SelectDrag::Move(slide, x, y));
                        }
                    }
                    None => {
                        if !shift {
                            show.ink.clear_selection();
                        }
                        show.select_drag = Some(SelectDrag::Marquee(slide, (x, y), (x, y), shift));
                    }
                }
            } else if let Some(drag) = show.select_drag.as_mut() {
                match drag {
                    SelectDrag::Move(_, lx, ly) => {
                        show.ink.move_selection_by(x - *lx, y - *ly);
                        *lx = x;
                        *ly = y;
                    }
                    SelectDrag::Marquee(_, _, cur, _) => *cur = (x, y),
                }
            }
        }
        if !resp.is_pointer_button_down_on()
            && let Some(drag) = show.select_drag.take()
        {
            match drag {
                SelectDrag::Move(..) => {
                    show.ink.end_move();
                }
                SelectDrag::Marquee(s, a, b, add) => {
                    // A (near-)zero drag is a click, not a marquee — the press already handled
                    // the selection. And if the show navigated mid-drag, the marquee dies here.
                    if s == show.state.slide && ((b.0 - a.0).abs() > 0.002 || (b.1 - a.1).abs() > 0.002) {
                        show.ink.select_in_rect(s, a, b, add);
                    }
                }
            }
        }
        ctx.set_cursor_icon(egui::CursorIcon::Default);
    } else if link.is_some() {
        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    } else if lasering {
        // The laser dot replaces the cursor. The fading trail needs frames only while the
        // pointer moves or the trail is still fading.
        ctx.set_cursor_icon(egui::CursorIcon::None);
        if moved && let Some(p) = ctx.input(|i| i.pointer.hover_pos()) {
            show.laser_trail.push((p, now));
            if show.laser_trail.len() > 240 {
                show.laser_trail.drain(..show.laser_trail.len() - 240);
            }
        }
        if !show.laser_trail.is_empty() {
            ctx.request_repaint();
        }
    } else if now - show.last_move > 2.5 && !show.reading {
        // Idle hides the cursor — but never while hovering the (always visible) bar.
        let over_bar = show.bar_rect.is_some_and(|b| ctx.input(|i| i.pointer.hover_pos()).is_some_and(|p| b.contains(p)));
        if !over_bar {
            ctx.set_cursor_icon(egui::CursorIcon::None);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }
    if !lasering && !show.laser_trail.is_empty() {
        show.laser_trail.clear();
    }
    // Ink of the current slide (hidden during transitions and on the end-of-show screen —
    // except under the keep prompt, so the user sees what they are keeping).
    if show.trans.is_none() && (!show.ended || show.ask_keep) {
        paint_ink(&painter, srect, doc.slide_size.width, show.ink.strokes(show.state.slide));
    }
    // Select tool overlay: an accent box with handles around the selection, and the live
    // marquee rectangle while dragging on empty space.
    if selecting {
        let accent = Color32::from_rgb(0x1E, 0x88, 0xE5);
        let to_screen = |(x, y): (f32, f32)| pos2(srect.min.x + x * srect.width(), srect.min.y + y * srect.height());
        if let Some((lo, hi)) = show.ink.selected_bounds(show.state.slide) {
            let r = Rect::from_two_pos(to_screen(lo), to_screen(hi)).expand(4.0);
            painter.rect_stroke(r, CornerRadius::same(2), Stroke::new(1.5, accent), egui::StrokeKind::Outside);
            for cx in [r.left(), r.right()] {
                for cy in [r.top(), r.bottom()] {
                    painter.rect_filled(Rect::from_center_size(pos2(cx, cy), vec2(7.0, 7.0)), CornerRadius::same(2), accent);
                }
            }
        }
        if let Some(SelectDrag::Marquee(_, a, b, _)) = &show.select_drag {
            let r = Rect::from_two_pos(to_screen(*a), to_screen(*b));
            painter.rect_filled(r, CornerRadius::ZERO, accent.gamma_multiply(0.12));
            painter.rect_stroke(r, CornerRadius::ZERO, Stroke::new(1.0, accent), egui::StrokeKind::Inside);
        }
    }
    // Reading view bar / on-screen controls.
    if show.reading {
        let bar = Rect::from_min_max(pos2(full.min.x, full.max.y - 34.0), full.max);
        painter.rect_filled(bar, CornerRadius::ZERO, Color32::from_gray(32));
        painter.text(
            pos2(bar.min.x + 14.0, bar.center().y),
            Align2::LEFT_CENTER,
            format!("Slide {} of {}", show.state.slide + 1, doc.slides.len()),
            theme::font(12.0),
            Color32::from_gray(200),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(pos2(bar.max.x - 260.0, bar.min.y), bar.max))
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        // The keep-ink prompt is modal: the reading bar is inert while it is up.
        if !show.ask_keep && bar_icon_button(&mut child, Icon::Close, "Close", false, true, "Close reading view (Esc)").clicked() {
            keep = false;
        }
        if !show.ask_keep && bar_icon_button(&mut child, Icon::ChevronRight, "Next slide", false, true, "Next slide (→)").clicked() {
            let a = show.state.next(&doc);
            if !show.apply(a, &doc, now) {
                keep = false;
            }
        }
        if !show.ask_keep && bar_icon_button(&mut child, Icon::ChevronLeft, "Previous slide", false, true, "Previous slide (←)").clicked() {
            let a = show.state.prev(&doc);
            show.apply(a, &doc, now);
        }
    } else if let Some(bar) = show.bar_rect {
        painter.rect_filled(bar, CornerRadius::same(17), Color32::from_black_alpha(140));
        let mut child =
            ui.new_child(egui::UiBuilder::new().max_rect(bar.shrink2(vec2(10.0, 4.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
        child.spacing_mut().item_spacing.x = 2.0;
        if bar_icon_button(&mut child, Icon::ChevronLeft, "Previous slide", false, true, "Previous slide (←)").clicked() {
            let a = show.state.prev(&doc);
            show.apply(a, &doc, now);
        }
        if bar_icon_button(&mut child, Icon::ChevronRight, "Next slide", false, true, "Next slide (→)").clicked() {
            let a = show.state.next(&doc);
            if !show.apply(a, &doc, now) {
                keep = false;
            }
        }
        bar_sep(&mut child);
        if bar_icon_button(
            &mut child,
            Icon::LassoSelect,
            "Select",
            show.ink.tool() == ShowTool::Select,
            true,
            "Select ink (V) — drag to move, ⌘D duplicate, ⌦ delete",
        )
        .clicked()
        {
            show.ink.set_tool(ShowTool::Select);
        }
        if bar_icon_button(
            &mut child,
            Icon::Pen,
            "Pen",
            show.ink.tool() == ShowTool::Pen,
            true,
            format!("Pen ({})", crate::ribbon::pretty_shortcut("Cmd+P")),
        )
        .clicked()
        {
            show.ink.toggle(ShowTool::Pen);
        }
        if bar_icon_button(
            &mut child,
            Icon::HighlighterPen,
            "Highlighter",
            show.ink.tool() == ShowTool::Highlighter,
            true,
            format!("Highlighter ({})", crate::ribbon::pretty_shortcut("Cmd+I")),
        )
        .clicked()
        {
            show.ink.toggle(ShowTool::Highlighter);
        }
        if bar_icon_button(
            &mut child,
            Icon::Eraser,
            "Eraser",
            show.ink.tool() == ShowTool::Eraser,
            true,
            format!("Eraser ({})", crate::ribbon::pretty_shortcut("Cmd+E")),
        )
        .clicked()
        {
            show.ink.toggle(ShowTool::Eraser);
        }
        if bar_icon_button(
            &mut child,
            Icon::LaserPointer,
            "Laser pointer",
            show.ink.tool() == ShowTool::Laser,
            true,
            format!("Laser pointer ({})", crate::ribbon::pretty_shortcut("Cmd+L")),
        )
        .clicked()
        {
            show.ink.toggle(ShowTool::Laser);
        }
        // Color swatches and width dots, whenever the full bar fits the window (narrow
        // windows get the compact bar without them).
        let roomy = show.bar_full.is_none_or(|fw| fw + 40.0 <= full.width());
        if roomy {
            bar_sep(&mut child);
            let cur = show.ink.tool_color();
            for &(name, c) in PALETTE.iter() {
                let resp = child.add(egui::Button::new("").frame(false).min_size(vec2(22.0, 24.0)));
                let r = Rect::from_center_size(resp.rect.center(), vec2(15.0, 15.0));
                child.painter().rect_filled(r, CornerRadius::same(4), theme::to_color32(c));
                child.painter().rect_stroke(r, CornerRadius::same(4), Stroke::new(1.0, Color32::from_black_alpha(140)), egui::StrokeKind::Inside);
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
                let resp = resp.on_hover_text(name);
                if cur == c {
                    // Ring outside the fill so it also shows on the white swatch.
                    let ring = if c.luminance() > 0.6 { Color32::from_gray(20) } else { Color32::WHITE };
                    child.painter().rect_stroke(r.expand(2.0), CornerRadius::same(6), Stroke::new(1.5, ring), egui::StrokeKind::Outside);
                } else if resp.hovered() {
                    child.painter().rect_stroke(
                        r.expand(2.0),
                        CornerRadius::same(6),
                        Stroke::new(1.0, Color32::from_white_alpha(90)),
                        egui::StrokeKind::Outside,
                    );
                }
                if resp.clicked() {
                    if !matches!(show.ink.tool(), ShowTool::Pen | ShowTool::Highlighter) {
                        show.ink.set_tool(ShowTool::Pen);
                    }
                    show.ink.set_color(c);
                }
            }
            // Width quick-picks drawn as dots of their size (`[` / `]` from the keyboard).
            let cur_w = show.ink.tool_width();
            for &w in show.ink.width_dots().iter() {
                let resp = child.add(egui::Button::new("").frame(false).min_size(vec2(22.0, 24.0)));
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Width {w} pt")));
                let resp = resp.on_hover_text(format!("{w} pt"));
                let on = (cur_w - w).abs() < 0.001;
                let dot = if on {
                    Color32::WHITE
                } else if resp.hovered() {
                    Color32::from_gray(220)
                } else {
                    Color32::from_gray(150)
                };
                child.painter().circle_filled(resp.rect.center(), (w as f32 * 0.9).clamp(1.5, 8.0), dot);
                if resp.clicked() {
                    if !matches!(show.ink.tool(), ShowTool::Pen | ShowTool::Highlighter) {
                        show.ink.set_tool(ShowTool::Pen);
                    }
                    show.ink.set_width(w);
                }
            }
        }
        bar_sep(&mut child);
        if bar_icon_button(
            &mut child,
            Icon::Undo,
            "Undo",
            false,
            show.ink.can_undo(),
            format!("Undo ink ({})", crate::ribbon::pretty_shortcut("Cmd+Z")),
        )
        .clicked()
        {
            show.ink.undo();
        }
        let slide_has_ink = !show.ink.strokes(show.state.slide).is_empty();
        if bar_icon_button(&mut child, Icon::Trash, "Clear", false, slide_has_ink, "Clear this slide's ink (E)").clicked() {
            show.ink.clear_slide(show.state.slide);
        }
        bar_sep(&mut child);
        if bar_icon_button(&mut child, Icon::BlankScreen, "Blank screen", show.blank.is_some(), true, "Blank screen (B/W)").clicked() {
            show.blank = if show.blank.is_some() { None } else { Some(Color32::BLACK) };
        }
        if bar_icon_button(&mut child, Icon::ExitShow, "End", false, true, "End slide show (Esc)").clicked() {
            keep = false;
        }
        // Re-measure the content so the pill and its click-guard match reality. Compact
        // frames still estimate the full width (extras are fixed-size), so widening the
        // window restores the swatches and dots immediately.
        let measured = (child.min_rect().width() + 20.0).ceil();
        show.bar_measured = Some(measured);
        show.bar_full = Some(if roomy { measured } else { measured + EXTRAS_W });
        // Keep frames coming so the idle-cursor hide can trigger.
        ctx.request_repaint_after(std::time::Duration::from_millis(600));
    }
    // Eraser cursor ring, on top of everything (two-tone so it reads on white and black).
    if let Some(p) = eraser_pos {
        painter.circle_stroke(p, 10.0, Stroke::new(3.0, Color32::from_black_alpha(200)));
        painter.circle_stroke(p, 10.0, Stroke::new(1.0, Color32::WHITE));
    }
    // Laser pointer: a glowing dot with a smooth, tapered, fading tail.
    if lasering {
        let col = theme::to_color32(doc.show.laser_color);
        let fade = |a: f32| Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), (255.0 * a.clamp(0.0, 1.0)) as u8);
        show.laser_trail.retain(|&(_, t)| now - t < 0.7);
        let mut raw: Vec<(f32, f32)> = show.laser_trail.iter().map(|&(p, _)| (p.x, p.y)).collect();
        let dot = ctx.input(|i| i.pointer.hover_pos());
        if let Some(p) = dot {
            raw.push((p.x, p.y));
        }
        let sm = smooth_stroke(&raw);
        let n = sm.len();
        let mut prev: Option<Pos2> = None;
        for (i, &(x, y)) in sm.iter().enumerate() {
            let p = pos2(x, y);
            if let Some(q) = prev {
                let t = i as f32 / n.max(1) as f32; // 0 at the tail's end → 1 at the dot
                painter.line_segment([q, p], Stroke::new(0.5 + 3.0 * t, fade(0.30 * t * t)));
            }
            prev = Some(p);
        }
        if let Some(p) = dot {
            painter.circle_filled(p, 13.0, fade(0.16));
            painter.circle_filled(p, 7.5, fade(0.35));
            painter.circle_filled(p, 4.5, col);
        }
    }
    // Keep-ink prompt (on exit with annotations, like PowerPoint).
    if show.ask_keep {
        painter.rect_filled(full, CornerRadius::ZERO, Color32::from_black_alpha(140));
        let dlg = Rect::from_center_size(full.center(), vec2(400.0, 140.0));
        painter.rect_filled(dlg, CornerRadius::same(10), Color32::from_gray(36));
        painter.rect_stroke(dlg, CornerRadius::same(10), Stroke::new(1.0, Color32::from_gray(90)), egui::StrokeKind::Inside);
        painter.text(pos2(dlg.center().x, dlg.min.y + 34.0), Align2::CENTER_CENTER, "Keep ink annotations?", theme::font(17.0), Color32::WHITE);
        let slides_with_ink = show.ink.by_slide().count();
        painter.text(
            pos2(dlg.center().x, dlg.min.y + 62.0),
            Align2::CENTER_CENTER,
            format!("{} stroke(s) on {} slide(s)", show.ink.count(), slides_with_ink),
            theme::font(12.0),
            Color32::from_gray(190),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_size(pos2(dlg.center().x - 100.0, dlg.max.y - 46.0), vec2(200.0, 30.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let btn = |ui: &mut Ui, s: &str| {
            ui.add(egui::Button::new(egui::RichText::new(s).color(Color32::WHITE).size(14.0)).fill(Color32::from_gray(70)).min_size(vec2(88.0, 26.0)))
        };
        if btn(&mut child, "Keep").clicked() {
            keep_answer = Some(true);
        }
        if btn(&mut child, "Discard").clicked() {
            keep_answer = Some(false);
        }
    }
    if show.rehearse {
        let t = now - show.slide_start;
        let total = now - show.show_start;
        let r = Rect::from_min_size(pos2(full.min.x + 12.0, full.min.y + 12.0), vec2(220.0, 30.0));
        painter.rect_filled(r, CornerRadius::same(6), Color32::from_black_alpha(160));
        painter.text(
            r.center(),
            Align2::CENTER_CENTER,
            format!("Slide {:02}:{:04.1}   Total {:02}:{:02}", (t / 60.0) as u32, t % 60.0, (total / 60.0) as u32, (total % 60.0) as u32),
            theme::font(13.0),
            Color32::WHITE,
        );
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    // Presenter view in a second window (closed while the modal keep-ink prompt is up).
    if show.presenter && !show.reading && !show.ended && !show.ask_keep {
        presenter_window(&ctx, &mut show, &doc, now, &mut keep);
    }
    // Exiting with annotations: ask first (PowerPoint's "Keep ink annotations?").
    if !keep && !show.ask_keep && !show.preview_only && show.ink.any() {
        show.ink.end();
        show.ink.end_erase();
        show.erase_slide = None;
        show.ask_keep = true;
        show.ask_since = now;
        keep = true;
    }
    // A scripted `show.end {keep}` answer applies as soon as the prompt is up (a manual
    // click this frame wins over a racing scripted answer).
    if show.ask_keep
        && keep_answer.is_none()
        && let Some(k) = show.pending_keep.take()
    {
        keep_answer = Some(k);
    }
    if let Some(keep_them) = keep_answer {
        show.ask_keep = false;
        // The prompt's dwell must not inflate rehearsed slide timings.
        show.slide_start += now - show.ask_since;
        if keep_them {
            let failed = keep_ink(app, &show, &doc);
            if failed == 0 {
                app.set_status("Ink annotations kept.");
            } else {
                app.set_status(format!("Ink annotations kept, but {failed} slide(s) failed."));
            }
        }
        keep = false;
    }
    if keep {
        app.show = Some(show);
    } else {
        app.media.stop_owned(crate::media::Owner::Show);
        if !show.reading {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        }
        if show.rehearse {
            if let Some((_, tl)) = &show.timeline {
                let _ = tl;
            }
            show.timings.push((show.state.slide, now - show.slide_start));
            for (i, secs) in show.timings.clone() {
                let _ = app.run("transition.timing", json!({"index": i, "after": (secs * 1000.0).round() as u64}));
            }
            app.set_status("Slide timings recorded.");
        }
        let last = show.state.slide;
        let _ = app.run("slide.go", json!({"index": last}));
    }
}

/// Start the media of a newly shown slide: media from the previous slide stops unless it plays
/// across slides; media set to start automatically plays. Everything stops at the end.
fn show_media(app: &mut SlideApp, show: &mut Show, doc: &Presentation) {
    let cur = if show.ended { None } else { Some(show.state.slide) };
    if show.media_slide == cur {
        return;
    }
    show.media_slide = cur;
    let keep: Vec<ShapeId> = match cur {
        Some(_) => app.media.ids().into_iter().filter(|id| crate::media::find_clip(doc, *id).is_some_and(|c| c.play_across_slides)).collect(),
        None => vec![],
    };
    for id in app.media.ids() {
        if !keep.contains(&id) {
            app.media.stop(id);
        }
    }
    if let Some(i) = cur {
        for (id, _, clip) in crate::media::slide_media(doc, i) {
            if clip.autoplay && !keep.contains(&id) {
                app.media.play(doc, id, &clip, None, crate::media::Owner::Show);
            }
        }
    }
}

/// Convert the show's ink annotations into document strokes through `slide.annotate`
/// (normalized → slide coordinates, clamped to the slide; the renderer applies its own
/// alpha for highlighter ink, so colors are stored opaque).
fn keep_ink(app: &mut SlideApp, show: &Show, doc: &Presentation) -> usize {
    let (sw, sh) = (doc.slide_size.width, doc.slide_size.height);
    let mut failed = 0;
    for (slide, strokes) in show.ink().by_slide() {
        let list: Vec<Value> = strokes
            .iter()
            .map(|s| {
                json!({
                    // Smoothed, so the kept ink matches what was on screen.
                    "points": smooth_stroke(&s.points).iter().map(|&(x, y)| [f64::from(x).clamp(0.0, 1.0) * sw, f64::from(y).clamp(0.0, 1.0) * sh]).collect::<Vec<_>>(),
                    "color": s.color.with_alpha(255).hex(),
                    "width": s.width,
                    "highlighter": s.highlighter,
                })
            })
            .collect();
        if !list.is_empty() && app.run("slide.annotate", json!({"index": slide, "strokes": list})).is_err() {
            failed += 1;
        }
    }
    failed
}

/// Paint show ink strokes (normalized coordinates) into a slide rect, smoothed
/// (`smooth_stroke`) so pen curves don't look like raw polylines.
fn paint_ink(painter: &egui::Painter, r: Rect, slide_w: f64, strokes: &[ShowStroke]) {
    let k = r.width() / slide_w.max(1.0) as f32;
    for s in strokes {
        let col = theme::to_color32(s.color);
        let w = (s.width as f32 * k).max(1.0);
        let raw: Vec<(f32, f32)> = s.points.iter().map(|&(x, y)| (r.min.x + x * r.width(), r.min.y + y * r.height())).collect();
        let pts: Vec<Pos2> = smooth_stroke(&raw).iter().map(|&(x, y)| pos2(x, y)).collect();
        match pts.as_slice() {
            [p] => {
                painter.circle_filled(*p, w / 2.0, col);
            }
            [_, ..] => {
                painter.add(egui::Shape::line(pts, Stroke::new(w, col)));
            }
            [] => {}
        }
    }
}

/// The media shape under the pointer on the shown slide.
/// One button of the show's control bars: a vector icon with hover and active fills; greyed
/// and inert when `enabled` is false.
fn bar_icon_button(ui: &mut Ui, icon: Icon, label: &str, on: bool, enabled: bool, tip: impl Into<String>) -> egui::Response {
    let resp = ui.add_enabled(enabled, egui::Button::new("").frame(false).min_size(vec2(26.0, 24.0)));
    if on {
        ui.painter().rect_filled(resp.rect, CornerRadius::same(6), Color32::from_white_alpha(46));
        ui.painter().rect_stroke(resp.rect, CornerRadius::same(6), Stroke::new(1.0, Color32::from_white_alpha(110)), egui::StrokeKind::Inside);
    } else if resp.hovered() {
        ui.painter().rect_filled(resp.rect, CornerRadius::same(6), Color32::from_white_alpha(20));
    }
    crate::icons::paint(ui.painter(), resp.rect.shrink(4.0), icon, Color32::WHITE, !enabled);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let tip = tip.into();
    let resp = resp.on_hover_text(tip.clone());
    if enabled {
        resp
    } else {
        // Disabled buttons don't hover in egui; without this the tooltip is silently lost.
        resp.on_disabled_hover_text(tip)
    }
}

/// Thin vertical divider between the bar's button groups.
fn bar_sep(ui: &mut Ui) {
    let (_, r) = ui.allocate_space(vec2(9.0, 24.0));
    ui.painter().vline(r.center().x, (r.top() + 5.0)..=(r.bottom() - 5.0), Stroke::new(1.0, Color32::from_white_alpha(50)));
}

fn media_hit(ctx: &egui::Context, ui: &Ui, show: &Show, doc: &Presentation) -> Option<(ShapeId, deckcraft_model::MediaClip)> {
    let p = ctx.input(|i| i.pointer.interact_pos())?;
    let full = ui.max_rect();
    let avail = if show.reading { Rect::from_min_max(full.min, pos2(full.max.x, full.max.y - 34.0)) } else { full };
    let srect = fit(avail, doc);
    let k = srect.width() / doc.slide_size.width.max(1.0) as f32;
    crate::media::slide_media(doc, show.state.slide).into_iter().rev().find_map(|(id, x, clip)| {
        let r = Rect::from_min_size(pos2(srect.min.x + x.x as f32 * k, srect.min.y + x.y as f32 * k), vec2(x.w as f32 * k, x.h as f32 * k));
        r.contains(p).then_some((id, clip))
    })
}

/// The link under the pointer: the topmost shape action or text hyperlink on the current slide
/// (a text link wins over its shape's action).
fn link_hit(ctx: &egui::Context, ui: &Ui, show: &mut Show, doc: &Presentation) -> Option<Hyperlink> {
    let p = ctx.input(|i| i.pointer.hover_pos())?;
    let full = ui.max_rect();
    let avail = if show.reading { Rect::from_min_max(full.min, pos2(full.max.x, full.max.y - 34.0)) } else { full };
    let srect = fit(avail, doc);
    let k = (srect.width() / doc.slide_size.width.max(1.0) as f32) as f64;
    let (x, y) = ((p.x - srect.min.x) as f64 / k, (p.y - srect.min.y) as f64 / k);
    let i = show.state.slide;
    if show.links.as_ref().is_none_or(|(s, _)| *s != i) {
        show.links = Some((i, deckcraft_render::place_slide(doc, i).links));
    }
    let (_, links) = show.links.as_ref()?;
    links.iter().rev().find(|l| l.rect.x0 <= x && x <= l.rect.x1 && l.rect.y0 <= y && y <= l.rect.y1).map(|l| l.link.clone())
}

/// A slide-space box on screen, the slide being drawn in `srect` at `k` pixels per point.
fn on_screen(srect: Rect, k: f32, x: &deckcraft_geom::Xfrm) -> Rect {
    Rect::from_min_size(pos2(srect.min.x + x.x as f32 * k, srect.min.y + x.y as f32 * k), vec2(x.w as f32 * k, x.h as f32 * k))
}

/// The animated GIF whose play/pause button is under the pointer on the shown slide.
fn gif_hit(ctx: &egui::Context, ui: &Ui, show: &Show, doc: &Presentation, app: &mut SlideApp) -> Option<ShapeId> {
    let p = ctx.input(|i| i.pointer.interact_pos())?;
    let full = ui.max_rect();
    let avail = if show.reading { Rect::from_min_max(full.min, pos2(full.max.x, full.max.y - 34.0)) } else { full };
    let srect = fit(avail, doc);
    let k = srect.width() / doc.slide_size.width.max(1.0) as f32;
    let gifs = app.media.gifs(doc, show.state.slide, ctx.input(|i| i.time), &app.session.gif_paused);
    gifs.iter().rev().find(|g| crate::media::gif_button_rect(on_screen(srect, k, &g.rect)).contains(p)).map(|g| g.id)
}

impl Show {
    /// The show's ink state (for the control channel's `ui.inspect`).
    pub fn ink(&self) -> &ShowInk {
        &self.ink
    }

    /// The keep-ink prompt is up (for the control channel).
    pub fn ask_keep(&self) -> bool {
        self.ask_keep
    }

    /// Queue a scripted answer for the keep-ink prompt (`show.end {keep}`).
    pub fn set_pending_keep(&mut self, keep: bool) {
        self.pending_keep = Some(keep);
    }

    /// Width of the bottom-left control bar: the measured content width (full or compact per
    /// the roomy decision, so content never spills past the pill), clamped to the window.
    fn bar_width(&self, window_w: f32) -> f32 {
        let roomy = self.bar_full.is_none_or(|fw| fw + 40.0 <= window_w);
        let w = if roomy { self.bar_full.or(self.bar_measured).unwrap_or(780.0) } else { self.bar_measured.unwrap_or(780.0) };
        w.min((window_w - 40.0).max(120.0))
    }

    fn reading_bar_hit(&self, ctx: &egui::Context) -> bool {
        // Clicks on the on-screen controls must not also advance. Uses the bar's actual rect
        // (recorded when drawn), so a hidden bar never swallows clicks.
        ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| {
            let r = ctx.content_rect();
            (self.reading && p.y > r.max.y - 34.0) || (!self.reading && self.bar_rect.is_some_and(|bar| bar.contains(p)))
        })
    }
}

fn presenter_window(ctx: &egui::Context, show: &mut Show, doc: &Presentation, now: f64, keep: &mut bool) {
    let cur = show.state.slide;
    let next = deckcraft_anim::show_order(doc).into_iter().find(|i| *i > cur);
    let cur_tex = show.tex.as_ref().map(|(_, t)| t.clone());
    if let Some(n) = next
        && show.next_tex.as_ref().is_none_or(|(k, _)| *k != n)
    {
        let img = deckcraft_render::render_slide(
            doc,
            n,
            &RenderOpts {
                scale: 480.0 / doc.slide_size.width.max(1.0),
                size: Some((480, (480.0 * doc.slide_size.height / doc.slide_size.width.max(1.0)) as u32)),
                ..Default::default()
            },
        );
        show.next_tex = Some((n, ctx.load_texture("presenter-next", crate::textures::to_color_image(&img, false), egui::TextureOptions::LINEAR)));
    }
    let next_tex = show.next_tex.as_ref().filter(|(k, _)| Some(*k) == next).map(|(_, t)| t.clone());
    let notes = doc.slides.get(cur).map(|s| s.notes_text()).unwrap_or_default();
    let elapsed = now - show.show_start;
    let mut actions = vec![];
    ctx.show_viewport_immediate(
        egui::ViewportId::from_hash_of("deckcraft_presenter"),
        egui::ViewportBuilder::default().with_title("Presenter View — DeckCraft").with_inner_size([1100.0, 700.0]),
        |ui, _| {
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::from_gray(24)).inner_margin(egui::Margin::same(16))).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{:02}:{:02}:{:02}",
                            (elapsed / 3600.0) as u32,
                            (elapsed / 60.0) as u32 % 60,
                            elapsed as u32 % 60
                        ))
                        .size(22.0)
                        .color(Color32::WHITE),
                    );
                    ui.add_space(20.0);
                    ui.label(egui::RichText::new(format!("Slide {} of {}", cur + 1, doc.slides.len())).size(16.0).color(Color32::from_gray(200)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("End Show").clicked() {
                            *keep = false;
                        }
                        if ui.button("Next ▶").clicked() {
                            actions.push(show.state.next(doc));
                        }
                        if ui.button("◀ Previous").clicked() {
                            actions.push(show.state.prev(doc));
                        }
                    });
                });
                ui.add_space(10.0);
                let avail = ui.available_rect_before_wrap();
                let left = Rect::from_min_max(avail.min, pos2(avail.min.x + avail.width() * 0.62, avail.max.y));
                let right = Rect::from_min_max(pos2(left.max.x + 16.0, avail.min.y), avail.max);
                let cr = fit(Rect::from_min_max(left.min, pos2(left.max.x, left.min.y + left.height() * 0.8)), doc);
                let cr = Rect::from_min_size(left.min, cr.size());
                if let Some(t) = &cur_tex {
                    ui.painter().image(t.id(), cr, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                }
                // Mirror the audience's ink so the presenter sees the annotations too
                // (clipped to the slide; hidden during transitions like the audience view).
                if show.trans.is_none() {
                    let ink_painter = ui.painter().with_clip_rect(cr);
                    paint_ink(&ink_painter, cr, doc.slide_size.width, show.ink.strokes(cur));
                }
                let nr = Rect::from_min_size(
                    right.min + vec2(0.0, 22.0),
                    vec2(right.width(), right.width() * (doc.slide_size.height / doc.slide_size.width.max(1.0)) as f32),
                );
                ui.painter().text(right.min, Align2::LEFT_TOP, "Next", theme::font(13.0), Color32::from_gray(170));
                match &next_tex {
                    Some(t) => {
                        ui.painter().image(t.id(), nr, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    None => {
                        ui.painter().rect_filled(nr, CornerRadius::ZERO, Color32::from_gray(40));
                        ui.painter().text(nr.center(), Align2::CENTER_CENTER, "End of slide show", theme::font(13.0), Color32::from_gray(170));
                    }
                }
                let notes_rect = Rect::from_min_max(pos2(right.min.x, nr.max.y + 16.0), right.max);
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(notes_rect));
                egui::ScrollArea::vertical().show(&mut child, |ui| {
                    ui.label(egui::RichText::new(if notes.is_empty() { "No notes." } else { &notes }).size(18.0).color(Color32::from_gray(230)));
                });
            });
            if ui.input(|i| i.viewport().close_requested()) {
                show.presenter = false;
            }
        },
    );
    for a in actions {
        if !show.apply(a, doc, now) {
            *keep = false;
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn a_transition_keeps_the_old_slide_in_its_own_texture() {
        // #33: the old slide's handle was cloned, so the new slide was rendered into the same
        // texture and every transition blended the new slide with itself.
        let mut s = deckcraft_engine::Session::with_new();
        s.execute("slide.new", &json!({})).unwrap();
        s.execute("transition.set", &json!({"kind": "fade", "index": 1})).unwrap();
        let doc = s.active().unwrap().doc.clone();
        let ctx = egui::Context::default();
        let red = ctx.load_texture("show", egui::ColorImage::filled([1, 1], Color32::RED), egui::TextureOptions::LINEAR);
        let mut show = Show::new(&doc, 0, false, false);
        show.tex = Some((Key { slide: 0, step: 0, size: (1, 1), doc: 0, hidden: vec![], gif: 0 }, red.clone()));
        assert!(show.apply(ShowAction::Slide(1), &doc, 0.0));
        let tr = show.trans.as_ref().expect("the fade plays");
        assert_eq!(tr.old.as_ref().map(TextureHandle::id), Some(red.id()));
        assert_eq!(tr.from, Some(0));
        assert!(show.tex.is_none(), "the new slide gets a texture of its own instead of overwriting the old one");
    }
}
