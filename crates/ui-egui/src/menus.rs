//! UI-level shortcuts, the command palette, and the menu structure (shared with the native macOS
//! menu bar in the desktop app).

use deckcraft_engine::Mods;
use serde_json::json;

use crate::SlideApp;
use crate::theme::{self, Tokens};

/// Shortcuts the UI handles itself (views, show, zoom, panes, dialogs, palette, file dialogs).
/// True when consumed. PowerPoint's Windows and Mac keys both work where they don't collide;
/// where they do, `session.mac_keys` picks the platform's (`docs/ui-parity.md`, Shortcut audit).
pub fn ui_shortcut(app: &mut SlideApp, ctx: &egui::Context, key: egui::Key, m: Mods) -> bool {
    use egui::Key::*;
    let mac = app.session.mac_keys;
    let (editing, selected) = app.session.active().map(|d| (d.selection.text.is_some(), !d.selection.shapes.is_empty())).unwrap_or_default();
    let dialog = |app: &mut SlideApp, id: &str| {
        app.dialog = Some(crate::dialogs::Dialog::new(id));
        true
    };
    let id = match (key, m.cmd, m.shift, m.alt) {
        // Presenter View: Alt+F5 (Windows), Option+Return (Mac; Return while editing is a new line).
        (F5, false, false, true) => {
            app.start_presenter();
            return true;
        }
        (Enter, false, false, true) if !editing => {
            app.start_presenter();
            return true;
        }
        (F5, _, false, _) => {
            app.start_show(0, false);
            return true;
        }
        // Shift+F5 (Windows) and Cmd+Return (Mac): from the current slide. On Windows, Ctrl+Enter
        // with text or an object selected is the next placeholder (`Session::key`).
        (F5, _, true, _) | (Enter, true, false, false) if key == F5 || mac || !(editing || selected) => {
            let i = app.session.active().map(|d| d.selection.slide).unwrap_or(0);
            app.start_show(i, false);
            return true;
        }
        (Enter, true, true, _) => {
            app.start_show(0, false);
            return true;
        }
        (S, true, false, false) => {
            app.save();
            return true;
        }
        (S, true, true, false) => "app.saveAsDialog",
        (O, true, false, false) => "app.openDialog",
        // Mac Cmd+Shift+O: recent presentations (the start screen lists them).
        (O, true, true, false) => "app.home",
        // Ctrl+Alt+O / Cmd+Option+O: fit the slide to the window.
        (O, true, false, true) => "view.fit",
        (P, true, true, false) => "app.palette",
        (Comma, true, false, false) => "app.preferences",
        // Ctrl+Plus zooms in; Ctrl+= is Subscript on Windows, and on a Mac Cmd+= zooms while
        // Cmd+Control+= is Subscript.
        (Plus, true, false, false) => "view.zoomIn",
        (Equals, true, false, false) if mac && !m.control => "view.zoomIn",
        (Minus, true, false, false) => "view.zoomOut",
        (Num0, true, false, false) => "view.fit",
        // Cmd+1..4 (Mac): Normal, Slide Sorter, Notes Page, Outline View; Cmd+Option+1 Slide Master.
        (Num1, true, false, false) => "view.normal",
        (Num2, true, false, false) => "view.sorter",
        (Num3, true, false, false) => "view.notesPage",
        (Num4, true, false, false) => "view.outline",
        (Num1, true, false, true) => "view.slideMaster",
        // Cmd+Shift+1 / 2 (Mac): Format pane for the selection / Format Background.
        (Num1, true, true, false) => {
            let _ = app.run("view.pane", json!({"pane": "format"}));
            return true;
        }
        (Num2, true, true, false) => {
            let _ = app.run("view.pane", json!({"pane": "background"}));
            return true;
        }
        // Alt+F10: the Selection pane.
        (F10, false, false, true) => {
            let _ = app.run("view.pane", json!({"pane": "selection"}));
            return true;
        }
        // Ctrl+Shift+Tab: switch between the thumbnails and the outline.
        (Tab, true, true, false) => {
            app.ui.view = if app.ui.view == crate::ViewMode::Outline { crate::ViewMode::Normal } else { crate::ViewMode::Outline };
            return true;
        }
        (F1, true, false, false) => "view.collapseRibbon",
        (F9, false, true, false) => "view.gridlines",
        (F9, false, false, true) => "view.guides",
        // Mac Cmd+Option+Control+G: guides (Cmd+Option+G alone groups).
        (G, true, false, true) if m.control => "view.guides",
        (K, true, false, false) => return dialog(app, "hyperlink"),
        // New Comment: Ctrl+Alt+M (Windows), Cmd+Shift+M (Mac). Cmd+Option+M on a Mac is the
        // Paragraph dialog.
        (M, true, false, true) if mac => return dialog(app, "paragraph"),
        (M, true, false, true) => return dialog(app, "comment"),
        (M, true, true, false) if mac => return dialog(app, "comment"),
        (F7, false, false, false) => return dialog(app, "spelling"),
        // Alt+= (Windows): Equation. On a Mac Option+= types ≠.
        (Equals, false, false, true) if !mac => return dialog(app, "equation"),
        // Ctrl+Q (Windows): quit, asking about unsaved changes. On a Mac the app menu has Cmd+Q.
        (Q, true, false, false) if !mac => {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return true;
        }
        // Find: there's no find-in-slides yet, so Cmd+F searches commands (Tell Me).
        (F, true, false, false) => {
            app.palette = Some((String::new(), 0));
            return true;
        }
        _ => return false,
    };
    let _ = app.run(id, json!({}));
    true
}

/// ⇧⌘P: search every command by name and run it.
pub fn palette(app: &mut SlideApp, ctx: &egui::Context) {
    let Some((mut query, mut sel)) = app.palette.take() else { return };
    let t = Tokens::get(ctx);
    let mut close = false;
    let mut run: Option<String> = None;
    egui::Window::new("command_palette")
        .title_bar(false)
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 90.0))
        .fixed_size(egui::vec2(520.0, 380.0))
        .show(ctx, |ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut query).hint_text("Search commands…").desired_width(f32::INFINITY).font(theme::font(15.0)));
            r.request_focus();
            let q = query.to_lowercase();
            let mut items: Vec<(String, String, Option<&'static str>)> = app
                .session
                .commands()
                .into_iter()
                .filter(|c| c.enabled)
                .map(|c| (c.id.to_string(), c.label.to_string(), c.shortcut))
                .chain(crate::UI_COMMANDS.iter().map(|c| (c.0.to_string(), c.1.to_string(), c.2)))
                .filter(|(id, label, _)| q.is_empty() || label.to_lowercase().contains(&q) || id.to_lowercase().contains(&q))
                .collect();
            items.sort_by_key(|(_, l, _)| !l.to_lowercase().starts_with(&q));
            items.truncate(60);
            let (down, up, enter, esc) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::ArrowDown),
                    i.key_pressed(egui::Key::ArrowUp),
                    i.key_pressed(egui::Key::Enter),
                    i.key_pressed(egui::Key::Escape),
                )
            });
            if down {
                sel = (sel + 1).min(items.len().saturating_sub(1));
            }
            if up {
                sel = sel.saturating_sub(1);
            }
            if esc {
                close = true;
            }
            if enter && let Some((id, ..)) = items.get(sel) {
                run = Some(id.clone());
            }
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                for (k, (id, label, sc)) in items.iter().enumerate() {
                    let r = ui.horizontal(|ui| {
                        let resp = ui.selectable_label(k == sel, label);
                        if let Some(s) = sc {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(egui::RichText::new(crate::ribbon::pretty_shortcut(s)).color(t.text_faint));
                            });
                        }
                        resp
                    });
                    if r.inner.clicked() {
                        run = Some(id.clone());
                    }
                }
            });
        });
    if let Some(id) = run {
        let _ = app.run(&id, json!({}));
        close = true;
    }
    if !close {
        app.palette = Some((query, sel));
    }
}

/// The application menu tree: (menu, [(label, command id, shortcut)]), "-" = separator. Used by the
/// native macOS menu bar and as the parity catalogue's live side.
pub fn menu_tree() -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
    vec![
        (
            "File",
            vec![
                ("New Presentation", "file.new"),
                ("Open…", "app.openDialog"),
                ("-", ""),
                ("Close", "file.close"),
                ("Save", "file.save"),
                ("Save As…", "app.saveAsDialog"),
                ("Export…", "app.exportDialog"),
                ("-", ""),
                ("Properties", "file.properties"),
            ],
        ),
        (
            "Edit",
            vec![
                ("Undo", "edit.undo"),
                ("Redo", "edit.redo"),
                ("-", ""),
                ("Cut", "edit.cut"),
                ("Copy", "edit.copy"),
                ("Paste", "edit.paste"),
                ("Paste and Match Formatting", "edit.pasteText"),
                ("-", ""),
                ("Select All", "edit.selectAll"),
                ("Duplicate", "edit.duplicate"),
                ("Delete Slide", "slide.delete"),
                ("-", ""),
                ("Find…", "app.palette"),
            ],
        ),
        (
            "View",
            vec![
                ("Normal", "view.normal"),
                ("Slide Sorter", "view.sorter"),
                ("Notes Page", "view.notesPage"),
                ("Outline View", "view.outline"),
                ("Reading View", "view.reading"),
                ("Slide Show", "show.start"),
                ("-", ""),
                ("Slide Master", "view.slideMaster"),
                ("-", ""),
                ("Ruler", "view.ruler"),
                ("Gridlines", "view.gridlines"),
                ("Guides", "view.guides"),
                ("-", ""),
                ("Zoom In", "view.zoomIn"),
                ("Zoom Out", "view.zoomOut"),
                ("Fit to Window", "view.fit"),
            ],
        ),
        (
            "Insert",
            vec![
                ("New Slide", "slide.new"),
                ("Duplicate Slide", "slide.duplicate"),
                ("-", ""),
                ("Section", "section.add"),
                ("Text Box", "insert.textBox"),
                ("WordArt", "insert.wordArt"),
                ("Date and Time", "insert.dateTime"),
                ("Slide Number", "insert.slideNumber"),
                ("-", ""),
                ("Table…", "insert.table"),
                ("Chart", "insert.chart"),
                ("Picture from File…", "app.insertPictureDialog"),
                ("Audio from File…", "app.insertAudioDialog"),
                ("Video from File…", "app.insertVideoDialog"),
            ],
        ),
        (
            "Format",
            vec![
                ("Bold", "format.bold"),
                ("Italic", "format.italic"),
                ("Underline", "format.underline"),
                ("-", ""),
                ("Align Left", "format.alignLeft"),
                ("Center", "format.alignCenter"),
                ("Align Right", "format.alignRight"),
                ("Justify", "format.justify"),
                ("-", ""),
                ("Right-to-Left Text Direction", "format.rtl"),
            ],
        ),
        (
            "Arrange",
            vec![
                ("Bring to Front", "arrange.bringToFront"),
                ("Send to Back", "arrange.sendToBack"),
                ("Bring Forward", "arrange.bringForward"),
                ("Send Backward", "arrange.sendBackward"),
                ("-", ""),
                ("Group", "arrange.group"),
                ("Ungroup", "arrange.ungroup"),
                ("Regroup", "arrange.regroup"),
                ("-", ""),
                ("Rotate Left 90°", "arrange.rotateLeft"),
                ("Rotate Right 90°", "arrange.rotateRight"),
                ("Flip Horizontal", "arrange.flipHorizontal"),
                ("Flip Vertical", "arrange.flipVertical"),
            ],
        ),
        (
            "Slide Show",
            vec![
                ("Play from Start", "show.fromStart"),
                ("Play from Current Slide", "show.fromCurrent"),
                ("-", ""),
                ("Hide Slide", "slide.hide"),
                ("Set Up Show…", "show.setup"),
            ],
        ),
        ("Help", vec![("About DeckCraft", "app.about"), ("Command Palette", "app.palette")]),
    ]
}
