//! Slide Show: full-screen playback with transitions and animations (`deckcraft-anim`),
//! keyboard/mouse navigation, pen, black/white screens, presenter view, rehearsed timings
//! and recorded timings, narration, ink and laser.

use deckcraft_anim::{Layer, LinkJump, ShowAction, ShowState, Source, Timeline};
use deckcraft_model::text::Hyperlink;
use deckcraft_model::{Presentation, ShapeId};
use deckcraft_render::{ParaState, RenderOpts, ShapeState};
use egui::{Align2, Color32, CornerRadius, Mesh, Pos2, Rect, Sense, Stroke, TextureHandle, Ui, pos2, vec2};
use serde_json::json;

use crate::SlideApp;
use crate::theme;

pub struct Show {
    pub state: ShowState,
    pub reading: bool,
    pub presenter: bool,
    /// Preview from the Transitions/Animations tab: exits after the current slide.
    pub preview_only: bool,
    /// Play every step automatically (animation preview).
    pub autoplay: bool,
    pub rehearse: bool,
    /// Rehearse Timings is asking whether to keep the new slide times.
    pub ask_keep: bool,
    /// Yes on that prompt. Esc discards.
    keep_timings: bool,
    /// Slide and total clocks for Rehearse Timings. Pause freezes both; Repeat restarts the slide.
    clock: RehearseClock,
    /// Record Slide Show: keep timings, narration, pen ink and laser when the show ends.
    pub record: bool,
    timeline: Option<(usize, Timeline)>,
    step_start: f64,
    slide_start: f64,
    step_end: f64,
    trans: Option<Transition>,
    tex: Option<(Key, TextureHandle)>,
    next_tex: Option<(usize, TextureHandle)>,
    blank: Option<Color32>,
    pen: bool,
    /// Laser pointer left on (the Laser button, or L while recording). Hold Ctrl for the same
    /// dot in any show. It leaves no ink; points are kept only while recording.
    laser: bool,
    strokes: Vec<Vec<Pos2>>,
    goto: String,
    started: bool,
    show_start: f64,
    timings: Vec<(usize, f64)>,
    /// Pen strokes kept while recording, in slide points, with the slide they were drawn on.
    recorded: Vec<(usize, Vec<(f64, f64)>)>,
    /// Laser samples kept while recording: slide points and milliseconds from the slide start.
    laser_live: Vec<(f64, f64, u32)>,
    /// Where the laser dot is on the slide this frame (0–1), so the presenter window can draw it.
    laser_dot: Option<(f32, f32, Color32)>,
    laser_saved: Vec<(usize, Vec<(f64, f64, u32)>)>,
    /// WAV bytes per slide, taken from the microphone when the slide is left.
    narration: Vec<(usize, Vec<u8>)>,
    mic_tried: bool,
    mic_ok: bool,
    /// Pen width in slide points, matching the 3px stroke on screen.
    ink_width: f64,
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
    /// Backgrounds and contents of the old and new slides, rendered once when the transition
    /// moves contents over backgrounds (Pan, Ferris Wheel, Reveal…): old back, old content, new
    /// back, new content.
    parts: Option<[Option<TextureHandle>; 4]>,
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
            ask_keep: false,
            keep_timings: false,
            clock: RehearseClock::start(0.0),
            record: false,
            timeline: None,
            step_start: 0.0,
            slide_start: 0.0,
            step_end: 0.0,
            trans: None,
            tex: None,
            next_tex: None,
            blank: None,
            pen: false,
            laser: false,
            strokes: vec![],
            goto: String::new(),
            started: false,
            show_start: 0.0,
            timings: vec![],
            recorded: vec![],
            laser_live: vec![],
            laser_dot: None,
            laser_saved: vec![],
            narration: vec![],
            mic_tried: false,
            mic_ok: false,
            ink_width: 2.25,
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
                let left = self.timeline.as_ref().map(|t| t.0).unwrap_or(i);
                if self.rehearse || self.record {
                    let wall = now - self.slide_start;
                    let secs = if self.rehearse && !self.record { self.clock.slide(now) } else { wall };
                    let secs = if secs.is_finite() { secs.max(0.0) } else { 0.0 };
                    self.timings.push((left, secs));
                    if self.rehearse && !self.record {
                        self.clock.advance(now);
                    }
                }
                self.stash_ink(doc, left);
                self.stash_laser(left);
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
                    Some(Transition { old, from, morph: None, parts: None, start: now, kind, option, dur })
                } else {
                    None
                };
                self.slide_start = now;
                self.step_start = now;
                self.step_end = now;
                self.strokes.clear();
                if self.preview_only {
                    return false;
                }
                self.after_enter(doc, now);
            }
            ShowAction::End => {
                self.ended = true;
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

    /// Move the pen's strokes into the recording, in slide points. Always clears the live strokes.
    fn stash_ink(&mut self, doc: &Presentation, slide: usize) {
        let strokes = std::mem::take(&mut self.strokes);
        let w = doc.slide_size.width;
        let h = doc.slide_size.height;
        if !self.record || !(w > 0.0 && h > 0.0) {
            return;
        }
        for stroke in strokes {
            let mut pts = Vec::new();
            for q in stroke.iter().take(4096) {
                let x = q.x as f64 * w;
                let y = q.y as f64 * h;
                if x.is_finite() && y.is_finite() {
                    pts.push((x, y));
                }
            }
            if pts.len() >= 2 {
                self.recorded.push((slide, pts));
            }
        }
    }

    fn stash_laser(&mut self, slide: usize) {
        let pts = std::mem::take(&mut self.laser_live);
        if self.record && !pts.is_empty() {
            self.laser_saved.push((slide, pts));
        }
    }

    fn note_laser(&mut self, x: f64, y: f64, t_ms: u32) {
        if !x.is_finite() || !y.is_finite() || self.laser_live.len() >= 4096 {
            return;
        }
        if let Some((px, py, pt)) = self.laser_live.last() {
            let moved = (x - px).hypot(y - py) >= 1.5;
            if t_ms.saturating_sub(*pt) < 40 && !moved {
                return;
            }
        }
        self.laser_live.push((x, y, t_ms));
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

fn record_bar_rect(full: Rect) -> Rect {
    Rect::from_min_size(pos2(full.min.x + 12.0, full.min.y + 12.0), vec2(420.0, 32.0))
}

fn pen_width_slide(doc: &Presentation, full: Rect, reading: bool) -> f64 {
    let avail = if reading { Rect::from_min_max(full.min, pos2(full.max.x, full.max.y - 34.0)) } else { full };
    let srect = fit(avail, doc);
    let scale = srect.width() / doc.slide_size.width.max(1.0) as f32;
    if scale.is_finite() && scale > 0.01 { (3.0 / scale) as f64 } else { 2.25 }
}

/// The recording badge. Returns false when Stop was clicked.
fn recording_bar(ui: &mut Ui, painter: &egui::Painter, ctx: &egui::Context, full: Rect, now: f64, show: &mut Show, mic_samples: usize) -> bool {
    let t = now - show.slide_start;
    let secs = if t.is_finite() { t.max(0.0) } else { 0.0 };
    let r = record_bar_rect(full);
    painter.rect_filled(r, CornerRadius::same(6), Color32::from_black_alpha(180));
    painter.circle_filled(pos2(r.min.x + 16.0, r.center().y), 5.0, Color32::from_rgb(220, 40, 40));
    let clock = format!("{:02}:{:04.1}", (secs / 60.0) as u32, secs % 60.0);
    let mic_live = mic_samples;
    let label = if show.mic_tried && !show.mic_ok {
        format!("Recording  {clock}  · no mic")
    } else if mic_live >= 800 {
        format!("Recording  {clock}  · mic")
    } else {
        format!("Recording  {clock}")
    };
    painter.text(pos2(r.min.x + 30.0, r.center().y), Align2::LEFT_CENTER, label, theme::font(13.0), Color32::WHITE);
    let laser = Rect::from_min_size(pos2(r.max.x - 124.0, r.min.y + 4.0), vec2(56.0, 24.0));
    let lresp = ui.interact(laser, ui.id().with("record_laser"), Sense::click());
    if lresp.clicked() {
        show.laser = !show.laser;
        if show.laser {
            show.pen = false;
        }
    }
    let laser_fill = if show.laser {
        Color32::from_rgb(180, 40, 40)
    } else if lresp.hovered() {
        Color32::from_rgb(80, 80, 80)
    } else {
        Color32::from_rgb(50, 50, 50)
    };
    painter.rect_filled(laser, CornerRadius::same(4), laser_fill);
    painter.text(laser.center(), Align2::CENTER_CENTER, "Laser", theme::font(12.0), Color32::WHITE);
    let stop = Rect::from_min_size(pos2(r.max.x - 62.0, r.min.y + 4.0), vec2(54.0, 24.0));
    let resp = ui.interact(stop, ui.id().with("record_stop"), Sense::click());
    painter.rect_filled(stop, CornerRadius::same(4), if resp.hovered() { Color32::from_rgb(180, 40, 40) } else { Color32::from_rgb(140, 32, 32) });
    painter.text(stop.center(), Align2::CENTER_CENTER, "Stop", theme::font(12.0), Color32::WHITE);
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
    !resp.clicked()
}

fn fit(avail: Rect, doc: &Presentation) -> Rect {
    let (sw, sh) = (doc.slide_size.width as f32, doc.slide_size.height as f32);
    let s = (avail.width() / sw).min(avail.height() / sh);
    Rect::from_center_size(avail.center(), vec2(sw * s, sh * s))
}

/// A transition layer as an egui mesh: [`deckcraft_anim::layer_mesh`]'s triangles (perspective
/// cards subdivided so their pictures stay straight) with per-vertex opacity.
fn quad_mesh(tex: Option<egui::TextureId>, rect: Rect, l: &Layer, color: Color32) -> Mesh {
    let mut m = match tex {
        Some(id) => Mesh::with_texture(id),
        None => Mesh::default(),
    };
    let (verts, tris) = deckcraft_anim::layer_mesh(l);
    for v in &verts {
        let a = if v.a.is_finite() { v.a.clamp(0.0, 1.0) } else { 0.0 };
        let col = if tex.is_some() { Color32::from_white_alpha((a * 255.0).round() as u8) } else { color.gamma_multiply(a as f32) };
        let p = pos2(rect.min.x + v.pos[0] as f32 * rect.width(), rect.min.y + v.pos[1] as f32 * rect.height());
        m.vertices.push(egui::epaint::Vertex { pos: p, uv: pos2(v.uv[0] as f32, v.uv[1] as f32), color: col });
    }
    for t in tris {
        m.add_triangle(t[0], t[1], t[2]);
    }
    m
}

/// Render the backgrounds and contents of a transition's two slides into textures, once.
fn part_textures(ctx: &egui::Context, doc: &Presentation, from: Option<usize>, to: usize, size: (u32, u32)) -> [Option<TextureHandle>; 4] {
    use deckcraft_render::SlidePart;
    let threads = if cfg!(target_arch = "wasm32") { 0 } else { std::thread::available_parallelism().map(|n| n.get().min(8) as u16).unwrap_or(0) };
    let opts = RenderOpts { scale: size.0 as f64 / doc.slide_size.width.max(1.0), size: Some(size), threads, ..Default::default() };
    let tex = |i: usize, part: SlidePart, name: &str| {
        let img = deckcraft_render::render_slide_part(doc, i, part, &opts);
        ctx.load_texture(name, crate::textures::to_color_image(&img, false), egui::TextureOptions::LINEAR)
    };
    [
        from.map(|i| tex(i, SlidePart::Backdrop, "trans-old-back")),
        from.map(|i| tex(i, SlidePart::Content, "trans-old-content")),
        Some(tex(to, SlidePart::Backdrop, "trans-new-back")),
        Some(tex(to, SlidePart::Content, "trans-new-content")),
    ]
}

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
        show.clock = RehearseClock::start(now);
        show.last_move = now;
        if !show.reading {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        }
        show.after_enter(&doc, now);
        if show.record && !show.mic_tried {
            show.mic_tried = true;
            match app.services.mic.as_mut() {
                Some(mic) => match mic.start() {
                    Ok(()) => show.mic_ok = true,
                    Err(e) => {
                        log::info!("record narration: {e}");
                        show.mic_ok = false;
                    }
                },
                None => show.mic_ok = false,
            }
        }
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
    let mut actions: Vec<ShowAction> = vec![];
    if show.ask_keep && keys.iter().any(|(k, _)| matches!(k, egui::Key::Escape | egui::Key::Minus)) {
        keep = false;
    }
    for (k, m) in keys {
        use egui::Key::*;
        if show.ask_keep {
            break;
        }
        match k {
            Escape | Minus => leave_show(&mut show, now, &mut keep),
            ArrowRight | ArrowDown | Space | PageDown | N => {
                if show.ended {
                    leave_show(&mut show, now, &mut keep);
                } else {
                    actions.push(show.state.next(&doc));
                }
            }
            Enter => {
                if let Ok(n) = show.goto.parse::<usize>() {
                    actions.push(show.state.goto(&doc, n.saturating_sub(1)));
                    show.goto.clear();
                } else if show.ended {
                    leave_show(&mut show, now, &mut keep);
                } else {
                    actions.push(show.state.next(&doc));
                }
            }
            ArrowLeft | ArrowUp | Backspace | PageUp | P if !(k == P && m.command) => {
                show.ended = false;
                actions.push(show.state.prev(&doc));
            }
            P if m.command => {
                show.pen = !show.pen;
                if show.pen {
                    show.laser = false;
                }
            }
            L if show.record => {
                show.laser = !show.laser;
                if show.laser {
                    show.pen = false;
                }
            }
            A if m.command => show.pen = false,
            Home => actions.push(show.state.goto(&doc, 0)),
            End => actions.push(show.state.goto(&doc, doc.slides.len().saturating_sub(1))),
            B | Period => show.blank = if show.blank == Some(Color32::BLACK) { None } else { Some(Color32::BLACK) },
            W | Comma => show.blank = if show.blank == Some(Color32::WHITE) { None } else { Some(Color32::WHITE) },
            E => {
                show.strokes.clear();
                show.laser_live.clear();
            }
            Num0 | Num1 | Num2 | Num3 | Num4 | Num5 | Num6 | Num7 | Num8 | Num9 => {
                show.goto.push_str(k.name());
            }
            _ => {}
        }
    }
    let ctrl = ctx.input(|i| i.modifiers.ctrl);
    let laser_on = pointer_is_laser(show.laser, ctrl, show.blank.is_some());
    let on_slide = !show.ask_keep
        && !show.pen
        && !laser_on
        && !show.ended
        && show.blank.is_none()
        && !show.reading_bar_hit(&ctx)
        && !show.record_bar_hit(&ctx)
        && !show.rehearse_bar_hit(&ctx);
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
            LinkJump::Exit => leave_show(&mut show, now, &mut keep),
            LinkJump::None => {}
        }
    } else if clicked
        && !show.ask_keep
        && !show.pen
        && !laser_on
        && !show.reading_bar_hit(&ctx)
        && !show.record_bar_hit(&ctx)
        && !show.rehearse_bar_hit(&ctx)
    {
        if show.ended {
            leave_show(&mut show, now, &mut keep);
        } else if show.blank.is_some() {
            show.blank = None;
        } else {
            actions.push(show.state.next(&doc));
        }
    }
    if secondary && !show.ask_keep && !show.ended && !laser_on && !show.record_bar_hit(&ctx) && !show.rehearse_bar_hit(&ctx) {
        actions.push(show.state.prev(&doc));
    }
    // Apply input first: a step started by this click must restart the clock before it is read.
    for a in actions.drain(..) {
        run_action(app, &mut show, &doc, now, a, &mut keep);
    }
    // Step clock.
    let (step, playing) = (show.state.step, show.state.playing);
    let t_step = if playing { now - show.step_start } else { 0.0 };
    if playing {
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
        if !show.ask_keep && (step_due || (!(show.rehearse || show.record) && show.state.auto_advance_due(&doc, idle) && !show.ended)) {
            actions.push(show.state.next(&doc));
        } else if show.preview_only && cur_step >= n_steps && idle > 0.8 {
            keep = false;
        }
    }
    show.ink_width = pen_width_slide(&doc, ui.max_rect(), show.reading);
    for a in actions {
        run_action(app, &mut show, &doc, now, a, &mut keep);
    }
    show_media(app, &mut show, &doc);
    // Draw.
    let full = ui.max_rect();
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
        } else if let (Some(tr), Some(new)) = (show.trans.as_mut(), &tex) {
            let p = ((now - tr.start) / tr.dur.max(0.01)).clamp(0.0, 1.0);
            if p < 1.0 {
                // Slides split into background and content are rendered once per transition;
                // every frame only composites cached textures.
                if tr.parts.is_none() && deckcraft_anim::uses_slide_parts(&tr.kind, &tr.option) {
                    tr.parts = Some(part_textures(&ctx, &doc, tr.from, idx, size));
                }
                let aspect = doc.slide_size.width / doc.slide_size.height.max(1.0);
                let part = |k: usize| tr.parts.as_ref().and_then(|p| p.get(k).cloned().flatten());
                let parts = [part(0), part(1), part(2), part(3)];
                for l in deckcraft_anim::transition_frame(&tr.kind, &tr.option, p, aspect) {
                    let old_or_black = |t: Option<&TextureHandle>| match t {
                        Some(o) => quad_mesh(Some(o.id()), srect, &l, Color32::WHITE),
                        None => quad_mesh(None, srect, &l, Color32::BLACK),
                    };
                    let mesh = match l.source {
                        Source::Old => old_or_black(tr.old.as_ref()),
                        Source::New => quad_mesh(Some(new.id()), srect, &l, Color32::WHITE),
                        Source::Black => quad_mesh(None, srect, &l, Color32::BLACK),
                        Source::White => quad_mesh(None, srect, &l, Color32::WHITE),
                        Source::OldBack => old_or_black(parts[0].as_ref().or(tr.old.as_ref())),
                        Source::NewBack => quad_mesh(Some(parts[2].as_ref().unwrap_or(new).id()), srect, &l, Color32::WHITE),
                        Source::OldContent => match &parts[1] {
                            Some(t) => quad_mesh(Some(t.id()), srect, &l, Color32::WHITE),
                            None => continue,
                        },
                        Source::NewContent => match &parts[3] {
                            Some(t) => quad_mesh(Some(t.id()), srect, &l, Color32::WHITE),
                            None => continue,
                        },
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
            if let Some(p) = ctx.input(|i| i.pointer.hover_pos()).filter(|_| !show.pen && now - show.last_move < 2.5) {
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
    // Pen.
    let resp = ui.interact(full, ui.id().with("show_pen"), if show.pen && !laser_on { Sense::drag() } else { Sense::hover() });
    if show.pen && !laser_on {
        if resp.drag_started() {
            show.strokes.push(vec![]);
        }
        if resp.dragged()
            && let (Some(p), Some(s)) = (resp.interact_pointer_pos(), show.strokes.last_mut())
        {
            s.push(pos2((p.x - srect.min.x) / srect.width(), (p.y - srect.min.y) / srect.height()));
        }
        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
    } else if laser_on {
        ctx.set_cursor_icon(egui::CursorIcon::None);
        ctx.request_repaint();
    } else if link.is_some() {
        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    } else if now - show.last_move > 2.5 && !show.reading {
        ctx.set_cursor_icon(egui::CursorIcon::None);
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }
    let pen_col = theme::to_color32(doc.show.pen_color);
    for s in &show.strokes {
        let pts: Vec<Pos2> = s.iter().map(|q| pos2(srect.min.x + q.x * srect.width(), srect.min.y + q.y * srect.height())).collect();
        painter.add(egui::Shape::line(pts, Stroke::new(3.0, pen_col)));
    }
    show.laser_dot = None;
    if laser_on {
        if let Some(p) = ctx.input(|i| i.pointer.hover_pos()) {
            let nx = (p.x - srect.min.x) / srect.width().max(1.0);
            let ny = (p.y - srect.min.y) / srect.height().max(1.0);
            let col = theme::to_color32(doc.show.laser_color);
            if (0.0..=1.0).contains(&nx) && (0.0..=1.0).contains(&ny) {
                show.laser_dot = Some((nx, ny, col));
                if show.record {
                    let ms = (now - show.slide_start).max(0.0) * 1000.0;
                    if ms.is_finite() {
                        show.note_laser(nx as f64 * doc.slide_size.width, ny as f64 * doc.slide_size.height, ms.min(3_600_000.0) as u32);
                    }
                }
            }
            paint_laser_dot(&painter, p, col, 7.0);
        }
    } else if !show.record
        && let Some(sl) = doc.slides.get(show.state.slide)
        && let Some(trace) = &sl.laser
    {
        let elapsed = (now - show.slide_start).max(0.0) * 1000.0;
        if elapsed.is_finite()
            && let Some((x, y)) = laser_pos(&trace.points, elapsed.min(3_600_000.0) as u32)
        {
            let k = srect.width() / doc.slide_size.width.max(1.0) as f32;
            let p = pos2(srect.min.x + x as f32 * k, srect.min.y + y as f32 * k);
            let col = theme::to_color32(trace.color);
            let nx = (x / doc.slide_size.width.max(1.0)) as f32;
            let ny = (y / doc.slide_size.height.max(1.0)) as f32;
            if (0.0..=1.0).contains(&nx) && (0.0..=1.0).contains(&ny) {
                show.laser_dot = Some((nx, ny, col));
            }
            paint_laser_dot(&painter, p, col, 7.0);
            ctx.request_repaint();
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
        if child.button("Close").clicked() {
            leave_show(&mut show, now, &mut keep);
        }
        if child.button("▶").clicked() {
            let a = show.state.next(&doc);
            run_action(app, &mut show, &doc, now, a, &mut keep);
        }
        if child.button("◀").clicked() {
            let a = show.state.prev(&doc);
            run_action(app, &mut show, &doc, now, a, &mut keep);
        }
    } else if now - show.last_move < 2.5 {
        let bar = Rect::from_min_size(pos2(full.min.x + 20.0, full.max.y - 50.0), vec2(310.0, 34.0));
        painter.rect_filled(bar, CornerRadius::same(17), Color32::from_black_alpha(140));
        let mut child =
            ui.new_child(egui::UiBuilder::new().max_rect(bar.shrink2(vec2(10.0, 4.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
        let b = |ui: &mut Ui, s: &str| ui.add(egui::Button::new(egui::RichText::new(s).color(Color32::WHITE).size(15.0)).frame(false));
        if b(&mut child, "◀").clicked() {
            let a = show.state.prev(&doc);
            run_action(app, &mut show, &doc, now, a, &mut keep);
        }
        if b(&mut child, "▶").clicked() {
            let a = show.state.next(&doc);
            run_action(app, &mut show, &doc, now, a, &mut keep);
        }
        if b(&mut child, if show.pen { "✏ on" } else { "✏" }).clicked() {
            show.pen = !show.pen;
            if show.pen {
                show.laser = false;
            }
        }
        if b(&mut child, if show.laser { "● on" } else { "●" }).on_hover_text("Laser pointer. Hold Ctrl to point without leaving it on.").clicked()
        {
            show.laser = !show.laser;
            if show.laser {
                show.pen = false;
            }
        }
        if b(&mut child, "⬛").clicked() {
            show.blank = Some(Color32::BLACK);
        }
        if b(&mut child, "End").clicked() {
            leave_show(&mut show, now, &mut keep);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(300));
    }
    if show.record {
        let mic_samples = app.services.mic.as_ref().map(|m| m.captured()).unwrap_or(0);
        keep = recording_bar(ui, &painter, &ctx, full, now, &mut show, mic_samples) && keep;
    } else if show.rehearse {
        keep = rehearse_bar(ui, &painter, &ctx, full, now, &mut show) && keep;
    }
    // Presenter view in a second window.
    if show.presenter && !show.reading && !show.ended {
        presenter_window(app, &ctx, &mut show, &doc, now, &mut keep);
    }
    if keep {
        app.show = Some(show);
    } else {
        app.media.stop_owned(crate::media::Owner::Show);
        if !show.reading {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        }
        if show.record {
            take_narration(app, &mut show);
            if show.mic_tried {
                app.stop_mic();
            }
            show.stash_ink(&doc, show.state.slide);
            show.stash_laser(show.state.slide);
            let wall = now - show.slide_start;
            show.timings.push((show.state.slide, if wall.is_finite() { wall.max(0.0) } else { 0.0 }));
            for (i, secs) in show.timings.clone() {
                let ms = if secs.is_finite() { (secs.max(0.0) * 1000.0).round() as u64 } else { 0 };
                let _ = app.run("transition.timing", json!({"index": i, "after": ms}));
            }
            let c = doc.show.pen_color;
            for (i, pts) in &show.recorded {
                let points: Vec<_> = pts.iter().map(|(x, y)| json!([x, y, 0.5])).collect();
                let _ = app.run("ink.stroke", json!({"index": i, "points": points, "width": show.ink_width, "color": [c.r, c.g, c.b]}));
            }
            for (i, wav) in &show.narration {
                let _ = app.run("show.narration", json!({"index": i, "name": "narration.wav", "data": deckcraft_engine::cmd::base64_encode(wav)}));
            }
            let lc = doc.show.laser_color;
            for (i, pts) in &show.laser_saved {
                let points: Vec<_> = pts.iter().map(|(x, y, t)| json!([x, y, t])).collect();
                let _ = app.run("show.laser", json!({"index": i, "points": points, "color": [lc.r, lc.g, lc.b]}));
            }
            let _ = app.run("show.setup", json!({"useTimings": true}));
            app.set_status(saved_status(!show.narration.is_empty(), !show.recorded.is_empty(), !show.laser_saved.is_empty()));
        } else if show.rehearse && show.keep_timings {
            show.timings.push((show.state.slide, show.clock.slide(now)));
            for (i, secs) in show.timings.clone() {
                let ms = timing_ms(secs);
                let _ = app.run("transition.timing", json!({"index": i, "after": ms}));
            }
            let _ = app.run("show.setup", json!({"useTimings": true}));
            app.set_status("Slide timings kept.");
        } else if show.rehearse {
            app.set_status("New timings discarded.");
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
            if play_on_enter(&clip, doc.show.without_narration) && !keep.contains(&id) {
                app.media.play(doc, id, &clip, None, crate::media::Owner::Show);
            }
        }
    }
}

/// The media shape under the pointer on the shown slide.
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
    pub fn rehearse_paused(&self) -> bool {
        self.clock.paused
    }

    pub fn rehearse_times(&self, now: f64) -> (f64, f64) {
        (self.clock.slide(now), self.clock.total(now))
    }

    fn arm_prompt(&mut self, now: f64) {
        if !self.ask_keep {
            self.clock.pause(now);
            self.ask_keep = true;
        }
    }

    fn record_bar_hit(&self, ctx: &egui::Context) -> bool {
        if !self.record {
            return false;
        }
        ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| record_bar_rect(ctx.content_rect()).contains(p))
    }

    fn rehearse_bar_hit(&self, ctx: &egui::Context) -> bool {
        if !self.rehearse || self.record {
            return false;
        }
        let r = ctx.content_rect();
        ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| rehearse_bar_rect(r).contains(p) || (self.ask_keep && keep_panel(r).contains(p)))
    }

    fn reading_bar_hit(&self, ctx: &egui::Context) -> bool {
        // Clicks on the on-screen controls must not also advance.
        ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| {
            let r = ctx.content_rect();
            (self.reading && p.y > r.max.y - 34.0) || (!self.reading && p.y > r.max.y - 56.0 && p.x < r.min.x + 340.0)
        })
    }
}

fn leave_show(show: &mut Show, now: f64, keep: &mut bool) {
    if show.rehearse && !show.record && !show.preview_only && !show.ask_keep {
        show.arm_prompt(now);
    } else {
        *keep = false;
    }
}

fn run_action(app: &mut SlideApp, show: &mut Show, doc: &Presentation, now: f64, action: ShowAction, keep: &mut bool) {
    before_slide_change(app, show, &action);
    let stay = show.apply(action, doc, now);
    if show.rehearse && !show.record && !show.preview_only {
        if (show.ended || !stay) && !show.ask_keep {
            show.arm_prompt(now);
        }
    } else if !stay {
        *keep = false;
    }
}

fn timing_ms(secs: f64) -> u64 {
    if !secs.is_finite() {
        return 0;
    }
    (secs.max(0.0) * 1000.0).round().min(86_400_000.0) as u64
}

/// Slide clock `mm:ss.s` and the whole-show time for the keep prompt.
fn format_clock(secs: f64) -> String {
    let s = if secs.is_finite() { secs.max(0.0) } else { 0.0 };
    format!("{:02}:{:04.1}", (s / 60.0) as u32, s % 60.0)
}

fn format_total(secs: f64) -> String {
    let s = if secs.is_finite() { secs.max(0.0).round() as u32 } else { 0 };
    let h = s / 3600;
    let m = (s % 3600) / 60;
    let sec = s % 60;
    if h > 0 { format!("{h}:{m:02}:{sec:02}") } else { format!("{m}:{sec:02}") }
}

fn rehearse_bar_rect(full: Rect) -> Rect {
    Rect::from_min_size(pos2(full.min.x + 12.0, full.min.y + 12.0), vec2(420.0, 32.0))
}

fn rehearse_pause_rect(full: Rect) -> Rect {
    let r = rehearse_bar_rect(full);
    Rect::from_min_size(pos2(r.min.x + 4.0, r.min.y + 4.0), vec2(64.0, 24.0))
}

fn rehearse_repeat_rect(full: Rect) -> Rect {
    let r = rehearse_bar_rect(full);
    Rect::from_min_size(pos2(r.min.x + 220.0, r.min.y + 4.0), vec2(72.0, 24.0))
}

fn keep_panel(full: Rect) -> Rect {
    Rect::from_center_size(full.center(), vec2(460.0, 128.0))
}

fn keep_no_rect(full: Rect) -> Rect {
    let p = keep_panel(full);
    Rect::from_min_size(pos2(p.max.x - 168.0, p.max.y - 40.0), vec2(72.0, 28.0))
}

fn keep_yes_rect(full: Rect) -> Rect {
    let p = keep_panel(full);
    Rect::from_min_size(pos2(p.max.x - 88.0, p.max.y - 40.0), vec2(72.0, 28.0))
}

fn paint_laser_dot(painter: &egui::Painter, p: Pos2, col: Color32, radius: f32) {
    painter.circle_filled(p, radius, col);
    painter.circle_stroke(p, radius + 4.0, Stroke::new(2.0, Color32::from_white_alpha(200)));
}

/// The laser dot on a slide rectangle. `nx` and `ny` are 0–1 across that slide.
fn laser_on_rect(rect: Rect, nx: f32, ny: f32) -> Option<Pos2> {
    if !nx.is_finite() || !ny.is_finite() || !(0.0..=1.0).contains(&nx) || !(0.0..=1.0).contains(&ny) {
        return None;
    }
    if rect.width() <= 1.0 || rect.height() <= 1.0 {
        return None;
    }
    Some(pos2(rect.min.x + nx * rect.width(), rect.min.y + ny * rect.height()))
}

/// The rehearsal badge. Returns false once Keep or Don't keep has been chosen.
fn rehearse_bar(ui: &mut Ui, painter: &egui::Painter, ctx: &egui::Context, full: Rect, now: f64, show: &mut Show) -> bool {
    let pause = rehearse_pause_rect(full);
    let repeat = rehearse_repeat_rect(full);
    let presp = ui.interact(pause, ui.id().with("rehearse_pause"), Sense::click());
    if presp.clicked() && !show.ask_keep {
        show.clock.toggle(now);
    }
    let rresp = ui.interact(repeat, ui.id().with("rehearse_repeat"), Sense::click());
    if rresp.clicked() && !show.ask_keep {
        show.clock.repeat(now);
    }
    let r = rehearse_bar_rect(full);
    painter.rect_filled(r, CornerRadius::same(6), Color32::from_black_alpha(180));
    let pause_fill = if show.clock.paused {
        Color32::from_rgb(160, 110, 20)
    } else if presp.hovered() {
        Color32::from_rgb(80, 80, 80)
    } else {
        Color32::from_rgb(50, 50, 50)
    };
    painter.rect_filled(pause, CornerRadius::same(4), pause_fill);
    painter.text(pause.center(), Align2::CENTER_CENTER, if show.clock.paused { "Resume" } else { "Pause" }, theme::font(12.0), Color32::WHITE);
    let (slide, total) = show.rehearse_times(now);
    painter.text(
        pos2(pause.max.x + 8.0, r.center().y),
        Align2::LEFT_CENTER,
        format!("Slide {}", format_clock(slide)),
        theme::font(13.0),
        Color32::WHITE,
    );
    let repeat_fill = if rresp.hovered() { Color32::from_rgb(80, 80, 80) } else { Color32::from_rgb(50, 50, 50) };
    painter.rect_filled(repeat, CornerRadius::same(4), repeat_fill);
    painter.text(repeat.center(), Align2::CENTER_CENTER, "Repeat", theme::font(12.0), Color32::WHITE);
    painter.text(
        pos2(repeat.max.x + 8.0, r.center().y),
        Align2::LEFT_CENTER,
        format!("Total {}", format_clock(total)),
        theme::font(13.0),
        Color32::WHITE,
    );
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
    if show.ask_keep { keep_prompt(ui, painter, full, now, show) } else { true }
}

/// "Keep the new slide timings?" Returns false when Yes or No was clicked.
fn keep_prompt(ui: &mut Ui, painter: &egui::Painter, full: Rect, now: f64, show: &mut Show) -> bool {
    let panel = keep_panel(full);
    painter.rect_filled(panel, CornerRadius::same(8), Color32::from_gray(28));
    painter.rect_stroke(panel, CornerRadius::same(8), Stroke::new(1.0, Color32::from_gray(90)), egui::StrokeKind::Inside);
    let total = format_total(show.clock.total(now));
    painter.text(
        pos2(panel.min.x + 18.0, panel.min.y + 32.0),
        Align2::LEFT_CENTER,
        format!("The total time for the slide show was {total}."),
        theme::font(15.0),
        Color32::WHITE,
    );
    painter.text(pos2(panel.min.x + 18.0, panel.min.y + 58.0), Align2::LEFT_CENTER, "Keep the new slide timings?", theme::font(16.0), Color32::WHITE);
    let no = keep_no_rect(full);
    let yes = keep_yes_rect(full);
    let nresp = ui.interact(no, ui.id().with("rehearse_no"), Sense::click());
    let yresp = ui.interact(yes, ui.id().with("rehearse_yes"), Sense::click());
    painter.rect_filled(no, CornerRadius::same(4), if nresp.hovered() { Color32::from_rgb(90, 90, 90) } else { Color32::from_rgb(60, 60, 60) });
    painter.text(no.center(), Align2::CENTER_CENTER, "No", theme::font(13.0), Color32::WHITE);
    painter.rect_filled(yes, CornerRadius::same(4), if yresp.hovered() { Color32::from_rgb(40, 140, 80) } else { Color32::from_rgb(28, 110, 64) });
    painter.text(yes.center(), Align2::CENTER_CENTER, "Yes", theme::font(13.0), Color32::WHITE);
    if yresp.clicked() {
        show.keep_timings = true;
        return false;
    }
    !nresp.clicked()
}

/// Rehearsal clocks. Pause freezes the slide and the total. Repeat restarts the current slide
/// and drops that attempt from the total. Advancing stores the slide and starts the next at zero.
#[derive(Clone, Copy)]
struct RehearseClock {
    earlier: f64,
    origin: f64,
    paused: bool,
    frozen: f64,
}

impl RehearseClock {
    fn start(now: f64) -> Self {
        Self { earlier: 0.0, origin: finite_secs(now), paused: false, frozen: 0.0 }
    }

    fn slide(&self, now: f64) -> f64 {
        let t = if self.paused { self.frozen } else { finite_secs(now) - self.origin };
        if t.is_finite() { t.max(0.0) } else { 0.0 }
    }

    fn total(&self, now: f64) -> f64 {
        let t = self.earlier + self.slide(now);
        if t.is_finite() { t.max(0.0) } else { 0.0 }
    }

    fn pause(&mut self, now: f64) {
        if !self.paused {
            self.frozen = self.slide(now);
            self.paused = true;
        }
    }

    fn resume(&mut self, now: f64) {
        if self.paused {
            self.origin = finite_secs(now) - self.frozen;
            self.paused = false;
        }
    }

    fn toggle(&mut self, now: f64) {
        if self.paused { self.resume(now) } else { self.pause(now) }
    }

    fn repeat(&mut self, now: f64) {
        self.frozen = 0.0;
        self.origin = finite_secs(now);
    }

    fn advance(&mut self, now: f64) {
        self.earlier = self.total(now);
        self.frozen = 0.0;
        self.origin = finite_secs(now);
    }
}

fn finite_secs(n: f64) -> f64 {
    if n.is_finite() { n } else { 0.0 }
}

fn presenter_window(app: &mut SlideApp, ctx: &egui::Context, show: &mut Show, doc: &Presentation, now: f64, keep: &mut bool) {
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
                            leave_show(show, now, keep);
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
                if let Some((nx, ny, col)) = show.laser_dot
                    && let Some(p) = laser_on_rect(cr, nx, ny)
                {
                    let radius = (cr.width() * 0.02).clamp(5.0, 9.0);
                    paint_laser_dot(ui.painter(), p, col, radius);
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
        run_action(app, show, doc, now, a, keep);
    }
}

fn before_slide_change(app: &mut SlideApp, show: &mut Show, action: &ShowAction) {
    if show.record && matches!(action, ShowAction::Slide(_)) {
        take_narration(app, show);
    }
}

fn take_narration(app: &mut SlideApp, show: &mut Show) {
    if !show.record || !show.mic_ok {
        return;
    }
    let Some(mic) = app.services.mic.as_mut() else { return };
    let (rate, samples) = mic.take();
    if let Some(wav) = narration_wav(rate, &samples) {
        let slide = show.state.slide;
        show.narration.retain(|(i, _)| *i != slide);
        show.narration.push((slide, wav));
    }
}

/// Skip near-silence and clips shorter than 0.2 s so an idle microphone adds no shape.
fn narration_wav(rate: u32, samples: &[f32]) -> Option<Vec<u8>> {
    if !(8_000..=192_000).contains(&rate) {
        return None;
    }
    let min_n = (rate as u64).saturating_mul(200) / 1000;
    let mut sum = 0.0f64;
    let mut n = 0u64;
    for s in samples.iter().take(48_000 * 60 * 10) {
        if s.is_finite() {
            let v = *s as f64;
            sum += v * v;
            n += 1;
        }
    }
    if n < min_n.max(1) {
        return None;
    }
    let rms = (sum / n as f64).sqrt();
    if !rms.is_finite() || rms < 0.008 {
        return None;
    }
    deckcraft_media::wav_mono(rate, samples).ok()
}

fn play_on_enter(clip: &deckcraft_model::MediaClip, without_narration: bool) -> bool {
    clip.autoplay && !(clip.narration && without_narration)
}

fn saved_status(narr: bool, ink: bool, laser: bool) -> &'static str {
    match (narr, ink, laser) {
        (false, false, false) => "Recording saved: slide timings.",
        (true, false, false) => "Recording saved: slide timings and narration.",
        (false, true, false) => "Recording saved: slide timings and ink.",
        (false, false, true) => "Recording saved: slide timings and laser.",
        (true, true, false) => "Recording saved: slide timings, narration and ink.",
        (true, false, true) => "Recording saved: slide timings, narration and laser.",
        (false, true, true) => "Recording saved: slide timings, ink and laser.",
        (true, true, true) => "Recording saved: slide timings, narration, ink and laser.",
    }
}

/// The laser dot at `elapsed_ms`, held briefly after the last sample.
/// The laser dot: the button (or L while recording) leaves it on, and holding Ctrl shows it
/// in any show. A blank screen hides it. It never draws ink.
fn pointer_is_laser(sticky: bool, ctrl: bool, blank: bool) -> bool {
    !blank && (sticky || ctrl)
}

fn laser_pos(points: &[deckcraft_model::LaserSample], elapsed_ms: u32) -> Option<(f64, f64)> {
    let last = points.last()?;
    if elapsed_ms > last.t_ms.saturating_add(300) {
        return None;
    }
    let mut chosen = &points[0];
    for p in points {
        if p.t_ms <= elapsed_ms {
            chosen = p;
        } else {
            break;
        }
    }
    Some((chosen.x, chosen.y))
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

    #[test]
    fn narration_skips_silence_and_play_narrations_can_turn_it_off() {
        let quiet = vec![0.0f32; 4000];
        assert!(narration_wav(8000, &quiet).is_none());
        assert!(narration_wav(8000, &[0.2; 100]).is_none());
        let tone: Vec<f32> = (0..4000).map(|i| (i as f32 * 0.2).sin() * 0.3).collect();
        assert!(narration_wav(8000, &tone).is_some());
        let narr = deckcraft_model::MediaClip { autoplay: true, narration: true, ..Default::default() };
        assert!(play_on_enter(&narr, false));
        assert!(!play_on_enter(&narr, true));
        let clip = deckcraft_model::MediaClip { autoplay: true, ..Default::default() };
        assert!(play_on_enter(&clip, true));
    }

    #[test]
    fn ctrl_shows_the_laser_without_a_trace_and_the_button_can_leave_it_on() {
        assert!(pointer_is_laser(false, true, false));
        assert!(pointer_is_laser(true, false, false));
        assert!(!pointer_is_laser(false, false, false));
        assert!(!pointer_is_laser(true, true, true));
    }

    #[test]
    fn laser_follows_the_slide_clock_then_goes_out() {
        let pts = vec![deckcraft_model::LaserSample { x: 1.0, y: 2.0, t_ms: 0 }, deckcraft_model::LaserSample { x: 9.0, y: 8.0, t_ms: 400 }];
        assert_eq!(laser_pos(&pts, 100), Some((1.0, 2.0)));
        assert_eq!(laser_pos(&pts, 400), Some((9.0, 8.0)));
        assert_eq!(laser_pos(&pts, 700), Some((9.0, 8.0)));
        assert_eq!(laser_pos(&pts, 701), None);
        assert_eq!(saved_status(true, false, true), "Recording saved: slide timings, narration and laser.");
        assert_eq!(saved_status(false, false, false), "Recording saved: slide timings.");
    }

    #[test]
    fn rehearse_pause_freezes_and_repeat_drops_the_current_attempt() {
        let mut c = RehearseClock::start(10.0);
        assert!((c.slide(12.5) - 2.5).abs() < 1e-9);
        c.pause(12.5);
        assert!(c.paused);
        assert!((c.slide(20.0) - 2.5).abs() < 1e-9);
        assert!((c.total(20.0) - 2.5).abs() < 1e-9);
        c.resume(20.0);
        assert!((c.slide(21.0) - 3.5).abs() < 1e-9);
        c.advance(21.0);
        assert!((c.slide(22.0) - 1.0).abs() < 1e-9);
        assert!((c.total(22.0) - 4.5).abs() < 1e-9);
        c.repeat(22.0);
        assert!((c.slide(22.0) - 0.0).abs() < 1e-9);
        assert!((c.total(22.0) - 3.5).abs() < 1e-9, "repeat drops the current attempt from the total");
        assert!((c.slide(23.0) - 1.0).abs() < 1e-9);
        assert!((c.total(23.0) - 4.5).abs() < 1e-9);
        c.pause(23.0);
        c.repeat(30.0);
        assert!((c.slide(40.0) - 0.0).abs() < 1e-9);
        assert!(c.paused);
        assert_eq!(format_clock(65.2), "01:05.2");
        assert_eq!(format_total(65.4), "1:05");
        assert_eq!(format_total(3661.2), "1:01:01");
        assert_eq!(timing_ms(1.25), 1250);
    }

    #[test]
    fn the_prompt_pauses_the_clock_and_the_laser_maps_onto_the_presenter_slide() {
        let mut s = deckcraft_engine::Session::with_new();
        s.execute("slide.new", &json!({})).unwrap();
        let doc = s.active().unwrap().doc.clone();
        let mut show = Show::new(&doc, 0, false, true);
        show.rehearse = true;
        show.clock = RehearseClock::start(10.0);
        let mut keep = true;
        leave_show(&mut show, 13.0, &mut keep);
        assert!(keep, "the first leave asks instead of closing");
        assert!(show.ask_keep);
        assert!(show.rehearse_paused());
        assert!((show.rehearse_times(99.0).0 - 3.0).abs() < 1e-9);
        leave_show(&mut show, 99.0, &mut keep);
        assert!(!keep, "answering later closes, and does not keep the timings");
        assert!(!show.keep_timings);
        let cr = Rect::from_min_size(pos2(10.0, 20.0), vec2(200.0, 100.0));
        assert_eq!(laser_on_rect(cr, 0.5, 0.25), Some(pos2(110.0, 45.0)));
        assert_eq!(laser_on_rect(cr, -0.1, 0.5), None);
        assert!(
            rehearse_bar_rect(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0)))
                .contains(rehearse_pause_rect(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))).center())
        );
    }
}
