//! The equation editor: equaSHion's [`EquationEditor`] widget, bare, over the equation where it
//! sits on the slide.
//!
//! It opens from Insert › Equation (a new equation, in the middle of the slide) and from a
//! double-click on an equation (the engine asks for the `equation` dialog with
//! `{edit: true, id, cell?, paragraph, run}`). The dialog is only the open request: nothing is
//! drawn in a window. The Equation ribbon tab (see [`crate::equation_tab`]) does the choosing and
//! inserts into the widget. The widget edits a copy, loaded from the run's OMML through
//! `equation.get`; the presentation changes once, through the engine's own commands, so undo and
//! save work as for any other edit: `insert.equation` (a new text box) or `equation.update`
//! (that run), one undo step. An empty equation is not applied.
//!
//! The widget shares the app's `egui::Context` and never touches its fonts, visuals or style: it
//! draws the equation with its own bundled fonts.

use equashion_egui::EquationEditor;
use serde_json::{Value, json};

use crate::SlideApp;
use crate::dialogs::Dialog;
use crate::theme::Tokens;

/// Parameters that say which equation run an edit targets.
const TARGET_KEYS: [&str; 4] = ["id", "cell", "paragraph", "run"];

/// The editor while the Equation dialog is open.
pub(crate) struct Open {
    widget: EquationEditor,
    /// The dialog parameters it was opened with (a new request reloads).
    params: Value,
    /// Where the edited equation lives (`id`, `cell`, `paragraph`, `run`); `None` inserts a new one.
    target: Option<Value>,
    /// Why loading failed.
    error: Option<String>,
    /// Where the equation is drawn on the slide, `[x, y, w, h]` in slide points (a new one has none).
    rect: Option<[f64; 4]>,
    /// The font size is set for this slide zoom (and, for an existing equation, matched to the
    /// size it is drawn at).
    sized: bool,
    calibrated: bool,
    /// The font size now, in screen points.
    pt: f64,
}

/// The equation editor's state in [`SlideApp`].
#[derive(Default)]
pub struct EquationHost {
    pub(crate) open: Option<Open>,
    /// The ribbon has already switched to the Equation tab for this editor.
    pub(crate) tab_shown: bool,
    /// The ribbon's Close Equation asked for the edit to be applied, as Insert / Update does.
    pub(crate) commit_requested: bool,
    /// Draws the faces in Insert › Equation's menu of ready-made equations (no editor is open then).
    pub(crate) preview: Option<EquationEditor>,
}

impl EquationHost {
    /// The Equation dialog's editor is open.
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// The open editor's widget.
    pub fn widget(&self) -> Option<&EquationEditor> {
        self.open.as_ref().map(|o| &o.widget)
    }

    /// The open editor's widget, mutable.
    pub fn widget_mut(&mut self) -> Option<&mut EquationEditor> {
        self.open.as_mut().map(|o| &mut o.widget)
    }

    /// The equation being edited (`id`, `cell`, `paragraph`, `run`), or `None` for a new one.
    pub fn target(&self) -> Option<&Value> {
        self.open.as_ref().and_then(|o| o.target.as_ref())
    }
}

/// Open the editor for the dialog's parameters: blank, or on the equation `{edit: true, ...}`
/// names, read with `equation.get` (a query: the presentation and its history stay as they are).
fn open(app: &mut SlideApp, params: &Value) -> Open {
    let mut o = Open {
        widget: EquationEditor::new().inline(true).id_salt("deckcraft-equation-editor"),
        params: params.clone(),
        target: None,
        error: None,
        rect: None,
        sized: false,
        calibrated: false,
        pt: 20.0,
    };
    // A ready-made equation from Insert › Equation's arrow: a new equation, started from it.
    if let Some(id) = params.get("builtin").and_then(Value::as_str) {
        if let Some(b) = equashion_egui::equashion_math::galleries().builtins.iter().find(|b| b.id == id) {
            o.widget.set_math(b.math.clone());
        }
        return o;
    }
    if params.get("edit").and_then(Value::as_bool) != Some(true) {
        return o;
    }
    let mut loc = serde_json::Map::new();
    for k in TARGET_KEYS {
        if let Some(v) = params.get(k).filter(|v| !v.is_null()) {
            loc.insert(k.into(), v.clone());
        }
    }
    match app.session.execute("equation.get", &Value::Object(loc)) {
        Ok(v) => {
            o.rect = v.get("rect").and_then(Value::as_array).and_then(|a| {
                let n: Vec<f64> = a.iter().filter_map(Value::as_f64).collect();
                <[f64; 4]>::try_from(n).ok()
            });
            if let Some(x) = v.get("omml").and_then(Value::as_str) {
                o.widget.set_omml(x);
                // Update the very run that was read, even if the selection moves meanwhile.
                let mut t = serde_json::Map::new();
                for k in TARGET_KEYS {
                    if let Some(x) = v.get(k).filter(|x| !x.is_null()) {
                        t.insert(k.into(), x.clone());
                    }
                }
                o.target = Some(Value::Object(t));
            }
        }
        Err(e) => o.error = Some(e.to_string()),
    }
    o
}

/// Apply the widget's equation: `insert.equation` or `equation.update`, one undo step.
fn apply(app: &mut SlideApp, o: &mut Open) -> Result<(), String> {
    o.widget.editor_mut().flush();
    if o.widget.editor().is_blank() {
        return Err("Type or build an equation first.".into());
    }
    let mut p = json!({"omml": o.widget.omml()});
    let cmd = match (&o.target, p.as_object_mut()) {
        (Some(Value::Object(t)), Some(obj)) => {
            obj.extend(t.clone());
            "equation.update"
        }
        _ => "insert.equation",
    };
    app.run(cmd, p).map(|_| ())
}

/// The equation, edited where it sits on the slide: the widget's bare canvas over the equation
/// (or in the middle of the slide for a new one), sized to match, with the Equation tab doing the
/// choosing. True when the editor should close.
///
/// - Enter, Esc, a click on the slide outside the equation, or the tab's Close Equation leave
///   it, keeping the edit (one undo step); a new equation never filled is dropped.
/// - Clicks in the ribbon and its menus do not leave it.
pub fn in_place(app: &mut SlideApp, ctx: &egui::Context, d: &mut Dialog) -> bool {
    let mut o = match app.equation.open.take() {
        Some(o) if o.params == d.params => o,
        _ => open(app, &d.params),
    };
    if let Some(e) = o.error.take() {
        app.set_status(e);
        return true;
    }
    let Some(slide) = app.slide_rect else {
        app.equation.open = Some(o);
        return false;
    };
    let scale = app.canvas_scale.max(0.05);
    let pad = EquationEditor::inline_pad();
    let at = |x: f64, y: f64| egui::pos2(slide.min.x + x as f32 * scale, slide.min.y + y as f32 * scale);
    let (anchor, target_h) = match o.rect {
        Some([x, y, w, h]) => {
            let (tl, br) = (at(x, y), at(x + w, y + h));
            (egui::pos2(tl.x - pad.x, (tl.y + br.y) / 2.0), Some(br.y - tl.y))
        }
        None => (egui::pos2(slide.center().x - 70.0, slide.center().y), None),
    };
    // Size: a new equation at a body-text size; an existing one calibrated once so that what is
    // typed is the size of what was there.
    if !o.sized {
        o.pt = f64::from(if target_h.is_some() { 18.0 } else { 24.0 } * scale);
        o.widget.set_font_size(o.pt);
        o.sized = true;
    } else if !o.calibrated {
        if let (Some(h), Some(cs)) = (target_h, o.widget.content_size()) {
            let natural = f64::from((cs.y - 2.0 * pad.y).max(1.0));
            o.pt = (o.pt * f64::from(h) / natural).clamp(6.0, 400.0);
            o.widget.set_font_size(o.pt);
        }
        o.calibrated = true;
    }

    let t = Tokens::get(ctx);
    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let mut response = None;
    let area = egui::Area::new(egui::Id::new("deckcraft-equation-in-place"))
        .order(egui::Order::Foreground)
        .fixed_pos(anchor)
        .pivot(egui::Align2::LEFT_CENTER)
        .show(ctx, |ui| {
            egui::Frame::new().fill(egui::Color32::from_rgba_unmultiplied(255, 255, 255, 255)).corner_radius(3).show(ui, |ui| {
                // The widget draws its ink in the "strong" colour on this near-white backing.
                ui.visuals_mut().widgets.active.fg_stroke.color = egui::Color32::BLACK;
                ui.visuals_mut().selection.stroke.color = t.accent;
                response = Some(o.widget.show(ui));
            });
        });
    let outside = ctx.input(|i| i.pointer.primary_pressed())
        && ctx.pointer_interact_pos().is_some_and(|p| {
            !area.response.rect.contains(p)
                && app.canvas_rect.is_some_and(|c| c.contains(p))
                // The slide itself, not a menu or window above it.
                && ctx.layer_id_at(p).is_some_and(|l| l.order == egui::Order::Background)
        });
    let from_ribbon = std::mem::take(&mut app.equation.commit_requested);
    let leave = esc || outside || from_ribbon || response.as_ref().is_some_and(|r| r.committed || r.cancelled);
    if !leave {
        app.equation.open = Some(o);
        return false;
    }
    o.widget.editor_mut().flush();
    // Nothing typed, or nothing changed: nothing to apply.
    let nothing = o.widget.editor().is_blank() || (o.target.is_some() && !o.widget.is_dirty());
    if !nothing && let Err(e) = apply(app, &mut o) {
        app.set_status(e);
    }
    true
}

#[cfg(test)]
mod tests {
    use deckcraft_engine::Session;
    use deckcraft_model::text::RunKind;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    use egui::vec2;

    use super::*;
    use crate::Services;

    fn harness() -> Harness<'static, SlideApp> {
        let mut h = Harness::builder().with_size(vec2(1280.0, 860.0)).build_ui_state(
            |ui, app: &mut SlideApp| {
                app.logic(ui.ctx());
                app.ui(ui);
            },
            SlideApp::new(Session::with_new(), Services::default()),
        );
        // The app keeps requesting repaints, so step frames instead of run().
        h.run_steps(3);
        h
    }

    fn steps(h: &mut Harness<'static, SlideApp>, n: usize) {
        h.run_steps(n);
    }

    fn type_text(h: &mut Harness<'static, SlideApp>, t: &str) {
        h.event(egui::Event::Text(t.into()));
        steps(h, 2);
    }

    /// (linear text, OMML) of every equation run on the current slide.
    fn equations(app: &SlideApp) -> Vec<(String, String)> {
        let Some(sl) = app.session.active().and_then(|d| d.current_slide()) else { return vec![] };
        sl.shapes
            .iter()
            .filter_map(|sh| sh.text.as_ref())
            .flat_map(|b| b.paragraphs.iter())
            .flat_map(|p| p.runs.iter())
            .filter_map(|r| match &r.kind {
                RunKind::Math { omml } => Some((r.text.clone(), omml.clone())),
                _ => None,
            })
            .collect()
    }

    fn undo_depth(app: &SlideApp) -> usize {
        app.session.active().map_or(0, |d| d.history.undo.len())
    }

    fn open_new(h: &mut Harness<'static, SlideApp>) {
        h.state_mut().run("app.dialog", json!({"id": "equation"})).unwrap();
        steps(h, 4);
    }

    /// The Equation tab: it appears with the dialog, its galleries insert into the widget, and
    /// it goes away with the dialog.
    #[test]
    fn the_equation_tab_inserts_from_its_galleries() {
        let mut h = harness();
        // The whole tab is about 1,300 points wide.
        h.set_size(vec2(1700.0, 860.0));
        assert_ne!(h.state().ui.tab, "Equation");
        open_new(&mut h);
        assert_eq!(h.state().ui.tab, "Equation", "editing an equation brings up the tab");
        h.get_by_label("Fraction").click();
        steps(&mut h, 3);
        h.get_by_label("Stacked Fraction").click();
        steps(&mut h, 3);
        let eq = h.state().equation.widget().map(|w| w.math().clone()).unwrap();
        assert!(matches!(eq.body.first().map(equashion_egui::equashion_math::Node::bare), Some(equashion_egui::equashion_math::Node::Frac { .. })), "{eq:?}");
        // The symbol strip is on the tab itself: the first set's first symbols.
        h.get_by_label("\u{221e}").click();
        steps(&mut h, 3);
        assert!(h.state().equation.widget().is_some_and(|w| w.linear().contains('\u{221e}')), "{:?}", h.state().equation.widget().map(|w| w.linear()));
        // Display and Inline switch the equation's flag.
        assert!(!h.state().equation.widget().unwrap().is_display());
        h.get_by_label("Display").click();
        steps(&mut h, 2);
        assert!(h.state().equation.widget().unwrap().is_display());
        h.state_mut().dialog = None;
        steps(&mut h, 3);
        assert!(!h.state().equation.is_open());
        assert_ne!(h.state().ui.tab, "Equation", "leaving the equation goes back");
    }

    /// Insert › Equation's arrow: a ready-made equation opens the editor started from it, and
    /// Enter inserts it as one undo step.
    #[test]
    fn a_ready_made_equation_opens_the_editor_started_from_it() {
        let mut h = harness();
        h.set_size(vec2(1700.0, 860.0));
        h.state_mut().ui.tab = "Insert".into();
        steps(&mut h, 3);
        let id = equashion_egui::equashion_math::galleries().builtins.first().map(|b| b.id.clone()).expect("a built-in equation");
        let mut d = crate::dialogs::Dialog::new("equation");
        d.params = json!({"builtin": id});
        h.state_mut().dialog = Some(d);
        steps(&mut h, 4);
        let w = h.state().equation.widget().expect("the editor is open");
        assert!(!w.linear().is_empty(), "started from the ready-made equation");
        assert!(h.state().equation.target().is_none(), "a new equation, not an edit");
        let depth = undo_depth(h.state());
        h.key_press(egui::Key::Enter);
        steps(&mut h, 3);
        assert_eq!(equations(h.state()).len(), 1);
        assert_eq!(undo_depth(h.state()), depth + 1, "one undo step");
    }

    #[test]
    fn insert_opens_the_widget_in_place_and_enter_inserts_one_undo_step() {
        let mut h = harness();
        let fonts_before = h.ctx.fonts(|f| f.definitions().clone());
        let style_before = h.ctx.global_style();
        open_new(&mut h);
        assert!(h.state().equation.is_open(), "Insert › Equation opens the equaSHion widget");
        assert!(h.query_by_label("Insert").is_none() && h.query_by_label("Cancel").is_none(), "no dialog buttons: the ribbon does the choosing");
        let canvas = h.state().equation.widget().map(|w| w.canvas_id());
        assert!(canvas.is_some_and(|c| h.ctx.memory(|m| m.has_focus(c))), "typing goes to the widget");
        let depth = undo_depth(h.state());
        type_text(&mut h, "x^2");
        assert_eq!(h.state().equation.widget().map(|w| w.linear()), Some("x^2".to_string()));
        assert!(equations(h.state()).is_empty(), "the presentation waits for Enter");
        h.key_press(egui::Key::Enter);
        steps(&mut h, 3);
        assert!(h.state().dialog.is_none() && !h.state().equation.is_open(), "Enter applies and closes");
        let eqs = equations(h.state());
        assert_eq!(eqs.len(), 1);
        assert!(eqs[0].1.contains("m:sSup"), "{eqs:?}");
        assert_eq!(undo_depth(h.state()), depth + 1, "one undo step");
        h.state_mut().run("edit.undo", json!({})).unwrap();
        steps(&mut h, 2);
        assert!(equations(h.state()).is_empty(), "one Undo takes the equation back");
        // The shared context keeps DeckCraft's fonts and style.
        assert!(h.ctx.fonts(|f| f.definitions().clone()) == fonts_before, "the widget installed fonts");
        assert!(*h.ctx.global_style() == *style_before, "the widget changed the shared style");
    }

    /// What a double-click on the equation asks for (see the engine's pointer tests).
    fn edit_request(h: &mut Harness<'static, SlideApp>, id: u64) {
        h.state_mut()
            .session
            .ui_requests
            .push(deckcraft_engine::UiRequest::Dialog { id: "equation".into(), params: json!({"edit": true, "id": id, "paragraph": 0, "run": 0}) });
        h.state_mut().run("edit.select", json!({"ids": [id]})).unwrap();
        steps(h, 4);
    }

    #[test]
    fn double_click_edits_the_equation_in_place_and_esc_keeps_the_edit() {
        let mut h = harness();
        let r = h.state_mut().run("insert.equation", json!({"linear": "a/b", "rect": [100, 100, 300, 60]})).unwrap();
        let id = r["id"].as_u64().unwrap();
        edit_request(&mut h, id);
        assert!(h.state().equation.is_open());
        assert_eq!(h.state().equation.widget().map(|w| w.linear()), Some("a/b".to_string()));
        assert_eq!(h.state().equation.target(), Some(&json!({"id": id, "paragraph": 0, "run": 0})));

        // Esc without a change closes and changes nothing.
        let depth = undo_depth(h.state());
        h.key_press(egui::Key::Escape);
        steps(&mut h, 3);
        assert!(!h.state().equation.is_open() && h.state().dialog.is_none());
        assert_eq!(undo_depth(h.state()), depth, "nothing changed, no undo step");

        // Typing then Esc keeps the edit, applied to that run in one undo step.
        edit_request(&mut h, id);
        type_text(&mut h, "+c");
        h.key_press(egui::Key::Escape);
        steps(&mut h, 3);
        assert!(!h.state().equation.is_open());
        let eqs = equations(h.state());
        assert_eq!(eqs.len(), 1, "updated, not inserted");
        assert!(eqs[0].0.ends_with("+c"), "{eqs:?}");
        assert_eq!(undo_depth(h.state()), depth + 1);
    }

    #[test]
    fn an_empty_new_equation_is_dropped() {
        let mut h = harness();
        let depth = undo_depth(h.state());
        open_new(&mut h);
        h.key_press(egui::Key::Escape);
        steps(&mut h, 3);
        assert!(!h.state().equation.is_open() && h.state().dialog.is_none());
        assert!(equations(h.state()).is_empty());
        assert_eq!(undo_depth(h.state()), depth);
    }

    #[test]
    fn pasting_an_equation_on_the_slide_inserts_one() {
        let mut h = harness();
        h.event(egui::Event::Paste("$$\\frac{a}{b}$$".into()));
        steps(&mut h, 2);
        let eqs = equations(h.state());
        assert_eq!(eqs.len(), 1, "a delimited LaTeX paste becomes an equation");
        assert!(eqs[0].1.contains("<m:f>") && eqs[0].1.contains("oMathPara"), "{eqs:?}");
        // Text with dollar signs stays text.
        h.event(egui::Event::Paste("costs $5 and $6".into()));
        steps(&mut h, 2);
        assert_eq!(equations(h.state()).len(), 1);
    }
}
