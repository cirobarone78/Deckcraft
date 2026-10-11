# Interface appearance

Preferences > General, the View ribbon and the title bar each offer the same Appearance
selector: System, Light and Dark. New installations still use Light. Existing `ui.json`
values `"brightness": "light"` and `"brightness": "dark"` retain their manual behavior.
System saves as `"brightness": "system"` and remains selected when the displayed palette changes.

System chooses the light interface for a Light desktop and the dark interface for a Dark
desktop. If the host supplies no appearance, it uses Light. Fixed Light and Dark ignore
host appearance changes. Presentation themes, slide backgrounds and content colors are
independent of this preference. The existing interface surround uses the chosen palette.

On macOS and Windows, the native window backend supplies the initial appearance and live
appearance events. DeckCraft keeps the native window's appearance inherited instead of
pinning it to the resolved palette. Both egui style branches receive DeckCraft's resolved
colors so menus, dialogs and custom-painted interface panels agree even in a manual mode.

On Linux, DeckCraft also subscribes to the freedesktop Settings portal's
`org.freedesktop.appearance` / `color-scheme` setting. Standard values are 0 for no preference,
1 for Dark and 2 for Light. The listener subscribes before reading the initial value and
requests a repaint when the value changes. It performs no periodic desktop polling.
Startup waits at most 250 ms for the worker; a slower initial result updates the interface
when it arrives. The current Linux window backend does not report the desktop appearance.
If the portal is unavailable, System uses Light and cannot follow desktop changes automatically.

FreeBSD currently has no appearance feed and System uses Light. Other native platforms use
appearance reports where their window backend provides them, then fall back to Light. The web frontend uses the browser
appearance reports supplied by eframe. This change preserves its existing persistence
behavior; desktop `ui.json` storage is separate from the web host.

Automation can select a mode through `view.appearance` with
`{"brightness":"system"}`, `{"brightness":"light"}` or `{"brightness":"dark"}`.
`ui.inspect` reports the saved selection, resolved palette and native system report under
`appearance`. The existing `view.dark` boolean command still selects a fixed manual mode;
without an `on` argument it toggles the currently displayed palette.

For an isolated desktop session, set `DECKCRAFT_CONFIG_DIR` to a separate settings directory.
It contains that session's `ui.json`, `prefs.json` and logs. Use `DECKCRAFT_NO_RECOVERY=1`
to disable recovery of presentations from the normal desktop recovery directory.
