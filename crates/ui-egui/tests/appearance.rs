//! Issue #41: saved appearance, actual app frames, all controls and native viewport output.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use deckcraft_engine::Session;
use deckcraft_ui_egui::{
    Services, SlideApp, UiState,
    theme::{Brightness, Tokens},
};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;

fn frame(app: &mut SlideApp, ctx: &egui::Context, system: Option<egui::Theme>) -> egui::FullOutput {
    let mut output = ctx.run_ui(egui::RawInput { system_theme: system, ..Default::default() }, |ui| app.logic(ui.ctx()));
    // These tests inspect logic/styles/viewport commands; the native proof renders textures.
    // egui requires a non-rendering backend to explicitly consume its unused texture deltas.
    output.textures_delta.clear();
    output
}

#[test]
fn saved_modes_defaults_and_legacy_values() {
    assert_eq!(UiState::default().brightness, Brightness::Light);
    assert_eq!(serde_json::from_value::<UiState>(json!({})).unwrap().brightness, Brightness::Light);
    for (name, mode) in [("light", Brightness::Light), ("dark", Brightness::Dark), ("system", Brightness::System)] {
        let prefs: UiState = serde_json::from_value(json!({"brightness": name})).unwrap();
        assert_eq!(prefs.brightness, mode);
        let saved = serde_json::to_value(&prefs).unwrap();
        assert_eq!(saved["brightness"], name);
        let restored: UiState = serde_json::from_value(saved).unwrap();
        assert_eq!(restored.brightness, mode);
    }
}

#[test]
fn system_transitions_styles_native_inheritance_and_document_independence() {
    let ctx = egui::Context::default();
    // Recover from an embedding host that pinned egui before starting the app.
    ctx.set_theme(egui::Theme::Dark);
    let mut app = SlideApp::new(Session::with_new(), Services::default());
    let before = serde_json::to_value(&app.session.active().unwrap().doc).unwrap();
    app.run("view.appearance", json!({"brightness": "system"})).unwrap();
    for (step, (system, resolved)) in [
        (Some(egui::Theme::Light), Brightness::Light),
        (Some(egui::Theme::Dark), Brightness::Dark),
        (None, Brightness::Light), // Missing appearance must not retain the preceding Dark palette.
        (Some(egui::Theme::Light), Brightness::Light),
        (None, Brightness::Light),
    ]
    .into_iter()
    .enumerate()
    {
        let out = frame(&mut app, &ctx, system);
        assert_eq!(app.ui.brightness, Brightness::System);
        assert_eq!(ctx.options(|o| o.theme_preference), egui::ThemePreference::System);
        let tokens = Tokens::for_brightness(resolved);
        assert_eq!(Tokens::get(&ctx).dark, tokens.dark);
        for branch in [egui::Theme::Light, egui::Theme::Dark] {
            let style = ctx.style_of(branch);
            assert_eq!(style.visuals.dark_mode, tokens.dark);
            assert_eq!(style.visuals.panel_fill, tokens.chrome);
            assert_eq!(style.visuals.window_fill, tokens.card);
            assert_eq!(style.visuals.override_text_color, Some(tokens.text));
        }
        let native: Vec<_> = out
            .viewport_output
            .values()
            .flat_map(|v| &v.commands)
            .filter_map(|c| if let egui::ViewportCommand::SetTheme(t) = c { Some(*t) } else { None })
            .collect();
        if step == 0 {
            assert_eq!(native, [egui::SystemTheme::SystemDefault]);
        }
        assert!(native.iter().all(|t| *t == egui::SystemTheme::SystemDefault), "concrete native pin: {native:?}");
        let saved = serde_json::to_value(&app.ui).unwrap();
        assert_eq!(saved["brightness"], "system");
        let restored: UiState = serde_json::from_value(saved).unwrap();
        assert_eq!(restored.brightness, Brightness::System);
    }
    assert_eq!(serde_json::to_value(&app.session.active().unwrap().doc).unwrap(), before);
    let restarted_ctx = egui::Context::default();
    let mut restarted = SlideApp::new(Session::new(), Services::default());
    restarted.ui = serde_json::from_slice(&serde_json::to_vec(&app.ui).unwrap()).unwrap();
    frame(&mut restarted, &restarted_ctx, Some(egui::Theme::Dark));
    assert!(Tokens::get(&restarted_ctx).dark);
    assert_eq!(restarted.ui.brightness, Brightness::System);
}

#[test]
fn manual_modes_ignore_native_and_host_reports_and_legacy_command_still_works() {
    let ctx = egui::Context::default();
    let mut app = SlideApp::new(
        Session::new(),
        Services { system_theme: Some(Box::new(|| panic!("manual mode must not consult the desktop feed"))), ..Default::default() },
    );
    for (mode, dark) in [(Brightness::Light, false), (Brightness::Dark, true)] {
        app.run("view.appearance", json!({"brightness": mode})).unwrap();
        for system in [Some(egui::Theme::Light), Some(egui::Theme::Dark), None] {
            frame(&mut app, &ctx, system);
            assert_eq!(app.ui.brightness, mode);
            assert_eq!(Tokens::get(&ctx).dark, dark);
        }
    }
    app.services.system_theme = None;
    app.run("view.appearance", json!({"brightness": "system"})).unwrap();
    frame(&mut app, &ctx, Some(egui::Theme::Dark));
    app.run("view.dark", json!({})).unwrap();
    assert_eq!(app.ui.brightness, Brightness::Light);
    app.run("view.dark", json!({"on": true})).unwrap();
    assert_eq!(app.ui.brightness, Brightness::Dark);
    app.run("view.dark", json!({})).unwrap();
    assert_eq!(app.ui.brightness, Brightness::Light, "back-to-back commands use the manual selection");
    app.run("view.dark", json!({"on": true})).unwrap();
    assert!(app.run("view.appearance", json!({"brightness": "invalid"})).is_err());
    assert_eq!(app.ui.brightness, Brightness::Dark);
}

#[test]
fn host_feed_overrides_unavailable_or_stale_window_backend() {
    use std::cell::Cell;
    use std::rc::Rc;
    let report = Rc::new(Cell::new(Some(egui::Theme::Dark)));
    let feed = report.clone();
    let mut app = SlideApp::new(Session::new(), Services { system_theme: Some(Box::new(move || feed.get())), ..Default::default() });
    app.ui.brightness = Brightness::System;
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, Some(egui::Theme::Light));
    assert!(Tokens::get(&ctx).dark);
    report.set(Some(egui::Theme::Light));
    frame(&mut app, &ctx, Some(egui::Theme::Dark));
    assert!(!Tokens::get(&ctx).dark);
    report.set(None);
    frame(&mut app, &ctx, None);
    assert!(!Tokens::get(&ctx).dark);
    assert_eq!(app.ui.brightness, Brightness::System);
}

#[test]
fn every_existing_control_selects_and_displays_the_saved_mode() {
    for location in ["title", "view", "preferences"] {
        let mut app = SlideApp::new(Session::with_new(), Services::default());
        if location == "view" {
            app.ui.tab = "View".into();
        }
        if location == "preferences" {
            app.run("app.preferences", json!({})).unwrap();
        }
        let mut harness = Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
            |ui, app: &mut SlideApp| {
                app.logic(ui.ctx());
                app.ui(ui);
            },
            app,
        );
        harness.run_steps(3);
        // View and Preferences each add a second selector after the title-bar selector.
        let index = usize::from(location != "title");
        harness.get_all_by_value("Appearance: Light").nth(index).unwrap().click();
        harness.run_steps(3);
        harness.get_by_label("System").click();
        harness.run_steps(3);
        assert_eq!(harness.state().ui.brightness, Brightness::System, "{location}");
        assert!(harness.get_all_by_value("Appearance: System").count() > index, "{location} displays the saved choice");
        for mode in ["Dark", "Light"] {
            let label = format!("Appearance: {}", harness.state().ui.brightness.label());
            harness.get_all_by_value(&label).nth(index).unwrap().click();
            harness.run_steps(3);
            harness.get_by_label(mode).click();
            harness.run_steps(3);
            assert_eq!(harness.state().ui.brightness.label(), mode, "{location}");
        }
    }
}
