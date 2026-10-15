//! Click Equation, then type on the slide without a second click or a modal dialog.

use deckcraft_engine::Session;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

use crate::{Services, SlideApp};

fn harness(inline: bool) -> Harness<'static, SlideApp> {
    let mut s = Session::with_new();
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    if inline {
        s.execute("insert.textBox", &json!({"rect": [90, 200, 700, 80], "text": "some normal text "})).unwrap();
        s.execute("text.edit", &json!({})).unwrap();
    }
    let mut app = SlideApp::new(s, Services::default());
    app.ui.tab = "Insert".into();
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 900.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut SlideApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(3);
    h
}

fn selected_text(h: &Harness<'_, SlideApp>) -> String {
    let st = h.state().session.active().unwrap();
    let sel = st.selection.text.as_ref().expect("slide keeps the text caret");
    deckcraft_engine::cmd::text::body_of(st, sel).unwrap().text()
}

fn preview(h: &mut Harness<'_, SlideApp>, name: &str) {
    if let Ok(dir) = std::env::var("EQUATION_PREVIEW_DIR") {
        h.render().expect("render equation UI").save(std::path::Path::new(&dir).join(name)).unwrap();
    }
}

#[test]
fn equation_button_inserts_and_types_without_an_extra_click() {
    let mut h = harness(false);
    let before = h.state().session.active().unwrap().shapes().len();
    h.get_by_label("Equation").click();
    h.run_steps(3);
    assert!(h.state().dialog.is_none());
    assert_eq!(h.state().session.active().unwrap().shapes().len(), before + 1);
    assert_eq!(selected_text(&h), "Type equation here");
    assert_eq!(h.state().ui.tab, "Equation");
    preview(&mut h, "equation-placeholder.png");
    for c in "\\alpha".chars() {
        h.event(egui::Event::Text(c.to_string()));
        h.run_steps(1);
    }
    assert_eq!(selected_text(&h), "\\alpha");
    h.event(egui::Event::Text(" ".into()));
    h.run_steps(2);
    assert_eq!(selected_text(&h), "α");
    h.event(egui::Event::Text(" + \\beta = x^2 ".into()));
    h.run_steps(3);
    assert_eq!(selected_text(&h), "α + β= x²");
    preview(&mut h, "equation-typing.png");
}

#[test]
fn equation_button_inserts_at_the_existing_text_caret() {
    let mut h = harness(true);
    let before = h.state().session.active().unwrap().shapes().len();
    h.get_by_label("Equation").click();
    h.run_steps(3);
    assert_eq!(h.state().session.active().unwrap().shapes().len(), before);
    assert_eq!(selected_text(&h), "some normal text Type equation here");
    preview(&mut h, "equation-inline-placeholder.png");
    h.event(egui::Event::Text("\\alpha + \\beta ".into()));
    h.run_steps(3);
    assert_eq!(selected_text(&h), "some normal text α+ β");
    preview(&mut h, "equation-inline.png");
}

#[test]
fn equation_legacy_dialog_command_inserts_directly() {
    let mut app = SlideApp::new(Session::with_new(), Services::default());
    app.run("app.dialog", json!({"id": "equation"})).unwrap();
    assert!(app.dialog.is_none());
    assert!(app.session.active().unwrap().selection.text.is_some());
}

fn click(h: &mut Harness<'_, SlideApp>, p: egui::Pos2) {
    h.hover_at(p);
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
    }
    h.run_steps(3);
}

fn text_point(h: &Harness<'_, SlideApp>, ch: usize) -> egui::Pos2 {
    let app = h.state();
    let st = app.session.active().unwrap();
    let mut t = st.selection.text.clone().unwrap();
    t.anchor = (0, ch);
    t.caret = t.anchor;
    let (top, bottom) = deckcraft_engine::cmd::text::caret_line(st, &t).unwrap();
    app.slide_rect.unwrap().min + egui::vec2(top.x as f32, ((top.y + bottom.y) / 2.0) as f32) * app.canvas_scale
}

#[test]
fn equation_context_tab_and_shading_follow_clicks_and_arrow_exit() {
    let mut h = harness(true);
    h.get_by_label("Equation").click();
    h.run_steps(3);
    h.event(egui::Event::Text("x + y".into()));
    h.run_steps(3);
    assert_eq!(h.state().ui.tab, "Equation");
    let st = h.state().session.active().unwrap();
    let field = deckcraft_engine::cmd::equation::active(st).unwrap();
    assert_eq!(field.ordered(), ((0, 17), (0, 22)));
    let marks = crate::canvas::text_marks(st, &field).unwrap();
    assert_eq!(marks.selection.len(), 1, "one shaded rectangle around the inline equation");
    assert!(deckcraft_engine::cmd::text::body_of(st, &field).unwrap().paragraphs[0].runs.iter().all(|r| r.props.highlight.is_none()));
    preview(&mut h, "equation-context-active.png");

    h.key_press(egui::Key::ArrowRight);
    h.run_steps(3);
    assert_eq!(h.state().ui.tab, "Insert");
    assert!(deckcraft_engine::cmd::equation::active(h.state().session.active().unwrap()).is_none());
    h.event(egui::Event::Text(" normal again".into()));
    h.run_steps(3);
    assert_eq!(selected_text(&h), "some normal text x + y normal again");
    assert_ne!(h.state_mut().run("format.state", json!({})).unwrap()["italic"], true);
    preview(&mut h, "equation-context-exited.png");

    h.get_by_label("Home tab").click();
    h.run_steps(3);
    let math = text_point(&h, 19);
    click(&mut h, math);
    assert_eq!(h.state().ui.tab, "Equation", "clicking an existing equation activates its tab");
    let normal = text_point(&h, 5);
    click(&mut h, normal);
    assert_eq!(h.state().ui.tab, "Home", "clicking ordinary text restores the original tab");
    assert!(deckcraft_engine::cmd::equation::active(h.state().session.active().unwrap()).is_none());

    // The first click into an unselected text box works as well.
    let blank = h.state().slide_rect.unwrap().min + egui::vec2(100.0, 100.0) * h.state().canvas_scale;
    click(&mut h, blank);
    click(&mut h, math);
    assert_eq!(h.state().ui.tab, "Equation");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert_eq!(h.state().ui.tab, "Home");
    assert!(h.state().equation_previous_tab.is_none());
}

#[test]
fn equation_context_allows_manual_tab_changes_and_left_arrow_reentry() {
    let mut h = harness(false);
    h.get_by_label("Equation").click();
    h.run_steps(3);
    h.event(egui::Event::Text("xy".into()));
    h.run_steps(3);
    h.get_by_label("View tab").click();
    h.run_steps(3);
    assert_eq!(h.state().ui.tab, "View", "context does not override a manual tab choice every frame");
    h.key_press(egui::Key::ArrowRight);
    h.run_steps(3);
    assert_eq!(h.state().ui.tab, "Insert");
    h.key_press(egui::Key::ArrowLeft);
    h.run_steps(3);
    assert_eq!(h.state().ui.tab, "Equation");
    assert_eq!(h.state().session.active().unwrap().selection.text.as_ref().unwrap().caret, (0, 2));
}
