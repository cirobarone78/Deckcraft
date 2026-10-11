//! The equation editor: equaSHion's [`EquationEditor`] widget in the Equation dialog.
//!
//! It opens from Insert › Equation (a new equation) and from a double-click on an equation (the
//! engine asks for the `equation` dialog with `{edit: true, id, cell?, paragraph, run}`). The
//! widget edits a copy, loaded from the run's OMML through `equation.get`; the presentation
//! changes once, through the engine's own commands, so undo and save work as for any other edit:
//! - Insert / Update or Enter applies it: `insert.equation` (a new text box) or
//!   `equation.update` (that run), each one undo step. An empty equation is not applied.
//! - Esc follows the widget's [`equashion_egui::DISCARD_PROMPT`] (a second Esc when there are
//!   changes); Cancel and the close button discard at once. Nothing was changed, so nothing is
//!   undone.
//!
//! The widget shares the app's `egui::Context` and never touches its fonts, visuals or style:
//! it draws the equation with its own bundled fonts. Only this dialog's `Ui` gets the theme's
//! accent as the selection stroke, which the widget uses for its caret and focus ring.

use egui::{Ui, vec2};
use equashion_egui::{DISCARD_PROMPT, EquationEditor};
use serde_json::{Value, json};

use crate::SlideApp;
use crate::dialogs::{Dialog, buttons};
use crate::theme::Tokens;

/// Width of the editor's contents, points.
const WIDTH: f32 = 560.0;

/// Parameters that say which equation run an edit targets.
const TARGET_KEYS: [&str; 4] = ["id", "cell", "paragraph", "run"];

/// The editor while the Equation dialog is open.
pub(crate) struct Open {
    widget: EquationEditor,
    /// The dialog parameters it was opened with (a new request reloads).
    params: Value,
    /// Where the edited equation lives (`id`, `cell`, `paragraph`, `run`); `None` inserts a new one.
    target: Option<Value>,
    /// Why loading or applying failed, or the discard prompt.
    error: Option<String>,
    /// Esc was pressed once with unsaved changes while the widget did not have the keyboard.
    esc_armed: bool,
}

/// The equation editor's state in [`SlideApp`].
#[derive(Default)]
pub struct EquationHost {
    pub(crate) open: Option<Open>,
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
        widget: EquationEditor::new().id_salt("deckcraft-equation-editor"),
        params: params.clone(),
        target: None,
        error: None,
        esc_armed: false,
    };
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

/// The Equation dialog's body. True when the dialog should close.
pub fn body(app: &mut SlideApp, ui: &mut Ui, d: &mut Dialog) -> bool {
    let mut o = match app.equation.open.take() {
        Some(o) if o.params == d.params => o,
        _ => open(app, &d.params),
    };
    let had_keyboard = ui.memory(|m| m.has_focus(o.widget.canvas_id()) || m.has_focus(o.widget.linear_id()));
    let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
    let t = Tokens::get(ui.ctx());

    ui.set_width(WIDTH);
    // The widget's caret and focus ring use the selection stroke: give it the theme's accent
    // here (DeckCraft's own stroke is the white text drawn on a selection).
    ui.visuals_mut().selection.stroke.color = t.accent;
    let r = o.widget.show(ui);
    // One line is always reserved for the error, so showing it does not move the buttons.
    ui.allocate_ui_with_layout(vec2(WIDTH, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        if let Some(e) = &o.error {
            ui.colored_label(t.accent, e);
        }
    });
    let editing = o.target.is_some();
    let (ok, cancel) = buttons(ui, if editing { "Update" } else { "Insert" });

    if r.changed {
        o.error = None;
        o.esc_armed = false;
    }
    let mut close = cancel || r.cancelled;
    // Esc the widget did not see (it did not have the keyboard): the same rule as its own.
    if esc && !had_keyboard && !close {
        if !o.widget.is_dirty() || o.esc_armed {
            close = true;
        } else {
            o.esc_armed = true;
            o.error = Some(DISCARD_PROMPT.to_string());
        }
    }
    if !close && (ok || r.committed) {
        match apply(app, &mut o) {
            Ok(()) => close = true,
            Err(e) => o.error = Some(e),
        }
    }
    if !close {
        app.equation.open = Some(o);
    }
    close
}

#[cfg(test)]
mod tests {
    use deckcraft_engine::Session;
    use deckcraft_model::text::RunKind;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

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

    #[test]
    fn insert_opens_the_widget_and_enter_inserts_one_undo_step() {
        let mut h = harness();
        let fonts_before = h.ctx.fonts(|f| f.definitions().clone());
        let style_before = h.ctx.global_style();
        open_new(&mut h);
        assert!(h.state().equation.is_open(), "Insert › Equation opens the equaSHion widget");
        assert!(h.query_by_label("Insert").is_some() && h.query_by_label("Cancel").is_some());
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

    #[test]
    fn double_click_edits_the_equation_in_place_and_esc_asks_before_discarding() {
        let mut h = harness();
        let r = h.state_mut().run("insert.equation", json!({"linear": "a/b", "rect": [100, 100, 300, 60]})).unwrap();
        let id = r["id"].as_u64().unwrap();
        // What a double-click on the equation asks for (see the engine's pointer tests).
        h.state_mut()
            .session
            .ui_requests
            .push(deckcraft_engine::UiRequest::Dialog { id: "equation".into(), params: json!({"edit": true, "id": id, "paragraph": 0, "run": 0}) });
        h.state_mut().run("edit.select", json!({"ids": [id]})).unwrap();
        steps(&mut h, 4);
        assert!(h.query_by_label("Update").is_some(), "an existing equation is updated");
        assert_eq!(h.state().equation.widget().map(|w| w.linear()), Some("a/b".to_string()));
        assert_eq!(h.state().equation.target(), Some(&json!({"id": id, "paragraph": 0, "run": 0})));

        // Esc with changes asks first; the second Esc discards and leaves the slide as it was.
        type_text(&mut h, "+c");
        h.key_press(egui::Key::Escape);
        steps(&mut h, 2);
        assert!(h.query_by_label(DISCARD_PROMPT).is_some(), "the first Esc asks");
        assert!(h.state().equation.is_open());
        h.key_press(egui::Key::Escape);
        steps(&mut h, 3);
        assert!(!h.state().equation.is_open() && h.state().dialog.is_none(), "the second Esc discards");
        assert_eq!(equations(h.state())[0].0, "a/b");

        // Again, and Update applies to that run in one undo step.
        h.state_mut()
            .session
            .ui_requests
            .push(deckcraft_engine::UiRequest::Dialog { id: "equation".into(), params: json!({"edit": true, "id": id, "paragraph": 0, "run": 0}) });
        h.state_mut().run("edit.select", json!({"ids": [id]})).unwrap();
        steps(&mut h, 4);
        let depth = undo_depth(h.state());
        type_text(&mut h, "+c");
        h.get_by_label("Update").click();
        steps(&mut h, 3);
        assert!(!h.state().equation.is_open());
        let eqs = equations(h.state());
        assert_eq!(eqs.len(), 1, "updated, not inserted");
        assert!(eqs[0].0.ends_with("+c"), "{eqs:?}");
        assert_eq!(undo_depth(h.state()), depth + 1);
    }

    #[test]
    fn cancel_and_empty_equations_change_nothing() {
        let mut h = harness();
        let depth = undo_depth(h.state());
        open_new(&mut h);
        type_text(&mut h, "y");
        h.get_by_label("Cancel").click();
        steps(&mut h, 3);
        assert!(!h.state().equation.is_open() && h.state().dialog.is_none());
        assert!(equations(h.state()).is_empty());
        assert_eq!(undo_depth(h.state()), depth);
        // Insert on an empty equation says why and stays open.
        open_new(&mut h);
        h.get_by_label("Insert").click();
        steps(&mut h, 2);
        assert!(h.state().equation.is_open());
        assert!(h.query_by_label("Type or build an equation first.").is_some());
        assert_eq!(undo_depth(h.state()), depth);
        // A clean editor closes on the first Esc.
        h.key_press(egui::Key::Escape);
        steps(&mut h, 3);
        assert!(!h.state().equation.is_open());
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
