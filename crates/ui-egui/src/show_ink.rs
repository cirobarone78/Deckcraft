//! Ink annotations during the slide show: a per-slide stroke store with tools and undo.
//!
//! Pure state, no egui: `show.rs` feeds pointer positions (normalized to the slide rect) and
//! paints the strokes. Kept annotations become document ink (`slide.annotate`) when the show
//! ends, so stroke width is stored in slide units (points) and stays resolution-independent.

use std::collections::BTreeMap;

use deckcraft_color::Rgba;

/// The annotation tool active during the show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShowTool {
    /// Plain mouse: clicks advance the show.
    #[default]
    Pointer,
    Pen,
    Highlighter,
    Eraser,
    Laser,
}

/// Default pen width in slide units (points).
pub const PEN_WIDTH: f64 = 2.5;
/// Default highlighter width in slide units (points).
pub const HIGHLIGHTER_WIDTH: f64 = 14.0;
/// Highlighter ink is translucent so the slide shows through.
pub const HIGHLIGHTER_ALPHA: u8 = 110;

/// The width ladder (slide units / points) that `[` and `]` step through.
pub const WIDTHS: [f64; 8] = [0.75, 1.5, 2.5, 4.0, 6.0, 9.0, 14.0, 20.0];
/// The widths offered as quick-pick dots in the toolbar for the pen.
pub const WIDTH_DOTS: [f64; 4] = [1.5, 2.5, 4.0, 9.0];
/// The highlighter's quick-pick dots (translucent ink below ~6pt is nearly invisible).
pub const WIDTH_DOTS_HL: [f64; 4] = [6.0, 9.0, 14.0, 20.0];
/// The highlighter never goes thinner than this.
pub const HIGHLIGHTER_MIN_WIDTH: f64 = 6.0;

/// The ink color palette offered during the show (our own picks; the highlighter defaults
/// to Yellow).
pub const PALETTE: [(&str, Rgba); 8] = [
    ("Red", Rgba::rgb(0xE5, 0x39, 0x35)),
    ("Orange", Rgba::rgb(0xFB, 0x8C, 0x00)),
    ("Yellow", Rgba::rgb(0xFF, 0xE0, 0x00)),
    ("Green", Rgba::rgb(0x43, 0xA0, 0x47)),
    ("Blue", Rgba::rgb(0x1E, 0x88, 0xE5)),
    ("Purple", Rgba::rgb(0x8E, 0x24, 0xAA)),
    ("Black", Rgba::rgb(0x00, 0x00, 0x00)),
    ("White", Rgba::rgb(0xFF, 0xFF, 0xFF)),
];

/// One stroke: points normalized to the slide rect (0..=1 while over the slide; the pointer
/// may leave that range onto the letterbox bars — values are clamped when converted to
/// document ink). Width is in slide units (points).
#[derive(Clone, Debug, PartialEq)]
pub struct ShowStroke {
    /// Unique within a show (drives undo and eraser bookkeeping).
    pub id: u64,
    pub points: Vec<(f32, f32)>,
    pub color: Rgba,
    pub width: f64,
    pub highlighter: bool,
}

/// Points per stroke segment; a longer draw continues in a fresh segment (with the same
/// style) so one stroke cannot grow without bound.
const MAX_POINTS: usize = 8192;

impl ShowTool {
    /// Lowercase name (for `ui.inspect` and logs).
    pub fn name(self) -> &'static str {
        match self {
            ShowTool::Pointer => "pointer",
            ShowTool::Pen => "pen",
            ShowTool::Highlighter => "highlighter",
            ShowTool::Eraser => "eraser",
            ShowTool::Laser => "laser",
        }
    }
}

/// One undoable ink action (the journal, oldest first).
#[derive(Clone, Debug)]
enum Op {
    /// A stroke was drawn (slide, stroke id).
    Draw(usize, u64),
    /// Strokes were erased or cleared (slide, (position in the slide's stroke list, stroke),
    /// positions ascending; each restores at `min(pos, len)`).
    Erased(usize, Vec<(usize, ShowStroke)>),
}

/// Ink state for one running show. Strokes are kept per slide, so they survive navigation.
#[derive(Clone, Debug)]
pub struct ShowInk {
    tool: ShowTool,
    strokes: BTreeMap<usize, Vec<ShowStroke>>,
    journal: Vec<Op>,
    next_id: u64,
    /// The stroke still being drawn (pointer down); `push` writes only to that stroke, so
    /// undo, erasing or a slide change mid-drag cannot corrupt finished strokes.
    active: Option<(usize, u64)>,
    /// Inside one erase gesture (button held): consecutive `erase_at` calls merge into a
    /// single journal entry, so one sweep is one undo.
    erase_open: bool,
    pub pen_color: Rgba,
    pub highlighter_color: Rgba,
    pub pen_width: f64,
    pub highlighter_width: f64,
}

/// Points closer than this (normalized) merge into one, so a stationary pointer does not
/// flood the stroke.
const EPS: f32 = 0.0006;

/// Subdivisions per quadratic segment when flattening a stroke for painting/keeping.
const SMOOTH_SUBDIV: usize = 4;

/// Flatten stroke points into a smooth polyline: quadratic Béziers through the segment
/// midpoints with the input points as controls (classic pen smoothing). Endpoints are
/// preserved; 0–2 points pass through unchanged.
pub fn smooth_stroke(pts: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let Some(&first) = pts.first() else { return vec![] };
    if pts.len() <= 2 {
        return pts.to_vec();
    }
    let n = pts.len();
    let mut out = Vec::with_capacity((n - 1) * SMOOTH_SUBDIV + 1);
    out.push(first);
    let mut start = first;
    for (i, w) in pts.windows(2).enumerate() {
        let &[c, next] = w else { continue };
        // Segment end: the midpoint, except for the final leg which lands on the last point.
        let end = if i + 2 == n { next } else { ((c.0 + next.0) * 0.5, (c.1 + next.1) * 0.5) };
        for step in 1..=SMOOTH_SUBDIV {
            let t = step as f32 / SMOOTH_SUBDIV as f32;
            let u = 1.0 - t;
            out.push((u * u * start.0 + 2.0 * u * t * c.0 + t * t * end.0, u * u * start.1 + 2.0 * u * t * c.1 + t * t * end.1));
        }
        start = end;
    }
    out
}

impl ShowInk {
    pub fn new(pen_color: Rgba) -> Self {
        ShowInk {
            tool: ShowTool::Pointer,
            strokes: BTreeMap::new(),
            journal: vec![],
            next_id: 0,
            active: None,
            erase_open: false,
            pen_color,
            highlighter_color: Rgba::rgb(0xFF, 0xE0, 0x00),
            pen_width: PEN_WIDTH,
            highlighter_width: HIGHLIGHTER_WIDTH,
        }
    }

    /// The active annotation tool.
    pub fn tool(&self) -> ShowTool {
        self.tool
    }

    /// Select a tool; toggling the active pen/highlighter off returns to the pointer.
    pub fn toggle(&mut self, tool: ShowTool) {
        self.set_tool(if self.tool == tool { ShowTool::Pointer } else { tool });
    }

    /// Switch tools; any open stroke or erase gesture ends (a stroke belongs to one tool).
    pub fn set_tool(&mut self, tool: ShowTool) {
        self.end();
        self.end_erase();
        self.tool = tool;
    }

    /// Begin an erase gesture (eraser button pressed): removals until `end_erase` merge into
    /// one undo step.
    pub fn start_erase(&mut self) {
        self.erase_open = true;
    }

    /// End the erase gesture (button released, slide changed, tool switched).
    pub fn end_erase(&mut self) {
        self.erase_open = false;
    }

    pub fn strokes(&self, slide: usize) -> &[ShowStroke] {
        self.strokes.get(&slide).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The color the active pen/highlighter draws with (the pen color for other tools).
    pub fn tool_color(&self) -> Rgba {
        match self.tool {
            ShowTool::Highlighter => self.highlighter_color,
            _ => self.pen_color,
        }
    }

    /// Set the active pen/highlighter's color (a no-op for the other tools).
    pub fn set_color(&mut self, color: Rgba) {
        match self.tool {
            ShowTool::Highlighter => self.highlighter_color = color,
            ShowTool::Pen => self.pen_color = color,
            _ => {}
        }
    }

    /// Cycle the active pen/highlighter through the palette (`C` in the show); a color
    /// outside the palette (e.g. the document's pen color) jumps to the first entry.
    pub fn cycle_color(&mut self) {
        let cur = self.tool_color();
        let next = PALETTE.iter().position(|&(_, c)| c == cur).map_or(PALETTE[0].1, |i| PALETTE[(i + 1) % PALETTE.len()].1);
        self.set_color(next);
    }

    /// The width the active pen/highlighter draws with (the pen width for other tools).
    pub fn tool_width(&self) -> f64 {
        match self.tool {
            ShowTool::Highlighter => self.highlighter_width,
            _ => self.pen_width,
        }
    }

    /// Set the active pen/highlighter's width (a no-op for the other tools). The highlighter
    /// is floored at [`HIGHLIGHTER_MIN_WIDTH`]: translucent ink thinner than that is nearly
    /// invisible.
    pub fn set_width(&mut self, width: f64) {
        match self.tool {
            ShowTool::Highlighter => self.highlighter_width = width.max(HIGHLIGHTER_MIN_WIDTH),
            ShowTool::Pen => self.pen_width = width,
            _ => {}
        }
    }

    /// The toolbar's quick-pick widths for the active tool.
    pub fn width_dots(&self) -> &'static [f64; 4] {
        match self.tool {
            ShowTool::Highlighter => &WIDTH_DOTS_HL,
            _ => &WIDTH_DOTS,
        }
    }

    /// Step the active pen/highlighter along the width ladder (`[` / `]` in the show); a
    /// width outside the ladder moves to the nearest rung in that direction. Clamps at the
    /// ends.
    pub fn step_width(&mut self, up: bool) {
        let cur = self.tool_width();
        let next = if up {
            WIDTHS.iter().copied().find(|&w| w > cur + 0.001).unwrap_or(cur)
        } else {
            WIDTHS.iter().copied().rev().find(|&w| w < cur - 0.001).unwrap_or(cur)
        };
        self.set_width(next);
    }

    /// Any ink anywhere in the show (drives the keep-on-exit prompt).
    pub fn any(&self) -> bool {
        self.strokes.values().any(|v| !v.is_empty())
    }

    /// Slides with ink, in slide order: (slide index, strokes). Drives the keep-on-exit
    /// conversion (`slide.annotate`).
    pub fn by_slide(&self) -> impl Iterator<Item = (usize, &[ShowStroke])> {
        self.strokes.iter().map(|(i, v)| (*i, v.as_slice()))
    }

    /// The total number of strokes (all slides).
    pub fn count(&self) -> usize {
        self.strokes.values().map(Vec::len).sum()
    }

    /// Start a stroke with the current pen/highlighter settings. Ends any stroke still open.
    pub fn begin(&mut self, slide: usize, x: f32, y: f32) {
        let (color, width, highlighter) = match self.tool {
            ShowTool::Highlighter => (self.highlighter_color.with_alpha(HIGHLIGHTER_ALPHA), self.highlighter_width, true),
            _ => (self.pen_color, self.pen_width, false),
        };
        let id = self.next_id;
        self.next_id += 1;
        self.strokes.entry(slide).or_default().push(ShowStroke { id, points: vec![(x, y)], color, width, highlighter });
        self.journal.push(Op::Draw(slide, id));
        self.active = Some((slide, id));
    }

    /// Extend the open stroke; points that barely moved are dropped. Writes only to the open
    /// stroke: after `undo`/`end`/`clear_slide` mid-drag this is a no-op until the next
    /// `begin`.
    pub fn push(&mut self, slide: usize, x: f32, y: f32) {
        let Some((_, id)) = self.active.filter(|(s, _)| *s == slide) else { return };
        let Some(v) = self.strokes.get_mut(&slide) else {
            self.active = None;
            return;
        };
        let Some(pos) = v.iter().position(|s| s.id == id) else {
            self.active = None;
            return;
        };
        let stroke = &mut v[pos];
        if stroke.points.last().is_some_and(|&(px, py)| (px - x).abs() < EPS && (py - y).abs() < EPS) {
            return;
        }
        stroke.points.push((x, y));
        if stroke.points.len() >= MAX_POINTS {
            let (color, width, highlighter) = (stroke.color, stroke.width, stroke.highlighter);
            let new_id = self.next_id;
            self.next_id += 1;
            v.push(ShowStroke { id: new_id, points: vec![(x, y)], color, width, highlighter });
            self.journal.push(Op::Draw(slide, new_id));
            self.active = Some((slide, new_id));
        }
    }

    /// A stroke is open (pointer still down).
    pub fn open(&self) -> bool {
        self.active.is_some()
    }

    /// Close the open stroke (pointer released, slide changed, show ended).
    pub fn end(&mut self) {
        self.active = None;
    }

    /// Undo the most recent ink action — a drawn stroke is removed, an erasure or clear is
    /// restored (on any slide). Returns false when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        self.active = None;
        while let Some(op) = self.journal.pop() {
            match op {
                Op::Draw(slide, id) => {
                    if let Some(v) = self.strokes.get_mut(&slide)
                        && let Some(pos) = v.iter().position(|s| s.id == id)
                    {
                        v.remove(pos);
                        if v.is_empty() {
                            self.strokes.remove(&slide);
                        }
                        return true;
                    }
                }
                Op::Erased(slide, strokes) => {
                    if strokes.is_empty() {
                        continue;
                    }
                    let v = self.strokes.entry(slide).or_default();
                    for (pos, s) in strokes {
                        v.insert(pos.min(v.len()), s);
                    }
                    return true;
                }
            }
        }
        false
    }

    /// Erase whole strokes near a point given in normalized slide coordinates. `tol` is the
    /// pointer tolerance in slide units (points); a stroke counts as hit when the point comes
    /// within `tol + width/2` of its polyline. The hit test runs on the smoothed curve in
    /// slide coordinates, so it is isotropic and matches the painted geometry. Erased strokes
    /// are undoable. Returns how many strokes were erased.
    pub fn erase_at(&mut self, slide: usize, x: f32, y: f32, tol: f32, slide_w: f64, slide_h: f64) -> usize {
        let Some(v) = self.strokes.get_mut(&slide) else { return 0 };
        let (sw, sh) = (slide_w.max(1.0), slide_h.max(1.0));
        let (px, py) = (f64::from(x) * sw, f64::from(y) * sh);
        let hit = |s: &ShowStroke| {
            let t = f64::from(tol) + s.width / 2.0;
            match s.points.as_slice() {
                [] => false,
                [(sx, sy)] => ((f64::from(*sx) * sw - px).powi(2) + (f64::from(*sy) * sh - py).powi(2)).sqrt() <= t,
                _ => {
                    let sm = smooth_stroke(&s.points);
                    sm.windows(2).any(|w| {
                        seg_dist(px, py, f64::from(w[0].0) * sw, f64::from(w[0].1) * sh, f64::from(w[1].0) * sw, f64::from(w[1].1) * sh) <= t
                    })
                }
            }
        };
        let mut removed: Vec<(usize, ShowStroke)> = vec![];
        let mut i = 0;
        v.retain(|s| {
            let keep = !hit(s);
            if !keep {
                removed.push((i, s.clone()));
            }
            i += 1;
            keep
        });
        if removed.is_empty() {
            return 0;
        }
        if v.is_empty() {
            self.strokes.remove(&slide);
        }
        if self.active.is_some_and(|(_, id)| removed.iter().any(|(_, s)| s.id == id)) {
            self.active = None;
        }
        let n = removed.len();
        // One gesture = one undo step: merge into the open gesture's journal entry,
        // converting the just-removed positions (against the thinned list) back to positions
        // in the list as it was when the gesture started.
        if self.erase_open
            && let Some(Op::Erased(s, prior)) = self.journal.last_mut()
            && *s == slide
        {
            // Convert against the gesture's earlier removals only; strokes removed in this
            // same call share the pre-call list and must not shift each other.
            let prior_positions: Vec<usize> = prior.iter().map(|(p, _)| *p).collect();
            for (pos, stroke) in removed {
                let mut orig = pos;
                loop {
                    let shifted = pos + prior_positions.iter().filter(|rp| **rp <= orig).count();
                    if shifted == orig {
                        break;
                    }
                    orig = shifted;
                }
                prior.push((orig, stroke));
            }
            prior.sort_by_key(|(p, _)| *p);
            return n;
        }
        self.journal.push(Op::Erased(slide, removed));
        n
    }

    /// Clear one slide's ink (`E` in the show). Undoable.
    pub fn clear_slide(&mut self, slide: usize) {
        if self.active.is_some_and(|(s, _)| s == slide) {
            self.active = None;
        }
        if let Some(v) = self.strokes.remove(&slide)
            && !v.is_empty()
        {
            self.journal.push(Op::Erased(slide, v.into_iter().enumerate().collect()));
        }
    }
}

/// Distance from point (px,py) to the segment (ax,ay)-(bx,by).
fn seg_dist(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let (cx, cy) = (ax + t * dx, ay + t * dy);
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ink() -> ShowInk {
        ShowInk::new(Rgba::rgb(0xFF, 0x00, 0x00))
    }

    #[test]
    fn strokes_are_kept_per_slide() {
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen);
        ink.begin(0, 0.1, 0.1);
        ink.push(0, 0.2, 0.2);
        ink.begin(2, 0.5, 0.5);
        assert_eq!(ink.strokes(0).len(), 1);
        assert_eq!(ink.strokes(1).len(), 0);
        assert_eq!(ink.strokes(2).len(), 1);
        assert_eq!(ink.count(), 2);
        assert!(ink.any());
    }

    #[test]
    fn stroke_uses_current_tool_settings() {
        let mut ink = ink();
        ink.pen_color = Rgba::rgb(0x00, 0x00, 0xFF);
        ink.pen_width = 4.0;
        ink.set_tool(ShowTool::Pen);
        ink.begin(0, 0.1, 0.1);
        assert_eq!(ink.strokes(0)[0].color, Rgba::rgb(0x00, 0x00, 0xFF));
        assert_eq!(ink.strokes(0)[0].width, 4.0);
        assert!(!ink.strokes(0)[0].highlighter);
        ink.set_tool(ShowTool::Highlighter);
        ink.begin(0, 0.2, 0.2);
        let hl = &ink.strokes(0)[1];
        assert!(hl.highlighter);
        assert_eq!(hl.width, HIGHLIGHTER_WIDTH);
        assert_eq!(hl.color.a, HIGHLIGHTER_ALPHA);
    }

    #[test]
    fn toggle_selects_and_deselects() {
        let mut ink = ink();
        ink.toggle(ShowTool::Pen);
        assert_eq!(ink.tool(), ShowTool::Pen);
        ink.toggle(ShowTool::Highlighter);
        assert_eq!(ink.tool(), ShowTool::Highlighter);
        ink.toggle(ShowTool::Highlighter);
        assert_eq!(ink.tool(), ShowTool::Pointer);
    }

    #[test]
    fn push_merges_stationary_points() {
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen);
        ink.begin(0, 0.5, 0.5);
        ink.push(0, 0.5 + EPS / 2.0, 0.5);
        assert_eq!(ink.strokes(0)[0].points.len(), 1);
        ink.push(0, 0.6, 0.6);
        assert_eq!(ink.strokes(0)[0].points.len(), 2);
    }

    #[test]
    fn push_without_begin_is_a_no_op() {
        let mut ink = ink();
        ink.push(0, 0.5, 0.5);
        assert_eq!(ink.count(), 0);
    }

    #[test]
    fn undo_removes_the_latest_stroke_across_slides() {
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen);
        ink.begin(0, 0.1, 0.1);
        ink.begin(1, 0.2, 0.2);
        assert!(ink.undo());
        assert_eq!(ink.strokes(0).len(), 1);
        assert_eq!(ink.strokes(1).len(), 0);
        assert!(ink.undo());
        assert!(!ink.any());
        assert!(!ink.undo());
    }

    #[test]
    fn clear_slide_is_undoable_and_drops_only_that_slide() {
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen);
        ink.begin(0, 0.1, 0.1);
        ink.begin(1, 0.2, 0.2);
        ink.begin(0, 0.3, 0.3);
        ink.clear_slide(0);
        assert_eq!(ink.strokes(0).len(), 0);
        assert_eq!(ink.count(), 1);
        // Undo restores the cleared strokes in their original order.
        assert!(ink.undo());
        assert_eq!(ink.strokes(0).len(), 2);
        assert_eq!(ink.strokes(0)[0].points[0], (0.1, 0.1));
        assert_eq!(ink.strokes(0)[1].points[0], (0.3, 0.3));
        // Further undos walk back the draws.
        assert!(ink.undo());
        assert_eq!(ink.strokes(0).len(), 1);
        assert!(ink.undo());
        assert!(ink.strokes(1).is_empty()); // slide 1's stroke undone
        assert!(ink.undo());
        assert!(!ink.any());
        assert!(!ink.undo());
    }

    #[test]
    fn undo_mid_drag_does_not_corrupt_the_previous_stroke() {
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen);
        ink.begin(0, 0.1, 0.1);
        ink.push(0, 0.15, 0.15);
        ink.end();
        ink.begin(0, 0.5, 0.5);
        assert!(ink.undo());
        // Still holding the pointer: pushes must not extend the finished stroke.
        ink.push(0, 0.9, 0.9);
        assert_eq!(ink.strokes(0)[0].points, vec![(0.1, 0.1), (0.15, 0.15)]);
        assert!(!ink.open());
    }

    #[test]
    fn slide_change_mid_drag_ends_the_stroke() {
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen);
        ink.begin(0, 0.1, 0.1);
        ink.push(0, 0.2, 0.2);
        ink.end(); // the show moved to another slide
        ink.push(1, 0.5, 0.5);
        ink.push(0, 0.9, 0.9);
        assert_eq!(ink.strokes(0)[0].points.len(), 2);
        assert_eq!(ink.strokes(1).len(), 0);
    }

    #[test]
    fn clear_slide_mid_drag_stops_the_open_stroke() {
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen);
        ink.begin(0, 0.1, 0.1);
        ink.clear_slide(0);
        ink.push(0, 0.5, 0.5);
        assert!(!ink.any());
    }

    #[test]
    fn long_draws_continue_in_bounded_segments() {
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen);
        ink.begin(0, 0.0, 0.0);
        for i in 1..=MAX_POINTS * 2 {
            ink.push(0, (i as f32 * 0.001).fract(), 0.5);
        }
        let strokes = ink.strokes(0);
        assert!(strokes.len() >= 2);
        assert!(strokes.iter().all(|s| s.points.len() <= MAX_POINTS));
        // Segments chain: each starts where the previous ended.
        for w in strokes.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            assert_eq!(a.points.last(), b.points.first());
            assert_eq!((a.color, a.width, a.highlighter), (b.color, b.width, b.highlighter));
        }
    }

    #[test]
    fn palette_cycle_wraps_and_remembers_per_tool() {
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen);
        ink.pen_color = PALETTE[0].1;
        ink.cycle_color();
        assert_eq!(ink.pen_color, PALETTE[1].1);
        // The highlighter keeps its own color.
        assert_eq!(ink.highlighter_color, PALETTE[2].1);
        ink.set_tool(ShowTool::Highlighter);
        ink.cycle_color();
        assert_eq!(ink.highlighter_color, PALETTE[3].1);
        assert_eq!(ink.pen_color, PALETTE[1].1);
        // Wrap around the end of the palette.
        ink.pen_color = PALETTE[7].1;
        ink.set_tool(ShowTool::Pen);
        ink.cycle_color();
        assert_eq!(ink.pen_color, PALETTE[0].1);
    }

    #[test]
    fn palette_cycle_from_a_custom_color_starts_at_the_top() {
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen);
        ink.pen_color = Rgba::rgb(0x12, 0x34, 0x56);
        ink.cycle_color();
        assert_eq!(ink.pen_color, PALETTE[0].1);
    }

    #[test]
    fn set_color_only_touches_pen_and_highlighter() {
        let mut ink = ink();
        let before = ink.pen_color;
        ink.set_tool(ShowTool::Pointer);
        ink.set_color(PALETTE[4].1);
        assert_eq!(ink.pen_color, before);
        ink.set_tool(ShowTool::Pen);
        ink.set_color(PALETTE[4].1);
        assert_eq!(ink.tool_color(), PALETTE[4].1);
    }

    #[test]
    fn cycle_color_on_the_pointer_is_a_no_op() {
        let mut ink = ink();
        let before = ink.pen_color;
        ink.cycle_color();
        assert_eq!(ink.pen_color, before);
    }

    #[test]
    fn palette_colors_are_unique() {
        // Position-based cycling breaks if two entries share a color.
        for (i, a) in PALETTE.iter().enumerate() {
            for b in &PALETTE[i + 1..] {
                assert_ne!(a.1, b.1, "duplicate palette colors {} and {}", a.0, b.0);
            }
        }
    }

    #[test]
    fn mid_stroke_color_change_keeps_the_open_strokes_color() {
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen);
        ink.begin(0, 0.1, 0.1);
        ink.cycle_color();
        ink.push(0, 0.2, 0.2);
        // begin() snapshotted the initial color; cycling must not recolor the open stroke.
        assert_eq!(ink.strokes(0)[0].color, Rgba::rgb(0xFF, 0x00, 0x00));
        assert_ne!(ink.strokes(0)[0].color, ink.pen_color);
    }

    #[test]
    fn tool_color_for_the_highlighter_is_the_opaque_stored_color() {
        // The swatch ring compares `tool_color()` against opaque palette entries; returning
        // the alpha-applied stroke color would silently deselect it.
        let mut ink = ink();
        ink.set_tool(ShowTool::Highlighter);
        assert_eq!(ink.tool_color(), ink.highlighter_color);
        assert_eq!(ink.tool_color().a, 255);
    }

    #[test]
    fn width_ladder_is_sorted_and_matches_the_defaults() {
        assert!(WIDTHS.windows(2).all(|w| w[0] < w[1]));
        assert!(WIDTHS.contains(&PEN_WIDTH));
        assert!(WIDTHS.contains(&HIGHLIGHTER_WIDTH));
        assert!(WIDTH_DOTS.iter().all(|d| WIDTHS.contains(d)));
        assert!(WIDTH_DOTS_HL.iter().all(|d| WIDTHS.contains(d)));
        assert!(WIDTH_DOTS_HL.iter().all(|d| *d >= HIGHLIGHTER_MIN_WIDTH));
    }

    #[test]
    fn highlighter_width_has_a_floor() {
        let mut ink = ink();
        ink.set_tool(ShowTool::Highlighter);
        ink.set_width(1.5);
        assert_eq!(ink.highlighter_width, HIGHLIGHTER_MIN_WIDTH);
        // Stepping down from the floor stays at the floor.
        ink.highlighter_width = HIGHLIGHTER_MIN_WIDTH;
        ink.step_width(false);
        assert_eq!(ink.highlighter_width, HIGHLIGHTER_MIN_WIDTH);
        assert_eq!(ink.width_dots(), &WIDTH_DOTS_HL);
        ink.set_tool(ShowTool::Pen);
        assert_eq!(ink.width_dots(), &WIDTH_DOTS);
    }

    #[test]
    fn mid_stroke_width_change_keeps_the_open_strokes_width() {
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen);
        ink.begin(0, 0.1, 0.1);
        ink.step_width(true);
        ink.push(0, 0.2, 0.2);
        assert_eq!(ink.strokes(0)[0].width, PEN_WIDTH);
        assert_ne!(ink.strokes(0)[0].width, ink.pen_width);
    }

    #[test]
    fn eraser_removes_hit_strokes_and_erase_is_undoable() {
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen);
        // A: left line, B: right line, C: bottom line.
        ink.begin(0, 0.1, 0.5);
        ink.push(0, 0.2, 0.5);
        ink.end();
        ink.begin(0, 0.6, 0.5);
        ink.push(0, 0.7, 0.5);
        ink.end();
        ink.begin(0, 0.1, 0.9);
        ink.push(0, 0.2, 0.9);
        ink.end();
        // Erase B by its middle.
        assert_eq!(ink.erase_at(0, 0.65, 0.5, 7.2, 720.0, 405.0), 1);
        assert_eq!(ink.strokes(0).len(), 2);
        // Undo restores B at its original position.
        assert!(ink.undo());
        assert_eq!(ink.strokes(0).len(), 3);
        assert_eq!(ink.strokes(0)[1].points[0], (0.6, 0.5));
        // Then undos remove the draws, newest first.
        assert!(ink.undo());
        assert_eq!(ink.strokes(0).len(), 2);
        assert!(ink.undo());
        assert!(ink.undo());
        assert!(!ink.any());
    }

    #[test]
    fn eraser_misses_distant_points_and_hits_dots() {
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen);
        ink.begin(0, 0.5, 0.5);
        ink.end();
        assert_eq!(ink.erase_at(0, 0.9, 0.9, 7.2, 720.0, 405.0), 0);
        assert_eq!(ink.erase_at(0, 0.5, 0.505, 7.2, 720.0, 405.0), 1);
        assert!(!ink.any());
    }

    #[test]
    fn eraser_respects_stroke_width() {
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen);
        ink.begin(0, 0.1, 0.5);
        ink.push(0, 0.2, 0.5);
        ink.end();
        // Default pen (2.5pt): 8.1pt below the line is a miss (tol 3.6 + 1.25 half-width).
        assert_eq!(ink.erase_at(0, 0.15, 0.52, 3.6, 720.0, 405.0), 0);
        // Thick pen: the same offset hits (3.6 + 10).
        ink.pen_width = 20.0;
        ink.begin(0, 0.1, 0.7);
        ink.push(0, 0.2, 0.7);
        ink.end();
        assert_eq!(ink.erase_at(0, 0.15, 0.72, 3.6, 720.0, 405.0), 1);
        assert_eq!(ink.strokes(0).len(), 1);
    }

    #[test]
    fn eraser_tolerance_is_isotropic() {
        // On a 720x405 slide the same 5pt offset must hit vertically and horizontally alike
        // (a normalized-coordinates hit test would be an ellipse).
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen);
        ink.begin(0, 0.1, 0.5);
        ink.push(0, 0.9, 0.5);
        ink.end();
        assert_eq!(ink.erase_at(0, 0.5, 0.5 + 5.0 / 405.0, 5.0, 720.0, 405.0), 1);
        ink.begin(0, 0.1, 0.5);
        ink.push(0, 0.9, 0.5);
        ink.end();
        assert_eq!(ink.erase_at(0, 0.9 + 5.0 / 720.0, 0.5, 5.0, 720.0, 405.0), 1);
    }

    #[test]
    fn eraser_ignores_other_slides() {
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen);
        ink.begin(1, 0.5, 0.5);
        ink.end();
        assert_eq!(ink.erase_at(0, 0.5, 0.5, 7.2, 720.0, 405.0), 0);
        assert!(ink.any());
    }

    #[test]
    fn switching_tools_closes_the_open_stroke() {
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen);
        ink.begin(0, 0.1, 0.1);
        ink.set_tool(ShowTool::Eraser);
        assert!(!ink.open());
        ink.push(0, 0.9, 0.9);
        assert_eq!(ink.strokes(0)[0].points.len(), 1);
    }

    #[test]
    fn eraser_tests_the_smoothed_curve() {
        // A sharp-corner stroke (right angle at (0.1, 0.9)): the painted curve cuts the
        // corner, so the eraser must hit the visible bulge and miss the raw corner point.
        let draw = |ink: &mut ShowInk| {
            ink.begin(0, 0.1, 0.5);
            ink.push(0, 0.1, 0.9);
            ink.push(0, 0.6, 0.9);
            ink.end();
        };
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen);
        draw(&mut ink);
        // On the smoothed bulge (the midpoint quadratic's closest approach to the corner:
        // (72+360t², ...) at t=0.5 → (162, 344.25) in slide points).
        assert_eq!(ink.erase_at(0, 0.225, 0.85, 5.0, 720.0, 405.0), 1);
        draw(&mut ink);
        // The raw corner point itself is off-curve now.
        assert_eq!(ink.erase_at(0, 0.1, 0.9, 5.0, 720.0, 405.0), 0);
    }

    #[test]
    fn journal_interleaves_draws_and_erasures_in_reverse_order() {
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen);
        ink.begin(0, 0.1, 0.1);
        ink.end();
        assert_eq!(ink.erase_at(0, 0.1, 0.1, 7.2, 720.0, 405.0), 1);
        ink.begin(0, 0.5, 0.5);
        ink.end();
        assert_eq!(ink.strokes(0).len(), 1);
        assert!(ink.undo()); // removes the second draw
        assert_eq!(ink.strokes(0).len(), 0);
        assert!(ink.undo()); // restores the erased stroke
        assert_eq!(ink.strokes(0).len(), 1);
        assert_eq!(ink.strokes(0)[0].points[0], (0.1, 0.1));
        assert!(ink.undo()); // removes the first draw
        assert!(!ink.any());
        assert!(!ink.undo());
    }

    #[test]
    fn eraser_sweep_is_one_undo_and_restores_positions() {
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen);
        // Three horizontal lines at y = 0.2, 0.5, 0.8.
        for y in [0.2f32, 0.5, 0.8] {
            ink.begin(0, 0.1, y);
            ink.push(0, 0.9, y);
            ink.end();
        }
        ink.start_erase();
        // Frame 1 of the sweep erases the top line, frame 2 the bottom line.
        assert_eq!(ink.erase_at(0, 0.5, 0.2, 7.2, 720.0, 405.0), 1);
        assert_eq!(ink.erase_at(0, 0.5, 0.8, 7.2, 720.0, 405.0), 1);
        ink.end_erase();
        assert_eq!(ink.strokes(0).len(), 1);
        // A single undo restores both, in their original positions.
        assert!(ink.undo());
        assert_eq!(ink.strokes(0).len(), 3);
        assert_eq!(ink.strokes(0)[0].points[0], (0.1, 0.2));
        assert_eq!(ink.strokes(0)[1].points[0], (0.1, 0.5));
        assert_eq!(ink.strokes(0)[2].points[0], (0.1, 0.8));
    }

    #[test]
    fn eraser_without_a_gesture_journals_per_call() {
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen);
        for y in [0.2f32, 0.5] {
            ink.begin(0, 0.1, y);
            ink.push(0, 0.9, y);
            ink.end();
        }
        // No start_erase: each call is its own undo step.
        assert_eq!(ink.erase_at(0, 0.5, 0.2, 7.2, 720.0, 405.0), 1);
        assert_eq!(ink.erase_at(0, 0.5, 0.5, 7.2, 720.0, 405.0), 1);
        assert!(!ink.any());
        assert!(ink.undo());
        assert_eq!(ink.strokes(0).len(), 1);
        assert_eq!(ink.strokes(0)[0].points[0], (0.1, 0.5));
        assert!(ink.undo());
        assert_eq!(ink.strokes(0).len(), 2);
    }

    #[test]
    fn smooth_stroke_passthrough_and_bounded() {
        assert_eq!(smooth_stroke(&[]), vec![]);
        assert_eq!(smooth_stroke(&[(0.1, 0.2)]), vec![(0.1, 0.2)]);
        assert_eq!(smooth_stroke(&[(0.0, 0.0), (1.0, 1.0)]), vec![(0.0, 0.0), (1.0, 1.0)]);
        let pts: Vec<(f32, f32)> = (0..10).map(|i| (i as f32, (i % 2) as f32)).collect();
        let out = smooth_stroke(&pts);
        assert_eq!(out.len(), (pts.len() - 1) * SMOOTH_SUBDIV + 1);
    }

    #[test]
    fn smooth_stroke_preserves_endpoints() {
        let pts = [(0.1, 0.2), (0.5, 0.8), (0.9, 0.3)];
        let out = smooth_stroke(&pts);
        assert_eq!(out.first(), pts.first().copied().as_ref());
        assert_eq!(out.last(), pts.last().copied().as_ref());
    }

    #[test]
    fn smooth_stroke_keeps_collinear_points_on_the_line() {
        let pts = [(0.0, 0.5), (0.3, 0.5), (0.6, 0.5), (1.0, 0.5)];
        assert!(smooth_stroke(&pts).iter().all(|&(_, y)| (y - 0.5).abs() < 1e-6));
    }

    #[test]
    fn smooth_stroke_cuts_corners() {
        // A right-angle corner at (0, 1): the smoothed path must pass strictly inside it.
        let pts = [(0.0, 0.0), (0.0, 1.0), (1.0, 1.0)];
        let out = smooth_stroke(&pts);
        let corner_dist = |p: &(f32, f32)| (p.0 * p.0 + (p.1 - 1.0) * (p.1 - 1.0)).sqrt();
        let min_d = out.iter().map(corner_dist).fold(f32::INFINITY, f32::min);
        assert!(min_d > 0.01, "smoothed path still hits the corner");
        assert!(min_d < 0.5, "smoothing swallowed the corner entirely");
    }

    #[test]
    fn step_width_walks_the_ladder_and_clamps() {
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen); // default 2.5
        ink.step_width(true);
        assert_eq!(ink.pen_width, 4.0);
        ink.step_width(true);
        assert_eq!(ink.pen_width, 6.0);
        ink.step_width(false);
        assert_eq!(ink.pen_width, 4.0);
        ink.pen_width = WIDTHS[0];
        ink.step_width(false);
        assert_eq!(ink.pen_width, WIDTHS[0]);
        ink.pen_width = WIDTHS[7];
        ink.step_width(true);
        assert_eq!(ink.pen_width, WIDTHS[7]);
    }

    #[test]
    fn step_width_from_a_custom_width_finds_the_nearest_rung() {
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen);
        ink.pen_width = 3.0; // between 2.5 and 4.0
        ink.step_width(true);
        assert_eq!(ink.pen_width, 4.0);
        ink.pen_width = 3.0;
        ink.step_width(false);
        assert_eq!(ink.pen_width, 2.5);
    }

    #[test]
    fn width_is_remembered_per_tool_and_gated() {
        let mut ink = ink();
        ink.set_tool(ShowTool::Pen);
        ink.set_width(6.0);
        ink.set_tool(ShowTool::Highlighter);
        assert_eq!(ink.tool_width(), HIGHLIGHTER_WIDTH);
        ink.set_width(20.0);
        assert_eq!(ink.highlighter_width, 20.0);
        assert_eq!(ink.pen_width, 6.0);
        ink.set_tool(ShowTool::Pointer);
        ink.step_width(true);
        assert_eq!(ink.pen_width, 6.0); // unchanged
    }
}
