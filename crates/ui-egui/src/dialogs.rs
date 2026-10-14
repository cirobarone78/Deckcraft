//! Dialogs (Insert Table, Slide Size, Header & Footer, Hyperlink, Set Up Show, Zoom, …) and the
//! About window. The start screen is File › Home (`backstage.rs`).

use egui::{Align2, Color32, CornerRadius, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use crate::theme::{self, Tokens};
use crate::{SlideApp, i18n};

#[derive(Clone, Debug, serde::Serialize)]
pub struct Dialog {
    pub id: String,
    pub params: Value,
    /// Field values while the dialog is open.
    #[serde(skip)]
    pub fields: std::collections::HashMap<String, String>,
}

impl Dialog {
    pub fn new(id: &str) -> Self {
        Dialog { id: id.into(), params: Value::Null, fields: Default::default() }
    }
    pub fn modal(&self) -> bool {
        true
    }
    fn get(&mut self, k: &str, default: &str) -> String {
        self.fields.entry(k.into()).or_insert_with(|| default.to_string()).clone()
    }
}

fn title(lang: i18n::Resolved, id: &str) -> &'static str {
    match id {
        "table" => i18n::text(lang, "dialog.table.title"),
        "slideSize" => i18n::text(lang, "dialog.slide_size.title"),
        "headerFooter" => i18n::text(lang, "dialog.header_footer.title"),
        "hyperlink" => i18n::text(lang, "dialog.hyperlink.title"),
        "setupShow" => i18n::text(lang, "dialog.setup_show.title"),
        "zoom" => i18n::text(lang, "dialog.zoom.title"),
        "comment" => i18n::text(lang, "dialog.comment.title"),
        "altText" => i18n::text(lang, "dialog.alt_text.title"),
        "outline" => i18n::text(lang, "dialog.outline.title"),
        "renameSection" => i18n::text(lang, "dialog.rename_section.title"),
        "renameShape" => i18n::text(lang, "dialog.rename.title"),
        "renameLayout" => i18n::text(lang, "dialog.rename_layout.title"),
        "paragraph" => i18n::text(lang, "dialog.paragraph.title"),
        "about" => i18n::text(lang, "dialog.about.title"),
        "preferences" => i18n::text(lang, "pref.title"),
        "export" => i18n::text(lang, "nav.export"),
        "symbol" => i18n::text(lang, "dialog.symbol.title"),
        "equation" => i18n::text(lang, "dialog.equation.title"),
        "spelling" => i18n::text(lang, "dialog.spelling.title"),
        "accessibility" => i18n::text(lang, "dialog.accessibility.title"),
        "language" => i18n::text(lang, "dialog.language.title"),
        "customShow" => i18n::text(lang, "dialog.custom_show.title"),
        "chartData" => i18n::text(lang, "dialog.chart.title"),
        "trim" => i18n::text(lang, "dialog.trim.title"),
        "smartart" => i18n::text(lang, "dialog.smartart.title"),
        "quit" => "DeckCraft",
        _ => "DeckCraft",
    }
}

pub fn show(app: &mut SlideApp, ctx: &egui::Context) {
    let lang = app.lang();
    let Some(mut d) = app.dialog.take() else { return };
    let mut open = true;
    let mut close = false;
    let heading = title(lang, &d.id);
    egui::Window::new(heading)
        .id(egui::Id::new(("dialog", d.id.clone())))
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, vec2(0.0, -40.0))
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_min_width(340.0);
            close = body(app, ui, &mut d);
        });
    if open && !close && !ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.dialog = Some(d);
    }
}

fn buttons(ui: &mut Ui, lang: i18n::Resolved, ok_label: &str) -> (bool, bool) {
    buttons_labeled(ui, ok_label, i18n::text(lang, "pref.cancel"))
}

fn buttons_labeled(ui: &mut Ui, ok_label: &str, cancel_label: &str) -> (bool, bool) {
    let mut ok = false;
    let mut cancel = false;
    ui.add_space(8.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let t = Tokens::get(ui.ctx());
        if ui.add(egui::Button::new(egui::RichText::new(ok_label).color(t.accent_text)).fill(t.accent).min_size(vec2(72.0, 24.0))).clicked()
            || ui.input(|i| i.key_pressed(egui::Key::Enter))
        {
            ok = true;
        }
        if ui.add(egui::Button::new(cancel_label).min_size(vec2(72.0, 24.0))).clicked() {
            cancel = true;
        }
    });
    (ok, cancel)
}

fn field(ui: &mut Ui, d: &mut Dialog, label: &str, key: &str, default: &str) {
    let mut v = d.get(key, default);
    ui.horizontal(|ui| {
        ui.add_sized(vec2(120.0, 20.0), egui::Label::new(label));
        ui.add(egui::TextEdit::singleline(&mut v).desired_width(180.0));
    });
    d.fields.insert(key.into(), v);
}

fn check(ui: &mut Ui, d: &mut Dialog, label: &str, key: &str, default: bool) -> bool {
    let mut v = d.get(key, if default { "1" } else { "0" }) == "1";
    ui.checkbox(&mut v, label);
    d.fields.insert(key.into(), if v { "1".into() } else { "0".into() });
    v
}

fn num(d: &mut Dialog, key: &str, default: f64) -> f64 {
    d.get(key, "").trim().trim_end_matches(['"', '%', ' ']).parse::<f64>().ok().filter(|v| v.is_finite()).unwrap_or(default)
}

/// Returns true when the dialog should close.
fn body(app: &mut SlideApp, ui: &mut Ui, d: &mut Dialog) -> bool {
    let lang = app.lang();
    let run = |app: &mut SlideApp, id: &str, p: Value| {
        let _ = app.run(id, p);
    };
    match d.id.as_str() {
        "table" => {
            field(ui, d, i18n::text(lang, "dialog.table.columns"), "cols", "5");
            field(ui, d, i18n::text(lang, "dialog.table.rows"), "rows", "2");
            let (ok, cancel) = buttons(ui, lang, i18n::text(lang, "tab.insert"));
            if ok {
                let (r, c) = (num(d, "rows", 2.0) as usize, num(d, "cols", 5.0) as usize);
                run(app, "insert.table", json!({"rows": r, "cols": c}));
            }
            ok || cancel
        }
        "slideSize" => {
            let cur = app.session.active().map(|s| s.doc.slide_size).unwrap_or(deckcraft_model::defaults::WIDE);
            ui.label(i18n::text(lang, "dialog.slide_size.preset"));
            let preset = d.get("preset", "");
            egui::ComboBox::from_id_salt("sizes")
                .selected_text(if preset.is_empty() { i18n::text(lang, "dialog.slide_size.custom") } else { i18n::slide_size_label(lang, &preset) })
                .show_ui(ui, |ui| {
                    for (label, w, h) in deckcraft_model::defaults::SLIDE_SIZES {
                        if ui.selectable_label(preset == *label, i18n::slide_size_label(lang, label)).clicked() {
                            d.fields.insert("preset".into(), label.to_string());
                            d.fields.insert("w".into(), format!("{:.2}", w / 72.0));
                            d.fields.insert("h".into(), format!("{:.2}", h / 72.0));
                        }
                    }
                });
            field(ui, d, i18n::text(lang, "dialog.slide_size.width"), "w", &format!("{:.2}", cur.width / 72.0));
            field(ui, d, i18n::text(lang, "dialog.slide_size.height"), "h", &format!("{:.2}", cur.height / 72.0));
            let maximize = check(ui, d, i18n::text(lang, "dialog.slide_size.maximize"), "max", false);
            let (ok, cancel) = buttons(ui, lang, "OK");
            if ok {
                let (w, h) = (num(d, "w", cur.width / 72.0) * 72.0, num(d, "h", cur.height / 72.0) * 72.0);
                run(app, "design.slideSize", json!({"w": w, "h": h, "scale": if maximize { "maximize" } else { "ensureFit" }}));
            }
            ok || cancel
        }
        "headerFooter" => {
            let hf = app.session.active().map(|s| s.doc.header_footer.clone()).unwrap_or_default();
            let date = check(ui, d, i18n::text(lang, "dialog.header_footer.date"), "date", hf.date);
            field(ui, d, i18n::text(lang, "dialog.header_footer.auto_date"), "dateText", &hf.date_text);
            let num_on = check(ui, d, i18n::text(lang, "dialog.header_footer.number"), "num", hf.slide_number);
            let foot = check(ui, d, i18n::text(lang, "dialog.header_footer.footer"), "footer", hf.footer);
            field(ui, d, i18n::text(lang, "dialog.header_footer.footer_text"), "footerText", &hf.footer_text);
            let hide = check(ui, d, i18n::text(lang, "dialog.header_footer.hide_title"), "hide", hf.hide_on_title);
            ui.add_space(6.0);
            let mut apply_all = false;
            let mut apply = false;
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                apply_all = ui.button(i18n::text(lang, "action.apply_all")).clicked();
                apply = ui.button(i18n::text(lang, "action.apply")).clicked();
                if ui.button(i18n::text(lang, "pref.cancel")).clicked() {
                    d.fields.insert("cancel".into(), "1".into());
                }
            });
            if apply || apply_all {
                let dt = d.get("dateText", "");
                let ft = d.get("footerText", "");
                run(
                    app,
                    "design.headerFooter",
                    json!({"date": date, "dateText": dt, "slideNumber": num_on, "footer": foot, "footerText": ft, "hideOnTitle": hide, "all": apply_all}),
                );
            }
            apply || apply_all || d.fields.contains_key("cancel")
        }
        "hyperlink" => {
            let mode = d.get("mode", "url");
            ui.horizontal(|ui| {
                if ui.selectable_label(mode == "url", i18n::text(lang, "dialog.hyperlink.web")).clicked() {
                    d.fields.insert("mode".into(), "url".into());
                }
                if ui.selectable_label(mode == "slide", i18n::text(lang, "dialog.hyperlink.document")).clicked() {
                    d.fields.insert("mode".into(), "slide".into());
                }
                if ui.selectable_label(mode == "email", i18n::text(lang, "dialog.hyperlink.email")).clicked() {
                    d.fields.insert("mode".into(), "email".into());
                }
            });
            match mode.as_str() {
                "slide" => {
                    let titles: Vec<String> = app
                        .session
                        .active()
                        .map(|s| s.doc.slides.iter().enumerate().map(|(i, sl)| format!("{}. {}", i + 1, sl.title())).collect())
                        .unwrap_or_default();
                    let sel = d.get("slide", "0").parse::<usize>().unwrap_or(0);
                    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                        for (i, t) in titles.iter().enumerate() {
                            if ui.selectable_label(sel == i, t).clicked() {
                                d.fields.insert("slide".into(), i.to_string());
                            }
                        }
                    });
                }
                "email" => field(ui, d, i18n::text(lang, "dialog.hyperlink.email_field"), "email", ""),
                _ => field(ui, d, i18n::text(lang, "dialog.hyperlink.address"), "url", "https://"),
            }
            field(ui, d, i18n::text(lang, "dialog.hyperlink.tip"), "tip", "");
            let (ok, cancel) = buttons(ui, lang, "OK");
            if ok {
                let tip = d.get("tip", "");
                let p = match mode.as_str() {
                    "slide" => json!({"slide": d.get("slide", "0").parse::<usize>().unwrap_or(0), "tooltip": tip}),
                    "email" => json!({"url": format!("mailto:{}", d.get("email", "")), "tooltip": tip}),
                    _ => json!({"url": d.get("url", ""), "tooltip": tip}),
                };
                run(app, "insert.hyperlink", p);
            }
            ok || cancel
        }
        "setupShow" => {
            let sh = app.session.active().map(|s| s.doc.show.clone()).unwrap_or_default();
            let ty = d.get("type", &sh.show_type);
            ui.label(i18n::text(lang, "dialog.setup_show.type"));
            for (l, v) in [
                (i18n::text(lang, "dialog.setup_show.speaker"), "speaker"),
                (i18n::text(lang, "dialog.setup_show.browsed"), "browsed"),
                (i18n::text(lang, "dialog.setup_show.kiosk"), "kiosk"),
            ] {
                if ui.radio(ty == v, l).clicked() {
                    d.fields.insert("type".into(), v.into());
                }
            }
            let lp = check(ui, d, i18n::text(lang, "dialog.setup_show.loop"), "loop", sh.loop_until_esc);
            let nn = check(ui, d, i18n::text(lang, "dialog.setup_show.narration"), "nn", sh.without_narration);
            let na = check(ui, d, i18n::text(lang, "dialog.setup_show.animation"), "na", sh.without_animation);
            let ut = check(ui, d, i18n::text(lang, "dialog.setup_show.timings"), "ut", sh.use_timings);
            let (ok, cancel) = buttons(ui, lang, "OK");
            if ok {
                let ty = d.get("type", "speaker");
                run(app, "show.setup", json!({"type": ty, "loop": lp, "noNarration": nn, "noAnimation": na, "useTimings": ut}));
            }
            ok || cancel
        }
        "quit" => {
            let names: Vec<String> = app.session.documents().iter().filter(|d| d.is_dirty()).map(|d| d.title()).collect();
            if names.is_empty() {
                app.quit_confirmed = true;
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                return true;
            }
            let what = if names.len() == 1 {
                format!("“{}”", names[0])
            } else {
                i18n::fill(i18n::text(lang, "dialog.save.many"), &[("n", &names.len().to_string())])
            };
            ui.label(egui::RichText::new(i18n::fill(i18n::text(lang, "dialog.save.question"), &[("what", &what)])).strong());
            ui.label(i18n::text(lang, "dialog.save.warning"));
            ui.add_space(8.0);
            let mut done = false;
            ui.horizontal(|ui| {
                let t = Tokens::get(ui.ctx());
                if ui.add(egui::Button::new(i18n::text(lang, "action.dont_save")).min_size(vec2(84.0, 24.0))).clicked() {
                    let _ = app.session.execute("file.recovery.discard", &json!({}));
                    app.quit_confirmed = true;
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    done = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let save = ui.add(
                        egui::Button::new(egui::RichText::new(i18n::text(lang, "nav.save")).color(t.accent_text))
                            .fill(t.accent)
                            .min_size(vec2(72.0, 24.0)),
                    );
                    if save.clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if app.save_all() {
                            app.quit_confirmed = true;
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        done = true;
                    }
                    if ui.add(egui::Button::new(i18n::text(lang, "pref.cancel")).min_size(vec2(72.0, 24.0))).clicked() {
                        done = true;
                    }
                });
            });
            done
        }
        "close" => {
            // File › Close and Ctrl+W on a presentation with unsaved changes.
            let Some(name) = app.session.active().map(|d| d.title()) else { return true };
            ui.label(egui::RichText::new(i18n::fill(i18n::text(lang, "dialog.save.close_question"), &[("name", &name)])).strong());
            ui.label(i18n::text(lang, "dialog.save.warning"));
            ui.add_space(8.0);
            let mut done = false;
            ui.horizontal(|ui| {
                let t = Tokens::get(ui.ctx());
                if ui.add(egui::Button::new(i18n::text(lang, "action.dont_save")).min_size(vec2(84.0, 24.0))).clicked() {
                    run(app, "file.close", json!({}));
                    done = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let save = ui.add(
                        egui::Button::new(egui::RichText::new(i18n::text(lang, "nav.save")).color(t.accent_text))
                            .fill(t.accent)
                            .min_size(vec2(72.0, 24.0)),
                    );
                    if save.clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        app.save();
                        // In the browser a save is a download: the presentation stays "unsaved".
                        let web = app.services.pick_save.is_none() && app.services.download.is_some();
                        if web || app.session.active().is_some_and(|d| !d.is_dirty()) {
                            run(app, "file.close", json!({}));
                        }
                        done = true;
                    }
                    if ui.add(egui::Button::new(i18n::text(lang, "pref.cancel")).min_size(vec2(72.0, 24.0))).clicked() {
                        done = true;
                    }
                });
            });
            done
        }
        "zoom" => {
            for pct in [400, 200, 150, 100, 75, 66, 50, 33] {
                if ui.radio(false, format!("{pct}%")).clicked() {
                    let _ = app.run("view.zoom", json!({"percent": pct}));
                }
            }
            if ui.button(i18n::text(lang, "dialog.zoom.fit")).clicked() {
                app.ui.zoom = None;
            }
            let (ok, cancel) = buttons(ui, lang, "OK");
            ok || cancel
        }
        "comment" => {
            let mut v = d.get("text", "");
            ui.add(egui::TextEdit::multiline(&mut v).hint_text(i18n::text(lang, "dialog.comment.hint")).desired_rows(4).desired_width(320.0));
            d.fields.insert("text".into(), v.clone());
            let (ok, cancel) = buttons(ui, lang, i18n::text(lang, "action.post"));
            if ok && !v.trim().is_empty() {
                run(app, "comment.add", json!({"text": v}));
                app.ui.pane = Some("comments".into());
            }
            ok || cancel
        }
        "altText" => {
            let cur = app.session.active().and_then(|s| s.selected_shapes().first().map(|x| x.descr.clone())).unwrap_or_default();
            ui.label(i18n::text(lang, "dialog.alt_text.description"));
            let mut v = d.get("text", &cur);
            ui.add(egui::TextEdit::multiline(&mut v).desired_rows(4).desired_width(340.0));
            d.fields.insert("text".into(), v.clone());
            let deco = check(ui, d, "Mark as decorative", "deco", false);
            let (ok, cancel) = buttons(ui, lang, "OK");
            if ok {
                run(app, "shape.altText", json!({"text": v, "decorative": deco}));
            }
            ok || cancel
        }
        "outline" => {
            ui.label(i18n::text(lang, "dialog.outline.description"));
            let mut v = d.get("text", "");
            ui.add(egui::TextEdit::multiline(&mut v).desired_rows(10).desired_width(380.0).code_editor());
            d.fields.insert("text".into(), v.clone());
            let (ok, cancel) = buttons(ui, lang, i18n::text(lang, "tab.insert"));
            if ok {
                run(app, "slide.fromOutline", json!({"text": v}));
            }
            ok || cancel
        }
        "renameSection" | "renameShape" | "renameLayout" => {
            let cur = d.params.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            field(ui, d, i18n::text(lang, "dialog.rename.name"), "name", &cur);
            let (ok, cancel) = buttons(ui, lang, i18n::text(lang, "dialog.rename.title"));
            if ok {
                let name = d.get("name", "");
                match d.id.as_str() {
                    "renameSection" => run(app, "section.rename", json!({"index": d.params.get("index").cloned().unwrap_or_default(), "name": name})),
                    "renameShape" => run(app, "shape.rename", json!({"id": d.params.get("id").cloned().unwrap_or_default(), "name": name})),
                    _ => run(app, "master.renameLayout", json!({"name": name})),
                }
            }
            ok || cancel
        }
        "paragraph" => {
            field(ui, d, i18n::text(lang, "dialog.paragraph.left"), "left", "0");
            field(ui, d, i18n::text(lang, "dialog.paragraph.hanging"), "hang", "0");
            field(ui, d, i18n::text(lang, "dialog.paragraph.before"), "before", "0");
            field(ui, d, i18n::text(lang, "dialog.paragraph.after"), "after", "0");
            field(ui, d, i18n::text(lang, "dialog.paragraph.lines"), "lines", "1.0");
            let (ok, cancel) = buttons(ui, lang, "OK");
            if ok {
                run(
                    app,
                    "format.paragraph",
                    json!({"indentLeft": num(d, "left", 0.0) * 72.0, "indentFirst": -num(d, "hang", 0.0) * 72.0, "spaceBefore": num(d, "before", 0.0), "spaceAfter": num(d, "after", 0.0), "lineSpacing": num(d, "lines", 1.0)}),
                );
            }
            ok || cancel
        }
        "export" => {
            ui.label(i18n::text(lang, "dialog.export.description"));
            let fmt = d.get("fmt", "pdf");
            for (l, f) in [
                (i18n::text(lang, "dialog.export.pdf"), "pdf"),
                (i18n::text(lang, "dialog.export.png"), "png"),
                (i18n::text(lang, "dialog.export.png_all"), "pngAll"),
                (i18n::text(lang, "dialog.export.jpeg"), "jpeg"),
                (i18n::text(lang, "dialog.export.pptx"), "pptx"),
                (i18n::text(lang, "dialog.export.outline"), "outline"),
            ] {
                if ui.radio(fmt == f, l).clicked() {
                    d.fields.insert("fmt".into(), f.into());
                }
            }
            let pdf_layout = d.get("pdfLayout", "slides");
            if fmt == "pdf" {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(i18n::text(lang, "dialog.export.layout"));
                    egui::ComboBox::from_id_salt("pdf_layout")
                        .selected_text(match pdf_layout.as_str() {
                            "notes" => i18n::text(lang, "dialog.export.notes"),
                            "h1" => i18n::text(lang, "dialog.export.handout_1"),
                            "h2" => i18n::text(lang, "dialog.export.handout_2"),
                            "h3" => i18n::text(lang, "dialog.export.handout_3"),
                            "h4" => i18n::text(lang, "dialog.export.handout_4"),
                            "h6" => i18n::text(lang, "dialog.export.handout_6"),
                            "h9" => i18n::text(lang, "dialog.export.handout_9"),
                            _ => i18n::text(lang, "dialog.export.full_page"),
                        })
                        .show_ui(ui, |ui| {
                            for (k, l) in [
                                ("slides", i18n::text(lang, "dialog.export.full_page")),
                                ("notes", i18n::text(lang, "dialog.export.notes")),
                                ("h1", i18n::text(lang, "dialog.export.handout_1")),
                                ("h2", i18n::text(lang, "dialog.export.handout_2")),
                                ("h3", i18n::text(lang, "dialog.export.handout_3")),
                                ("h4", i18n::text(lang, "dialog.export.handout_4")),
                                ("h6", i18n::text(lang, "dialog.export.handout_6")),
                                ("h9", i18n::text(lang, "dialog.export.handout_9")),
                            ] {
                                if ui.selectable_label(pdf_layout == k, l).clicked() {
                                    d.fields.insert("pdfLayout".into(), k.into());
                                }
                            }
                        });
                });
                let mut hidden = d.get("pdfHidden", "false") == "true";
                if ui.checkbox(&mut hidden, i18n::text(lang, "dialog.export.hidden")).changed() {
                    d.fields.insert("pdfHidden".into(), hidden.to_string());
                }
            }
            let pdf_params = {
                let (layout, per) = match pdf_layout.as_str() {
                    "notes" => ("notes", 0),
                    l if l.starts_with('h') => ("handouts", l[1..].parse().unwrap_or(6)),
                    _ => ("slides", 0),
                };
                json!({"layout": layout, "perPage": per, "includeHidden": d.get("pdfHidden", "false") == "true"})
            };
            let (ok, cancel) = buttons(ui, lang, i18n::text(lang, "nav.export"));
            if ok {
                let extra = if fmt == "pdf" { pdf_params } else { json!({}) };
                app.export_as(&fmt, extra);
            }
            ok || cancel
        }
        "symbol" => {
            ui.label(i18n::text(lang, "dialog.symbol.description"));
            let groups: [(&str, &str); 6] = [
                (i18n::text(lang, "dialog.symbol.punctuation"), "–—‘’“”…•·§¶†‡‰′″‹›«»¡¿"),
                (i18n::text(lang, "dialog.symbol.currency"), "$€£¥¢₹₩₽₺₿"),
                (i18n::text(lang, "dialog.symbol.math"), "±×÷≠≈≤≥∞√∑∏∫∂∆∇∈∉∩∪⊂⊃∀∃¬∧∨⇒⇔°′"),
                (i18n::text(lang, "dialog.symbol.greek"), "αβγδεζηθικλμνξοπρστυφχψωΓΔΘΛΞΠΣΦΨΩ"),
                (i18n::text(lang, "dialog.symbol.arrows"), "←↑→↓↔↕⇐⇑⇒⇓↖↗↘↙⟵⟶"),
                (i18n::text(lang, "dialog.symbol.shapes"), "■□▪▫▲△▶▷▼▽◀◁◆◇○●◎★☆✓✔✗✘♠♣♥♦"),
            ];
            for (g, chars) in groups {
                ui.label(egui::RichText::new(g).font(theme::bold(12.0)));
                ui.horizontal_wrapped(|ui| {
                    for c in chars.chars() {
                        if ui.add(egui::Button::new(egui::RichText::new(c.to_string()).size(16.0)).min_size(vec2(26.0, 26.0))).clicked() {
                            if app.session.active().is_some_and(|s| s.selection.text.is_none()) {
                                let _ = app.run("text.edit", json!({}));
                            }
                            let _ = app.run("insert.symbol", json!({"text": c.to_string()}));
                        }
                    }
                });
            }
            let (ok, cancel) = buttons(ui, lang, i18n::text(lang, "nav.close"));
            ok || cancel
        }
        "equation" => {
            ui.label(i18n::text(lang, "dialog.equation.description"));
            let mut v = d.get("eq", "");
            ui.add(egui::TextEdit::singleline(&mut v).desired_width(340.0).font(theme::font(15.0)));
            d.fields.insert("eq".into(), v.clone());
            let (ok, cancel) = buttons(ui, lang, i18n::text(lang, "tab.insert"));
            if ok && !v.is_empty() {
                let pretty = pretty_equation(&v);
                let size = app.session.active().map(|s| s.doc.slide_size).unwrap_or(deckcraft_model::defaults::WIDE);
                run(app, "insert.textBox", json!({"rect": [size.width / 2.0 - 150.0, size.height / 2.0 - 25.0, 300, 50], "text": pretty}));
                run(app, "text.exit", json!({}));
                run(app, "format.font", json!({"family": "Liberation Serif"}));
                run(app, "format.italic", json!({"on": true}));
                run(app, "format.size", json!({"size": 28}));
            }
            ok || cancel
        }
        "spelling" => {
            let v = app.session.execute("review.spelling", &json!({})).unwrap_or_default();
            let list = v.as_array().cloned().unwrap_or_default();
            if list.is_empty() {
                ui.label(i18n::text(lang, "dialog.review.spelling_complete"));
            }
            for item in &list {
                ui.horizontal(|ui| {
                    let w = item.get("word").and_then(Value::as_str).unwrap_or("");
                    ui.label(i18n::fill(
                        i18n::text(lang, "dialog.review.word"),
                        &[("n", &(item.get("slide").and_then(Value::as_u64).unwrap_or(0) + 1).to_string()), ("word", w)],
                    ));
                    if ui.small_button(i18n::text(lang, "action.go_to")).clicked() {
                        let _ = app.run("slide.go", json!({"index": item.get("slide").cloned().unwrap_or_default()}));
                    }
                });
            }
            let (ok, cancel) = buttons(ui, lang, i18n::text(lang, "nav.close"));
            ok || cancel
        }
        "accessibility" => {
            let v = app.session.execute("review.accessibility", &json!({})).unwrap_or_default();
            let list = v.as_array().cloned().unwrap_or_default();
            if list.is_empty() {
                ui.label(i18n::text(lang, "dialog.review.accessibility_complete"));
            }
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for item in &list {
                    ui.horizontal(|ui| {
                        ui.label(i18n::fill(
                            i18n::text(lang, "dialog.review.issue"),
                            &[
                                ("n", &(item.get("slide").and_then(Value::as_u64).unwrap_or(0) + 1).to_string()),
                                ("issue", item.get("issue").and_then(Value::as_str).unwrap_or("")),
                                ("name", &item.get("name").and_then(Value::as_str).map(|n| format!(" — {n}")).unwrap_or_default()),
                            ],
                        ));
                        if ui.small_button(i18n::text(lang, "action.go_to")).clicked() {
                            let _ = app.run("slide.go", json!({"index": item.get("slide").cloned().unwrap_or_default()}));
                            if let Some(id) = item.get("shape") {
                                let _ = app.run("edit.select", json!({"ids": [id]}));
                            }
                        }
                    });
                }
            });
            let (ok, cancel) = buttons(ui, lang, i18n::text(lang, "nav.close"));
            ok || cancel
        }
        "language" => {
            ui.label(i18n::text(lang, "dialog.language.description"));
            for l in [
                "English (United States)",
                "English (United Kingdom)",
                "Español",
                "Français",
                "Deutsch",
                "Italiano",
                "Português",
                "日本語",
                "中文",
                "العربية",
            ] {
                let _ = ui.selectable_label(l.starts_with("English (United States)"), l);
            }
            let (ok, cancel) = buttons(ui, lang, "OK");
            ok || cancel
        }
        "customShow" => {
            let shows = app.session.active().map(|s| s.doc.custom_shows.clone()).unwrap_or_default();
            for s in &shows {
                ui.horizontal(|ui| {
                    ui.label(i18n::fill(i18n::text(lang, "dialog.custom_show.summary"), &[("name", &s.name), ("n", &s.slides.len().to_string())]));
                    if ui.small_button(i18n::text(lang, "action.delete")).clicked() {
                        let _ = app.run("show.customShow", json!({"name": s.name, "delete": true}));
                    }
                });
            }
            field(ui, d, i18n::text(lang, "dialog.custom_show.name"), "name", "Custom Show 1");
            field(ui, d, i18n::text(lang, "dialog.custom_show.slides"), "slides", "1");
            let (ok, cancel) = buttons(ui, lang, i18n::text(lang, "action.create"));
            if ok {
                let idx: Vec<usize> =
                    d.get("slides", "").split(',').filter_map(|s| s.trim().parse::<usize>().ok()).filter(|v| *v > 0).map(|v| v - 1).collect();
                run(app, "show.customShow", json!({"name": d.get("name", "Custom Show"), "slides": idx}));
            }
            ok || cancel
        }
        "chartData" => {
            let ch = app.session.active().and_then(|s| {
                s.selected_shapes()
                    .into_iter()
                    .find_map(|x| if let deckcraft_model::ShapeKind::Chart(c) = &x.kind { Some(((**c).clone(), x.id)) } else { None })
            });
            let Some((c, id)) = ch else {
                ui.label(i18n::text(lang, "dialog.chart.select"));
                let (ok, cancel) = buttons(ui, lang, i18n::text(lang, "nav.close"));
                return ok || cancel;
            };
            ui.label(i18n::text(lang, "dialog.chart.description"));
            let mut grid: Vec<Vec<String>> = d.params.get("grid").and_then(|g| serde_json::from_value(g.clone()).ok()).unwrap_or_else(|| {
                let mut g = vec![std::iter::once(String::new()).chain(c.series.iter().map(|s| s.name.clone())).collect::<Vec<_>>()];
                for (i, cat) in c.categories.iter().enumerate() {
                    let mut row = vec![cat.clone()];
                    for s in &c.series {
                        row.push(s.values.get(i).copied().flatten().map(|v| v.to_string()).unwrap_or_default());
                    }
                    g.push(row);
                }
                g
            });
            egui::Grid::new("chartdata").striped(true).show(ui, |ui| {
                for row in grid.iter_mut() {
                    for cell in row.iter_mut() {
                        ui.add(egui::TextEdit::singleline(cell).desired_width(70.0));
                    }
                    ui.end_row();
                }
            });
            ui.horizontal(|ui| {
                if ui.button(i18n::text(lang, "dialog.chart.add_row")).clicked() {
                    let n = grid.first().map(|r| r.len()).unwrap_or(2);
                    grid.push(vec![String::new(); n]);
                }
                if ui.button(i18n::text(lang, "dialog.chart.add_series")).clicked() {
                    for r in grid.iter_mut() {
                        r.push(String::new());
                    }
                }
            });
            d.params = json!({"grid": grid});
            let (ok, cancel) = buttons(ui, lang, "OK");
            if ok {
                let names: Vec<String> = grid.first().map(|r| r.iter().skip(1).cloned().collect()).unwrap_or_default();
                let cats: Vec<String> = grid.iter().skip(1).map(|r| r.first().cloned().unwrap_or_default()).collect();
                let series: Vec<Value> = names.iter().enumerate().map(|(k, n)| json!({"name": n, "values": grid.iter().skip(1).map(|r| r.get(k + 1).and_then(|v| v.trim().parse::<f64>().ok())).collect::<Vec<_>>()})).collect();
                run(app, "chart.data", json!({"id": id.0, "categories": cats, "series": series}));
            }
            ok || cancel
        }
        "trim" => {
            field(ui, d, i18n::text(lang, "dialog.trim.start"), "start", "0");
            field(ui, d, i18n::text(lang, "dialog.trim.end"), "end", "0");
            field(ui, d, i18n::text(lang, "dialog.trim.fade_in"), "fin", "0");
            field(ui, d, i18n::text(lang, "dialog.trim.fade_out"), "fout", "0");
            let (ok, cancel) = buttons(ui, lang, "OK");
            if ok {
                run(
                    app,
                    "media.options",
                    json!({"trimStart": (num(d, "start", 0.0) * 1000.0) as u64, "trimEnd": (num(d, "end", 0.0) * 1000.0) as u64, "fadeIn": (num(d, "fin", 0.0) * 1000.0) as u64, "fadeOut": (num(d, "fout", 0.0) * 1000.0) as u64}),
                );
            }
            ok || cancel
        }
        "smartart" => {
            ui.label(i18n::text(lang, "dialog.smartart.description"));
            let kind = d.get("kind", "process");
            ui.horizontal_wrapped(|ui| {
                for (k, l) in [
                    ("list", i18n::text(lang, "dialog.smartart.list")),
                    ("process", i18n::text(lang, "dialog.smartart.process")),
                    ("cycle", i18n::text(lang, "dialog.smartart.cycle")),
                    ("hierarchy", i18n::text(lang, "dialog.smartart.hierarchy")),
                    ("pyramid", i18n::text(lang, "dialog.smartart.pyramid")),
                    ("matrix", i18n::text(lang, "dialog.smartart.matrix")),
                ] {
                    if ui.selectable_label(kind == k, l).clicked() {
                        d.fields.insert("kind".into(), k.into());
                    }
                }
            });
            let mut v = d.get("text", "Plan\nBuild\nLaunch");
            ui.add(egui::TextEdit::multiline(&mut v).desired_rows(5).desired_width(320.0));
            d.fields.insert("text".into(), v.clone());
            let (ok, cancel) = buttons(ui, lang, i18n::text(lang, "tab.insert"));
            if ok {
                run(app, "insert.smartArt", json!({"kind": kind, "items": v.lines().filter(|l| !l.trim().is_empty()).collect::<Vec<_>>()}));
            }
            ok || cancel
        }
        "about" => {
            let tab = d.get("tab", "about");
            ui.horizontal(|ui| {
                for (k, l) in [
                    ("about", i18n::text(lang, "dialog.about.tab")),
                    ("contributors", i18n::text(lang, "dialog.about.contributors")),
                    ("models", i18n::text(lang, "dialog.about.models")),
                ] {
                    if ui.selectable_label(tab == k, l).clicked() {
                        d.fields.insert("tab".into(), k.into());
                    }
                }
            });
            ui.separator();
            match tab.as_str() {
                "contributors" | "models" => {
                    // The credits need room for the table; the About tab keeps its compact size.
                    ui.set_min_size(vec2(640.0, 380.0));
                    ui.set_max_height(380.0);
                    if tab == "models" { crate::credits::models_ui(ui) } else { crate::credits::contributors_ui(ui) }
                }
                _ => about(app, ui),
            }
            let (ok, _) = buttons(ui, lang, i18n::text(lang, "nav.close"));
            ok
        }
        "preferences" => {
            let lang = app.lang();
            ui.label(egui::RichText::new(crate::i18n::text(lang, "pref.general")).font(theme::bold(13.0)));
            ui.horizontal(|ui| {
                ui.label(crate::i18n::text(lang, "pref.language"));
                let current = app.ui.language;
                egui::ComboBox::from_id_salt("interface_language").selected_text(crate::i18n::choice_label(lang, current)).show_ui(ui, |ui| {
                    for choice in crate::i18n::LanguageChoice::ALL {
                        if ui.selectable_label(current == choice, crate::i18n::choice_label(lang, choice)).clicked() {
                            let _ = app.run("app.language", json!({"language": choice.code()}));
                        }
                    }
                });
            });
            let mut dark = app.ui.brightness == theme::Brightness::Dark;
            if ui.checkbox(&mut dark, crate::i18n::text(lang, "pref.dark")).changed() {
                let _ = app.run("view.dark", json!({"on": dark}));
            }
            let mut scale = app.ui.ui_scale;
            if ui.add(egui::Slider::new(&mut scale, 0.75..=1.75).text(crate::i18n::text(lang, "pref.size"))).changed() {
                app.ui.ui_scale = scale;
            }
            ui.label(egui::RichText::new(crate::i18n::text(lang, "pref.editing")).font(theme::bold(13.0)));
            ui.checkbox(&mut app.session.prefs.smart_guides, crate::i18n::text(lang, "pref.smart_guides"));
            ui.checkbox(&mut app.session.prefs.snap_to_grid, crate::i18n::text(lang, "pref.snap"));
            ui.checkbox(&mut app.session.prefs.smart_quotes, crate::i18n::text(lang, "pref.quotes"));
            ui.checkbox(&mut app.session.prefs.autocorrect, crate::i18n::text(lang, "pref.autocorrect"));
            ui.horizontal(|ui| {
                ui.label(crate::i18n::text(lang, "pref.author"));
                ui.text_edit_singleline(&mut app.session.prefs.author);
            });
            let (ok, _) = buttons_labeled(ui, crate::i18n::text(lang, "pref.done"), crate::i18n::text(lang, "pref.cancel"));
            ok
        }
        _ => {
            ui.label(i18n::fill(i18n::text(lang, "dialog.unavailable"), &[("id", &d.id)]));
            let (ok, cancel) = buttons(ui, lang, "OK");
            ok || cancel
        }
    }
}

fn pretty_equation(s: &str) -> String {
    let sup = |c: char| match c {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        'n' => 'ⁿ',
        'i' => 'ⁱ',
        '+' => '⁺',
        '-' => '⁻',
        _ => c,
    };
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '^' => {
                while let Some(&n) = chars.peek() {
                    if n.is_ascii_alphanumeric() || n == '+' || n == '-' {
                        out.push(sup(n));
                        chars.next();
                        if !n.is_ascii_digit() {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
            '*' => out.push('·'),
            _ => out.push(c),
        }
    }
    out.replace("sqrt", "√").replace("<=", "≤").replace(">=", "≥").replace("!=", "≠").replace("pi", "π").replace("+-", "±")
}

pub fn about(app: &mut SlideApp, ui: &mut Ui) {
    let lang = app.lang();
    let t = Tokens::get(ui.ctx());
    ui.vertical_centered(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
        paint_logo(ui.painter(), r);
        ui.label(egui::RichText::new("DeckCraft").font(theme::bold(22.0)));
        ui.label(egui::RichText::new(version_line()).color(t.text_dim));
        ui.add_space(6.0);
        ui.label(i18n::text(lang, "dialog.about.description"));
        ui.label(i18n::text(lang, "dialog.about.project"));
        ui.add_space(6.0);
        ui.hyperlink_to(i18n::text(lang, "dialog.about.community"), deckcraft_engine::links::DISCORD);
        ui.hyperlink_to("getartcraft.com/apps/deckcraft", deckcraft_engine::links::APP_PAGE);
        ui.hyperlink_to(i18n::text(lang, "dialog.about.source"), deckcraft_engine::links::GITHUB);
        ui.add_space(6.0);
        ui.label(egui::RichText::new("MIT OR Apache-2.0 · © 2026 ArtCraft Team and the DeckCraft contributors").size(11.0).color(t.text_faint));
    });
}

/// "Version X.Y.Z" plus, for release builds, the short commit and build date. The version comes
/// from `[workspace.package] version` (the single source of truth, `cargo xtask version`); the
/// release workflow sets `DECKCRAFT_BUILD_SHA` / `DECKCRAFT_BUILD_DATE` at build time.
pub fn version_line() -> String {
    let mut s = format!("Version {}", env!("CARGO_PKG_VERSION"));
    let sha = option_env!("DECKCRAFT_BUILD_SHA").map(|s| s.get(..9).unwrap_or(s)).filter(|s| !s.is_empty());
    let date = option_env!("DECKCRAFT_BUILD_DATE").filter(|s| !s.is_empty());
    match (sha, date) {
        (Some(sha), Some(date)) => s.push_str(&format!(" ({sha}, {date})")),
        (Some(x), None) | (None, Some(x)) => s.push_str(&format!(" ({x})")),
        (None, None) => {}
    }
    s
}

/// DeckCraft's mark, drawn in code: a slide card with a play triangle in our orange.
pub fn paint_logo(p: &egui::Painter, r: Rect) {
    let orange = Color32::from_rgb(0xF2, 0x6B, 0x1D);
    let deep = Color32::from_rgb(0xC2, 0x4E, 0x14);
    let card = r.shrink(r.width() * 0.08);
    p.rect_filled(card.translate(vec2(r.width() * 0.04, r.width() * 0.05)), CornerRadius::same((r.width() * 0.14) as u8), deep);
    p.rect_filled(card, CornerRadius::same((r.width() * 0.14) as u8), orange);
    let c = card.center();
    let s = card.width() * 0.22;
    p.add(egui::Shape::convex_polygon(
        vec![pos2(c.x - s * 0.7, c.y - s), pos2(c.x + s, c.y), pos2(c.x - s * 0.7, c.y + s)],
        Color32::WHITE,
        Stroke::NONE,
    ));
}
