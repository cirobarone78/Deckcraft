//! Headless slide-show UI tests (egui_kittest): the on-screen bar and the keep-ink prompt
//! are verified structurally (widget tree) and, when a GPU is available, rendered to
//! `target/show-ui/*.png` for visual inspection.

use deckcraft_engine::Session;
use deckcraft_ui_egui::{Services, SlideApp};
use egui_kittest::kittest::Queryable;

type Harness = egui_kittest::Harness<'static, SlideApp>;

fn make_show() -> Harness {
    let mut app = SlideApp::new(Session::new(), Services::default());
    deckcraft_engine::sample::open_sample(&mut app.session).expect("sample deck");
    app.ui.presenter_view = false;
    app.start_show(0, false);
    let builder = egui_kittest::Harness::builder().with_size(egui::vec2(1280.0, 800.0)).with_max_steps(8);
    // wgpu rendering when a GPU adapter is available; otherwise structure-only.
    let builder = match std::panic::catch_unwind(egui_kittest::wgpu::WgpuTestRenderer::new) {
        Ok(r) => builder.renderer(r),
        Err(_) => builder,
    };
    let mut h = builder.build_ui_state(
        move |ui, app: &mut SlideApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        app,
    );
    h.run_steps(3);
    h
}

fn key(h: &mut Harness, k: egui::Key, command: bool) {
    let mods = if command { egui::Modifiers::COMMAND } else { egui::Modifiers::NONE };
    for pressed in [true, false] {
        h.input_mut().events.push(egui::Event::Key { key: k, physical_key: None, pressed, repeat: false, modifiers: mods });
        h.step();
    }
}

fn drag(h: &mut Harness, from: (f32, f32), to: (f32, f32)) {
    h.input_mut().events.push(egui::Event::PointerMoved(egui::pos2(from.0, from.1)));
    h.step();
    h.input_mut().events.push(egui::Event::PointerButton {
        pos: egui::pos2(from.0, from.1),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.step();
    let steps = 12;
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        h.input_mut().events.push(egui::Event::PointerMoved(egui::pos2(from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t)));
        h.step();
    }
    h.input_mut().events.push(egui::Event::PointerButton {
        pos: egui::pos2(to.0, to.1),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.step();
}

fn dump(h: &mut Harness, name: &str) {
    if let Ok(img) = h.render() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/show-ui");
        let _ = std::fs::create_dir_all(dir);
        let _ = img.save(format!("{dir}/{name}.png"));
    }
}

/// The band between the tool buttons and Undo holds the swatches and width dots.
fn extras_band(h: &Harness) -> Option<egui::Rect> {
    let l = h.get_by_label("Laser pointer").rect();
    let undo = h.get_by_label("Undo").rect();
    (undo.left() > l.right()).then_some(egui::Rect::from_min_max(egui::pos2(l.right(), l.top()), egui::pos2(undo.left(), l.bottom())))
}

#[test]
fn show_bar_shows_all_tools() {
    let mut h = make_show();
    h.run_steps(2);
    // Every control exists in the widget tree.
    for label in ["Pen", "Highlighter", "Eraser", "Laser pointer", "Undo", "Clear", "End"] {
        h.get_by_label(label);
    }
    dump(&mut h, "bar");
    if let Ok(img) = h.render() {
        let band = extras_band(&h).expect("extras between L and Undo");
        // The red swatch is actually painted (not just laid out).
        let mut red = false;
        let mut white = false;
        for y in band.top() as u32..band.bottom() as u32 {
            for x in band.left() as u32..band.right() as u32 {
                let p = img.get_pixel(x, y).0;
                if (p[0] as i32 - 0xE5).abs() < 12 && (p[1] as i32 - 0x39).abs() < 12 && (p[2] as i32 - 0x35).abs() < 12 {
                    red = true;
                }
                if p[0] > 250 && p[1] > 250 && p[2] > 250 {
                    white = true;
                }
            }
        }
        assert!(red, "no red swatch painted in the bar");
        assert!(white, "no selected width dot / white swatch painted in the bar");
    }
}

#[test]
fn undo_clear_disabled_state_tracks_ink() {
    use egui_kittest::kittest::NodeT;
    let mut h = make_show();
    h.run_steps(2);
    let disabled = |h: &Harness, label: &str| h.get_by_label(label).accesskit_node().is_disabled();
    assert!(disabled(&h, "Undo"), "nothing to undo on a fresh show");
    assert!(disabled(&h, "Clear"), "no ink to clear on a fresh show");
    key(&mut h, egui::Key::P, true);
    drag(&mut h, (300.0, 400.0), (900.0, 500.0));
    h.run_steps(2);
    assert!(!disabled(&h, "Undo"), "a drawn stroke can be undone");
    assert!(!disabled(&h, "Clear"), "a drawn stroke can be cleared");
    h.get_by_label("Clear").click();
    h.run_steps(2);
    assert!(!disabled(&h, "Undo"), "the clear itself is undoable");
    assert!(disabled(&h, "Clear"), "no ink left to clear");
    h.get_by_label("Undo").click();
    h.run_steps(2);
    assert!(!disabled(&h, "Clear"), "undo restored the stroke");
}

#[test]
fn pen_draws_and_keep_prompt_has_buttons() {
    let mut h = make_show();
    h.run_steps(2);
    key(&mut h, egui::Key::P, true);
    drag(&mut h, (300.0, 400.0), (900.0, 500.0));
    let showing = format!("{:?}", h.state().show.as_ref().map(|s| s.ink().count()));
    assert!(h.state().show.as_ref().is_some_and(|s| s.ink().count() == 1), "stroke drawn: {showing}");
    dump(&mut h, "ink");
    key(&mut h, egui::Key::Escape, false);
    h.run_steps(2);
    // The keep prompt is up and its buttons exist, are painted, and are clickable.
    h.get_by_label("Keep");
    h.get_by_label("Discard");
    if let Ok(img) = h.render() {
        let keep = h.get_by_label("Keep").rect();
        let (mut fill, mut total) = (0, 0);
        for y in keep.top() as u32..keep.bottom() as u32 {
            for x in keep.left() as u32..keep.right() as u32 {
                total += 1;
                if img.get_pixel(x, y).0 == [70, 70, 70, 255] {
                    fill += 1;
                }
            }
        }
        assert!(total > 0 && fill * 2 > total, "Keep button is laid out but not painted (fill {fill}/{total})");
    }
    dump(&mut h, "keep_prompt");
    h.get_by_label("Keep").click();
    h.run_steps(2);
    let doc_has_ink = h
        .state()
        .session
        .active()
        .is_some_and(|d| d.doc.slides.iter().flat_map(|s| s.shapes.iter()).any(|sh| matches!(sh.kind, deckcraft_model::ShapeKind::Ink { .. })));
    assert!(doc_has_ink, "kept ink should land in the document");
}
