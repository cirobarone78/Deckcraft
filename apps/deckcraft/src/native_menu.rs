//! The macOS menu bar: DeckCraft, then the menus of `deckcraft_ui_egui::menus::menu_tree` (File,
//! Edit, View, Insert, Format, Arrange, Slide Show, Help), plus Window, as native menus.
//!
//! Every item runs its command through `SlideApp::run`, as the command palette does; Cut, Copy,
//! Paste and Select All go to a focused text field as the keyboard's would. Items carry no key
//! equivalents: the keyboard keeps going through egui exactly as before, so a menu never takes a
//! key away from a text field or the slide.

#[cfg(target_os = "macos")]
pub(crate) use bar::NativeMenu;

#[cfg(target_os = "macos")]
mod bar {
    use std::sync::mpsc::{Receiver, Sender, channel};

    use deckcraft_ui_egui::SlideApp;
    use muda::{IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
    use serde_json::json;

    #[derive(Clone, Debug)]
    enum Action {
        Run(&'static str),
        Open(String),
        Minimize,
    }

    pub(crate) struct NativeMenu {
        /// Kept alive: AppKit holds the menu only while muda's handles exist.
        _menu: Menu,
        _submenus: Vec<Submenu>,
        events: Receiver<MenuEvent>,
        actions: Vec<(muda::MenuId, Action)>,
        /// Command items, greyed out while their command can't run (nothing selected…).
        tracked: Vec<(MenuItem, &'static str, bool)>,
        recent: Submenu,
        recent_paths: Option<Vec<String>>,
        seq: u64,
        /// Input events for the next frame's `raw_input_hook`.
        pub(crate) input: Vec<egui::Event>,
    }

    fn append(menu: &Submenu, item: &dyn IsMenuItem) {
        if let Err(e) = menu.append(item) {
            log::warn!("native menu: {e}");
        }
    }

    impl NativeMenu {
        fn next_id(&mut self) -> String {
            self.seq = self.seq.saturating_add(1);
            format!("deckcraft.{}", self.seq)
        }

        fn add(&mut self, menu: &Submenu, text: &str, action: Action) {
            let item = MenuItem::with_id(self.next_id(), text, true, None);
            if let Action::Run(id) = action {
                self.tracked.push((item.clone(), id, true));
            }
            self.actions.push((item.id().clone(), action));
            append(menu, &item);
        }

        /// Build the menu bar and install it. `None` off the main thread.
        fn install(ctx: &egui::Context) -> Option<Self> {
            objc2::MainThreadMarker::new()?;
            let (tx, events) = channel::<MenuEvent>();
            set_handler(tx, ctx.clone());
            let menu = Menu::new();
            let recent = Submenu::new("Open Recent", false);
            let mut me = Self {
                _menu: menu.clone(),
                _submenus: Vec::new(),
                events,
                actions: Vec::new(),
                tracked: Vec::new(),
                recent: recent.clone(),
                recent_paths: None,
                seq: 0,
                input: Vec::new(),
            };

            let app_menu = Submenu::new("DeckCraft", true);
            me.add(&app_menu, "About DeckCraft", Action::Run("app.about"));
            append(&app_menu, &PredefinedMenuItem::separator());
            me.add(&app_menu, "Settings…", Action::Run("app.preferences"));
            append(&app_menu, &PredefinedMenuItem::separator());
            append(&app_menu, &PredefinedMenuItem::services(None));
            append(&app_menu, &PredefinedMenuItem::separator());
            append(&app_menu, &PredefinedMenuItem::hide(None));
            append(&app_menu, &PredefinedMenuItem::hide_others(None));
            append(&app_menu, &PredefinedMenuItem::show_all(None));
            append(&app_menu, &PredefinedMenuItem::separator());
            append(&app_menu, &PredefinedMenuItem::quit(None));
            let mut subs = vec![app_menu];

            let mut help = None;
            for (title, items) in deckcraft_ui_egui::menus::menu_tree() {
                let sub = Submenu::new(title, true);
                for (label, id) in items {
                    if label == "-" {
                        append(&sub, &PredefinedMenuItem::separator());
                        continue;
                    }
                    // About is in the DeckCraft menu, as on every Mac app.
                    if id == "app.about" {
                        continue;
                    }
                    me.add(&sub, label, Action::Run(id));
                    if id == "app.openDialog" {
                        append(&sub, &recent);
                    }
                }
                if title == "Help" {
                    help = Some(sub);
                    continue;
                }
                subs.push(sub);
            }
            // Window before Help, as on every Mac app.
            let window = Submenu::new("Window", true);
            me.add(&window, "Minimize", Action::Minimize);
            append(&window, &PredefinedMenuItem::maximize(None));
            append(&window, &PredefinedMenuItem::separator());
            append(&window, &PredefinedMenuItem::fullscreen(None));
            append(&window, &PredefinedMenuItem::separator());
            append(&window, &PredefinedMenuItem::bring_all_to_front(None));
            subs.push(window.clone());
            if let Some(h) = &help {
                subs.push(h.clone());
            }
            for s in &subs {
                if let Err(e) = menu.append(s) {
                    log::warn!("native menu: {e}");
                }
            }
            menu.init_for_nsapp();
            window.set_as_windows_menu_for_nsapp();
            if let Some(h) = help {
                h.set_as_help_menu_for_nsapp();
            }
            subs.push(recent);
            me._submenus = subs;
            Some(me)
        }

        /// Each frame: build the bar once, run what was chosen, keep Open Recent current.
        pub(crate) fn frame(slot: &mut Option<Self>, app: &mut SlideApp, ctx: &egui::Context) {
            if slot.is_none() {
                *slot = Self::install(ctx);
            }
            let Some(me) = slot.as_mut() else { return };
            while let Ok(e) = me.events.try_recv() {
                let Some(action) = me.actions.iter().find(|(id, _)| id == e.id()).map(|(_, a)| a.clone()) else { continue };
                match action {
                    Action::Run(id) => {
                        // In a text field, the clipboard and Select All act on its text, as ⌘X,
                        // ⌘C, ⌘V and ⌘A do.
                        if ctx.memory(|m| m.focused().is_some())
                            && let Some(events) = text_field_events(id)
                        {
                            me.input.extend(events);
                        } else {
                            // `run` shows any error in the status bar.
                            let _ = app.run(id, json!({}));
                        }
                    }
                    Action::Open(path) => {
                        if let Err(err) = app.open_path(&path) {
                            log::warn!("open recent {path}: {err}");
                        }
                    }
                    Action::Minimize => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
                }
                ctx.request_repaint();
            }
            let typing = ctx.memory(|m| m.focused().is_some());
            for (item, id, shown) in &mut me.tracked {
                // A text field takes the clipboard and Select All whatever the slide's state.
                let enabled = (typing && text_field_events(id).is_some()) || super::available(app, id);
                if enabled != *shown {
                    item.set_enabled(enabled);
                    *shown = enabled;
                }
            }
            if me.recent_paths.as_ref() != Some(&app.ui.recent) {
                me.rebuild_recent(app);
            }
        }

        fn rebuild_recent(&mut self, app: &SlideApp) {
            let sub = self.recent.clone();
            // Bounded: each pass removes one item of a finite submenu.
            while sub.remove_at(0).is_some() {}
            self.actions.retain(|(_, a)| !matches!(a, Action::Open(_)));
            for p in &app.ui.recent {
                let name = std::path::Path::new(p).file_name().map_or_else(|| p.clone(), |n| n.to_string_lossy().into_owned());
                let item = MenuItem::with_id(self.next_id(), name, true, None);
                self.actions.push((item.id().clone(), Action::Open(p.clone())));
                append(&sub, &item);
            }
            sub.set_enabled(!app.ui.recent.is_empty());
            self.recent_paths = Some(app.ui.recent.clone());
        }
    }

    /// Clicks go to `tx`, and wake the event loop so the next frame sees them.
    fn set_handler(tx: Sender<MenuEvent>, ctx: egui::Context) {
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            let _ = tx.send(e);
            ctx.request_repaint();
        }));
    }

    /// The keyboard's input events for a clipboard or Select All command (as egui-winit makes
    /// them), or `None` for any other command.
    fn text_field_events(id: &str) -> Option<Vec<egui::Event>> {
        let key = |key: egui::Key| {
            [true, false]
                .map(|pressed| egui::Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers: egui::Modifiers::COMMAND })
                .to_vec()
        };
        Some(match id {
            "edit.cut" => vec![egui::Event::Cut],
            "edit.copy" => vec![egui::Event::Copy],
            "edit.paste" | "edit.pasteText" => {
                let text = arboard::Clipboard::new().and_then(|mut c| c.get_text()).unwrap_or_default().replace("\r\n", "\n");
                if text.is_empty() { Vec::new() } else { vec![egui::Event::Paste(text)] }
            }
            "edit.selectAll" => key(egui::Key::A),
            "edit.undo" => key(egui::Key::Z),
            _ => return None,
        })
    }
}

/// Whether `id` can run now: engine commands say so themselves; the app's own (`app.*`, views,
/// the show) always can.
#[cfg(any(target_os = "macos", test))]
fn available(app: &deckcraft_ui_egui::SlideApp, id: &str) -> bool {
    deckcraft_engine::cmd::command_specs().iter().find(|c| c.id == id).is_none_or(|c| (c.enabled)(&app.session).is_ok())
}

#[cfg(test)]
mod tests {
    use deckcraft_ui_egui::{Services, SlideApp};
    use serde_json::json;

    /// Every menu item does something on the sample presentation: no command is unknown or
    /// fails for want of parameters.
    #[test]
    fn every_menu_item_runs_on_the_sample_presentation() {
        // These open the system's file or print dialogs, start a full-screen show, or close the
        // presentation; they run here as the palette runs them, just not in a unit test.
        const SYSTEM: &[&str] = &[
            "app.openDialog",
            "app.saveAsDialog",
            "app.exportDialog",
            "app.print",
            "app.closePresentation",
            "file.save",
            "show.start",
            "show.fromStart",
            "show.fromCurrent",
            "app.insertPictureDialog",
            "app.insertAudioDialog",
            "app.insertVideoDialog",
        ];
        let mut failures = Vec::new();
        let mut ids: Vec<&str> =
            deckcraft_ui_egui::menus::menu_tree().into_iter().flat_map(|(_, items)| items).map(|(_, id)| id).filter(|id| !id.is_empty()).collect();
        ids.push("app.preferences");
        for id in ids {
            if SYSTEM.contains(&id) {
                continue;
            }
            let mut session = deckcraft_engine::Session::new();
            assert!(deckcraft_engine::sample::open_sample(&mut session).is_ok(), "the sample opens");
            let mut app = SlideApp::new(session, Services::default());
            // A greyed-out item (nothing selected, nothing to undo) is right not to run.
            if !super::available(&app, id) {
                continue;
            }
            // Regroup needs a group ungrouped earlier; it says so, as its key does.
            if let Err(e) = app.run(id, json!({}))
                && id != "arrange.regroup"
            {
                failures.push(format!("{id}: {e}"));
            }
        }
        assert!(failures.is_empty(), "menu items that fail:\n{}", failures.join("\n"));
    }
}
