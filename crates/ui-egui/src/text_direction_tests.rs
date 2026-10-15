//! Home › Paragraph's direction buttons, as Office shows them: Right-to-Left and Left-to-Right
//! set the paragraph's reading direction and mirror its alignment; the pressed one follows the
//! paragraph.

use deckcraft_engine::Session;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

use crate::{Services, SlideApp};

const RTL: &str = "Right-to-Left Text Direction";
const LTR: &str = "Left-to-Right Text Direction";

/// A selected rectangle holding `text` (centred, as shape text is).
fn harness(text: &str) -> Harness<'static, SlideApp> {
    let mut s = Session::with_new();
    s.execute("shape.insert", &json!({"preset": "rect", "rect": [80.0, 80.0, 320.0, 80.0], "text": text})).unwrap();
    let _ = s.execute("text.exit", &json!({}));
    let app = SlideApp::new(s, Services::default());
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 900.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut SlideApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    h
}

/// The first paragraph's (direction, alignment) of the selected shape.
fn direction(h: &Harness<'_, SlideApp>) -> (Option<bool>, Option<String>) {
    let st = h.state().session.active().unwrap();
    let p = st.selected_shapes().first().and_then(|sh| sh.text.as_ref()).and_then(|t| t.paragraphs.first()).cloned().unwrap_or_default();
    (p.props.rtl, p.props.align.map(|a| a.xml().to_string()))
}

#[test]
fn the_direction_buttons_set_the_paragraph_direction_and_mirror_its_alignment() {
    let mut h = harness("שלום 2026");
    assert_eq!(direction(&h).0, None);
    h.get_by_label(RTL).click();
    h.run_steps(2);
    assert_eq!(direction(&h), (Some(true), Some("ctr".into())), "right to left; centred text stays centred");
    assert_eq!(h.state().session.active().unwrap().history.undo.last().unwrap().label, RTL);
    h.get_by_label(LTR).click();
    h.run_steps(2);
    assert_eq!(direction(&h), (Some(false), Some("ctr".into())), "back to left to right");
    // Left-aligned text moves to the right edge with the direction, as in Office.
    h.state_mut().session.execute("format.alignLeft", &json!({})).unwrap();
    h.run_steps(1);
    h.get_by_label(RTL).click();
    h.run_steps(2);
    assert_eq!(direction(&h), (Some(true), Some("r".into())));
}

#[test]
fn the_pressed_button_follows_the_paragraph() {
    let mut h = harness("Hello");
    let st = crate::ribbon::fmt_state(h.state_mut());
    assert_eq!(st["rtl"], json!(false));
    h.get_by_label(RTL).click();
    h.run_steps(2);
    let st = crate::ribbon::fmt_state(h.state_mut());
    assert_eq!(st["rtl"], json!(true), "format.state reports the direction the ribbon shows pressed");
}
