//! macOS: the native menu bar (DeckCraft, File, Edit, View, Insert, Format, Arrange, Tools,
//! Slide Show, Window, Help) matching Microsoft PowerPoint's top-level menu hierarchy on the Mac.
//! Items dispatch to SlideApp commands, dialogs, or system actions.

use std::str::FromStr;
use std::sync::mpsc::{Receiver, channel};

use deckcraft_ui_egui::{SlideApp, ViewMode, dialogs::Dialog};
use muda::accelerator::Accelerator;
use muda::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use serde_json::json;

pub struct NativeMenu {
    _menu: Menu,
    events: Receiver<MenuEvent>,
    view_items: Vec<(&'static str, CheckMenuItem)>,
}

fn item(id: &'static str, label: &'static str, acc: Option<&'static str>) -> MenuItem {
    let accel = acc.and_then(|a| Accelerator::from_str(a).ok());
    MenuItem::with_id(id, label, true, accel)
}

fn build_app_menu() -> Submenu {
    let app_menu = Submenu::new("DeckCraft", true);
    let _ = app_menu.append_items(&[
        &item("app.about", "About DeckCraft", None),
        &PredefinedMenuItem::separator(),
        &item("app.preferences", "Preferences…", Some("CMD+,")),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::services(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::hide(Some("Hide DeckCraft")),
        &PredefinedMenuItem::hide_others(Some("Hide Others")),
        &PredefinedMenuItem::show_all(Some("Show All")),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::quit(Some("Quit DeckCraft")),
    ]);
    app_menu
}

fn build_file_menu() -> Submenu {
    let file = Submenu::new("File", true);
    let _ = file.append_items(&[
        &item("file.new", "New Presentation", Some("CMD+N")),
        &item("file.newTemplate", "New from Template…", Some("CMD+SHIFT+P")),
        &item("app.openDialog", "Open…", Some("CMD+O")),
    ]);

    let recent = Submenu::new("Open Recent", true);
    let _ = recent.append(&item("file.clearRecent", "Clear Recent Files", None));
    let _ = file.append(&recent);

    let _ = file.append(&PredefinedMenuItem::separator());
    let _ = file.append_items(&[
        &item("app.closePresentation", "Close", Some("CMD+W")),
        &item("file.save", "Save", Some("CMD+S")),
        &item("app.saveAsDialog", "Save As…", Some("CMD+SHIFT+S")),
        &item("file.saveTemplate", "Save as Template…", None),
        &item("app.exportDialog", "Export…", None),
    ]);

    let share = Submenu::new("Share", true);
    let _ = share.append_items(&[
        &item("share.invite", "Invite People…", None),
        &item("share.copyLink", "Copy Link", None),
        &item("share.sendCopy", "Send a Copy", None),
    ]);
    let _ = file.append(&share);

    let _ = file.append(&PredefinedMenuItem::separator());
    let _ = file.append(&item("file.readOnly", "Always Open Read-Only", None));

    let perms = Submenu::new("Restrict Permissions", true);
    let _ = perms.append_items(&[&item("perm.unrestricted", "Unrestricted Access", None), &item("perm.restricted", "Restricted Access…", None)]);
    let _ = file.append(&perms);
    let _ = file.append(&item("file.passwords", "Passwords…", None));

    let _ = file.append(&PredefinedMenuItem::separator());
    let _ = file.append_items(&[
        &item("dialog:slideSize", "Page Setup…", None),
        &item("app.print", "Print…", Some("CMD+P")),
        &PredefinedMenuItem::separator(),
        &item("file.properties", "Properties", None),
    ]);

    file
}

fn build_edit_menu() -> Submenu {
    let edit = Submenu::new("Edit", true);
    let _ = edit.append_items(&[
        &item("edit.undo", "Undo", Some("CMD+Z")),
        &item("edit.redo", "Repeat", Some("CMD+Y")),
        &PredefinedMenuItem::separator(),
        &item("edit.cut", "Cut", Some("CMD+X")),
        &item("edit.copy", "Copy", Some("CMD+C")),
        &item("edit.paste", "Paste", Some("CMD+V")),
        &item("edit.pasteSpecial", "Paste Special…", Some("CTRL+CMD+V")),
        &item("edit.pasteText", "Paste and Match Formatting", Some("ALT+SHIFT+CMD+V")),
        &PredefinedMenuItem::separator(),
        &item("edit.clear", "Clear", None),
        &item("edit.selectAll", "Select All", Some("CMD+A")),
        &item("edit.duplicate", "Duplicate", Some("CMD+D")),
        &item("slide.delete", "Delete Slide", None),
        &PredefinedMenuItem::separator(),
        &item("section.delete", "Remove Section", None),
        &item("dialog:renameSection", "Rename Section", None),
        &PredefinedMenuItem::separator(),
    ]);

    let find = Submenu::new("Find", true);
    let _ = find.append_items(&[&item("app.palette", "Find…", Some("CMD+F")), &item("edit.replace", "Replace…", Some("ALT+CMD+F"))]);
    let _ = edit.append(&find);

    let _ = edit.append_items(&[
        &item("dialog:chartData", "Select Data…", None),
        &PredefinedMenuItem::separator(),
        &item("draw.toggle", "Toggle Drawing", Some("CTRL+CMD+Z")),
        &PredefinedMenuItem::separator(),
    ]);

    let autofill = Submenu::new("AutoFill", true);
    let _ = autofill.append(&item("edit.autofill", "AutoFill Options…", None));
    let _ = edit.append(&autofill);

    let _ = edit.append_items(&[&item("edit.dictation", "Start Dictation", None), &item("edit.emoji", "Emoji & Symbols", None)]);

    edit
}

fn check_item(id: &'static str, label: &'static str, checked: bool, acc: Option<&'static str>) -> CheckMenuItem {
    let accel = acc.and_then(|a| Accelerator::from_str(a).ok());
    CheckMenuItem::with_id(id, label, true, checked, accel)
}

fn build_view_menu(view_items: &mut Vec<(&'static str, CheckMenuItem)>) -> Submenu {
    let view = Submenu::new("View", true);

    let normal = check_item("view.normal", "Normal", true, Some("CMD+1"));
    let sorter = check_item("view.sorter", "Slide Sorter", false, Some("CMD+2"));
    let notes = check_item("view.notesPage", "Notes Page", false, Some("CMD+3"));
    let outline = check_item("view.outline", "Outline View", false, Some("CMD+4"));
    let reading = check_item("view.reading", "Reading View", false, Some("CMD+5"));

    view_items.push(("normal", normal.clone()));
    view_items.push(("sorter", sorter.clone()));
    view_items.push(("notesPage", notes.clone()));
    view_items.push(("outline", outline.clone()));

    let _ = view.append_items(&[
        &normal,
        &sorter,
        &notes,
        &outline,
        &reading,
        &item("show.presenter", "Presenter View", Some("ALT+ENTER")),
        &item("show.start", "Slide Show", Some("CMD+SHIFT+ENTER")),
        &PredefinedMenuItem::separator(),
        &item("view.showOutline", "Show Outline", Some("CTRL+SHIFT+TAB")),
    ]);

    let master = Submenu::new("Master", true);
    let _ = master.append_items(&[
        &item("view.slideMaster", "Slide Master", None),
        &item("view.handoutMaster", "Handout Master", None),
        &item("view.notesMaster", "Notes Master", None),
    ]);
    let _ = view.append(&master);

    let _ = view.append(&PredefinedMenuItem::separator());
    let ribbon = check_item("view.collapseRibbon", "Ribbon", true, Some("ALT+CMD+R"));
    view_items.push(("ribbon", ribbon.clone()));
    let _ = view.append(&ribbon);
    let _ = view.append(&item("view.messageBar", "Message Bar", None));

    let _ = view.append(&PredefinedMenuItem::separator());
    let _ = view.append_items(&[
        &item("dialog:headerFooter", "Header and Footer…", None),
        &item("view.markup", "Markup", None),
        &item("view.advancedMarkup", "Advanced Markup", None),
        &PredefinedMenuItem::separator(),
        &item("view.ruler", "Ruler", None),
    ]);

    let grid_guides = Submenu::new("Grid and Guides", true);
    let _ = grid_guides.append_items(&[&item("view.gridlines", "Gridlines", Some("SHIFT+F9")), &item("view.guides", "Guides", Some("ALT+F9"))]);
    let _ = view.append(&grid_guides);

    let zoom = Submenu::new("Zoom", true);
    let _ = zoom.append_items(&[
        &item("dialog:zoom", "Zoom…", None),
        &item("view.zoomIn", "Zoom In", Some("CMD+=")),
        &item("view.zoomOut", "Zoom Out", Some("CMD+-")),
        &item("view.fit", "Fit to Window", Some("CMD+0")),
    ]);
    let _ = view.append(&zoom);

    let _ = view.append(&item("view.fullScreen", "Enter Full Screen", Some("CTRL+CMD+F")));

    view
}

fn build_insert_menu() -> Submenu {
    let insert = Submenu::new("Insert", true);
    let _ = insert
        .append_items(&[&item("slide.new", "New Slide", Some("CMD+SHIFT+N")), &item("slide.duplicate", "Duplicate Slide", Some("CMD+SHIFT+D"))]);

    let slides_from = Submenu::new("Slides From", true);
    let _ = slides_from.append_items(&[&item("dialog:outline", "Outline…", None), &item("slide.reuse", "Reuse Slides…", None)]);
    let _ = insert.append(&slides_from);

    let _ = insert.append_items(&[
        &PredefinedMenuItem::separator(),
        &item("section.add", "Section", None),
        &item("dialog:comment", "Comment", Some("CMD+SHIFT+M")),
        &PredefinedMenuItem::separator(),
        &item("insert.textBox", "Text Box", None),
        &item("insert.wordArt", "WordArt", None),
        &item("dialog:headerFooter", "Header and Footer…", None),
        &item("insert.dateTime", "Date and Time…", None),
        &item("insert.slideNumber", "Slide Number", None),
        &PredefinedMenuItem::separator(),
        &item("dialog:table", "Table…", None),
    ]);

    let chart = Submenu::new("Chart", true);
    let _ = chart.append(&item("insert.chart", "Insert Chart…", None));
    let _ = insert.append(&chart);

    let smartart = Submenu::new("SmartArt", true);
    let _ = smartart.append(&item("dialog:smartart", "SmartArt…", None));
    let _ = insert.append(&smartart);

    let _ = insert.append(&PredefinedMenuItem::separator());

    let pic = Submenu::new("Picture", true);
    let _ = pic.append(&item("app.insertPictureDialog", "Picture from File…", None));
    let _ = insert.append(&pic);

    let audio = Submenu::new("Audio", true);
    let _ = audio.append(&item("app.insertAudioDialog", "Audio from File…", None));
    let _ = insert.append(&audio);

    let video = Submenu::new("Video", true);
    let _ = video.append(&item("app.insertVideoDialog", "Video from File…", None));
    let _ = insert.append(&video);

    let _ = insert.append_items(&[
        &item("insert.cameo", "Cameo", None),
        &item("dialog:equation", "Equation", None),
        &item("dialog:symbol", "Symbol…", None),
    ]);

    let shape = Submenu::new("Shape", true);
    let _ = shape.append_items(&[
        &item("shape.rect", "Rectangle", None),
        &item("shape.roundRect", "Rounded Rectangle", None),
        &item("shape.ellipse", "Oval", None),
        &item("shape.line", "Line", None),
        &item("shape.arrow", "Arrow", None),
    ]);
    let _ = insert.append(&shape);

    let _ = insert.append(&PredefinedMenuItem::separator());
    let _ = insert.append(&item("insert.icons", "Icons…", None));
    let _ = insert.append(&PredefinedMenuItem::separator());

    let models = Submenu::new("3D Models", true);
    let _ = models.append(&item("insert.model3d", "From a File…", None));
    let _ = insert.append(&models);

    let _ = insert.append(&PredefinedMenuItem::separator());

    let zoom = Submenu::new("Zoom", true);
    let _ = zoom.append_items(&[
        &item("insert.summaryZoom", "Summary Zoom", None),
        &item("insert.sectionZoom", "Section Zoom", None),
        &item("insert.slideZoom", "Slide Zoom", None),
    ]);
    let _ = insert.append(&zoom);

    let _ = insert.append(&PredefinedMenuItem::separator());

    let actions = Submenu::new("Action Buttons", true);
    let _ = actions.append_items(&[
        &item("action.back", "Back or Previous", None),
        &item("action.forward", "Forward or Next", None),
        &item("action.home", "Home", None),
        &item("action.info", "Information", None),
    ]);
    let _ = insert.append(&actions);
    let _ = insert.append(&item("action.settings", "Action Settings…", None));

    let _ = insert.append(&PredefinedMenuItem::separator());
    let _ = insert.append_items(&[&item("insert.object", "Object…", None), &item("dialog:hyperlink", "Hyperlink…", Some("CMD+K"))]);

    insert
}

fn build_format_menu() -> Submenu {
    let format = Submenu::new("Format", true);
    let _ = format.append_items(&[
        &item("format.bold", "Bold", Some("CMD+B")),
        &item("format.italic", "Italic", Some("CMD+I")),
        &item("format.underline", "Underline", Some("CMD+U")),
        &PredefinedMenuItem::separator(),
        &item("format.alignLeft", "Align Left", Some("CMD+L")),
        &item("format.alignCenter", "Center", Some("CMD+E")),
        &item("format.alignRight", "Align Right", Some("CMD+R")),
        &item("format.justify", "Justify", Some("CMD+J")),
        &PredefinedMenuItem::separator(),
        &item("dialog:paragraph", "Paragraph…", None),
        &item("format.rtl", "Right-to-Left Text Direction", None),
    ]);
    format
}

fn build_arrange_menu() -> Submenu {
    let arrange = Submenu::new("Arrange", true);
    let _ = arrange.append_items(&[
        &item("arrange.bringToFront", "Bring to Front", Some("CMD+SHIFT+]")),
        &item("arrange.sendToBack", "Send to Back", Some("CMD+SHIFT+[")),
        &item("arrange.bringForward", "Bring Forward", Some("CMD+]")),
        &item("arrange.sendBackward", "Send Backward", Some("CMD+[")),
        &PredefinedMenuItem::separator(),
        &item("arrange.group", "Group", Some("CMD+ALT+G")),
        &item("arrange.ungroup", "Ungroup", Some("CMD+ALT+SHIFT+G")),
        &item("arrange.regroup", "Regroup", None),
        &PredefinedMenuItem::separator(),
        &item("arrange.rotateLeft", "Rotate Left 90°", None),
        &item("arrange.rotateRight", "Rotate Right 90°", None),
        &item("arrange.flipHorizontal", "Flip Horizontal", None),
        &item("arrange.flipVertical", "Flip Vertical", None),
    ]);
    arrange
}

fn build_tools_menu() -> Submenu {
    let tools = Submenu::new("Tools", true);
    let _ = tools.append_items(&[
        &item("dialog:spelling", "Spelling…", Some("F7")),
        &item("dialog:accessibility", "Check Accessibility", None),
        &item("dialog:language", "Language…", None),
    ]);
    tools
}

fn build_slide_show_menu() -> Submenu {
    let show = Submenu::new("Slide Show", true);
    let _ = show.append_items(&[
        &item("show.start", "Play from Start", Some("CMD+SHIFT+ENTER")),
        &item("show.fromCurrent", "Play from Current Slide", Some("CMD+ENTER")),
        &item("show.presenter", "Presenter View", Some("ALT+ENTER")),
        &PredefinedMenuItem::separator(),
        &item("slide.hide", "Hide Slide", None),
        &item("dialog:customShow", "Custom Shows…", None),
        &item("dialog:setupShow", "Set Up Show…", None),
    ]);
    show
}

fn build_window_menu() -> Submenu {
    let window = Submenu::new("Window", true);
    let _ = window.append_items(&[
        &item("window.minimize", "Minimize", Some("CMD+M")),
        &item("window.zoom", "Zoom", None),
        &PredefinedMenuItem::separator(),
        &item("window.bringAllToFront", "Bring All to Front", None),
    ]);
    window
}

fn build_help_menu() -> Submenu {
    let help = Submenu::new("Help", true);
    let _ = help.append_items(&[
        &item("app.palette", "DeckCraft Help / Search", Some("CMD+SHIFT+P")),
        &item("url:https://discord.gg/artcraft", "Join Community Discord…", None),
        &PredefinedMenuItem::separator(),
        &item("app.about", "About DeckCraft", None),
    ]);
    help
}

impl NativeMenu {
    pub fn install(ctx: &egui::Context) -> NativeMenu {
        let menu = Menu::new();
        let mut view_items = Vec::new();

        let _ = menu.append(&build_app_menu());
        let _ = menu.append(&build_file_menu());
        let _ = menu.append(&build_edit_menu());
        let _ = menu.append(&build_view_menu(&mut view_items));
        let _ = menu.append(&build_insert_menu());
        let _ = menu.append(&build_format_menu());
        let _ = menu.append(&build_arrange_menu());
        let _ = menu.append(&build_tools_menu());
        let _ = menu.append(&build_slide_show_menu());

        let window = build_window_menu();
        let _ = menu.append(&window);
        let _ = menu.append(&build_help_menu());

        menu.init_for_nsapp();
        window.set_as_windows_menu_for_nsapp();

        let (tx, rx) = channel();
        let ctx_clone = ctx.clone();
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            let _ = tx.send(e);
            ctx_clone.request_repaint();
        }));

        NativeMenu { _menu: menu, events: rx, view_items }
    }

    pub fn poll(&self, app: &mut SlideApp, ctx: &egui::Context) {
        while let Ok(event) = self.events.try_recv() {
            let id = event.id().as_ref();
            if let Some(url) = id.strip_prefix("url:") {
                app.open_url(url);
            } else if let Some(d) = id.strip_prefix("dialog:") {
                app.dialog = Some(Dialog::new(d));
            } else {
                match id {
                    "file.newTemplate" => app.open_backstage(deckcraft_ui_egui::backstage::Page::New),
                    "file.save" => app.save(),
                    "file.saveTemplate" => app.save_as_dialog("deckcraft"),
                    "file.passwords" => app.set_status("Password protection is not supported in this version"),
                    "file.properties" => app.open_backstage(deckcraft_ui_egui::backstage::Page::Info),
                    "file.clearRecent" => {
                        app.ui.recent.clear();
                        app.set_status("Recent presentations cleared");
                    }
                    "file.readOnly" => {
                        app.set_status("Presentation is read-only");
                    }
                    "edit.pasteSpecial" => {
                        let _ = app.run("edit.paste", json!({}));
                    }
                    "edit.replace" => {
                        app.palette = Some((String::new(), 0));
                    }
                    "slide.reuse" => {
                        let _ = app.run("app.openDialog", json!({}));
                    }
                    "app.closePresentation" => app.close_presentation(),
                    "view.normal" => app.ui.view = ViewMode::Normal,
                    "view.sorter" => app.ui.view = ViewMode::Sorter,
                    "view.notesPage" => app.ui.view = ViewMode::NotesPage,
                    "view.outline" => app.ui.view = ViewMode::Outline,
                    "view.reading" => {
                        let i = app.session.active().map(|d| d.selection.slide).unwrap_or(0);
                        app.start_show(i, true);
                    }
                    "view.collapseRibbon" => app.ui.ribbon_collapsed = !app.ui.ribbon_collapsed,
                    "view.fullScreen" => {
                        let is_fs = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!is_fs));
                    }
                    "window.minimize" => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                    "window.zoom" => {
                        let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
                    }
                    "show.fromCurrent" => {
                        let i = app.session.active().map(|d| d.selection.slide).unwrap_or(0);
                        app.start_show(i, false);
                    }
                    "shape.rect" => {
                        let _ = app.run("insert.shape", json!({"preset": "rect"}));
                    }
                    "shape.roundRect" => {
                        let _ = app.run("insert.shape", json!({"preset": "roundRect"}));
                    }
                    "shape.ellipse" => {
                        let _ = app.run("insert.shape", json!({"preset": "ellipse"}));
                    }
                    "shape.line" => {
                        let _ = app.run("insert.shape", json!({"preset": "line"}));
                    }
                    "shape.arrow" => {
                        let _ = app.run("insert.shape", json!({"preset": "rightArrow"}));
                    }
                    cmd => {
                        let _ = app.run(cmd, json!({}));
                    }
                }
            }
        }

        // Sync view checkboxes
        for (mode, item) in &self.view_items {
            let checked = match *mode {
                "normal" => app.ui.view == ViewMode::Normal,
                "sorter" => app.ui.view == ViewMode::Sorter,
                "notesPage" => app.ui.view == ViewMode::NotesPage,
                "outline" => app.ui.view == ViewMode::Outline,
                "ribbon" => !app.ui.ribbon_collapsed,
                _ => false,
            };
            if item.is_checked() != checked {
                item.set_checked(checked);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_accelerators_parse() {
        for acc in [
            "CMD+,",
            "CMD+N",
            "CMD+SHIFT+P",
            "CMD+O",
            "CMD+W",
            "CMD+S",
            "CMD+SHIFT+S",
            "CMD+P",
            "CMD+Z",
            "CMD+Y",
            "CMD+X",
            "CMD+C",
            "CMD+V",
            "CTRL+CMD+V",
            "ALT+SHIFT+CMD+V",
            "CMD+A",
            "CMD+D",
            "CMD+F",
            "ALT+CMD+F",
            "CTRL+CMD+Z",
            "CMD+1",
            "CMD+2",
            "CMD+3",
            "CMD+4",
            "CMD+5",
            "ALT+ENTER",
            "CMD+SHIFT+ENTER",
            "CTRL+SHIFT+TAB",
            "ALT+CMD+R",
            "SHIFT+F9",
            "ALT+F9",
            "CMD+=",
            "CMD+-",
            "CMD+0",
            "CTRL+CMD+F",
            "CMD+SHIFT+N",
            "CMD+SHIFT+D",
            "CMD+SHIFT+M",
            "CMD+K",
            "CMD+B",
            "CMD+I",
            "CMD+U",
            "CMD+L",
            "CMD+E",
            "CMD+R",
            "CMD+J",
            "CMD+SHIFT+]",
            "CMD+SHIFT+[",
            "CMD+]",
            "CMD+[",
            "CMD+ALT+G",
            "CMD+ALT+SHIFT+G",
            "F7",
            "CMD+ENTER",
            "CMD+M",
        ] {
            if let Err(e) = Accelerator::from_str(acc) {
                panic!("accelerator {acc} failed: {e:?}");
            }
        }
    }
}
