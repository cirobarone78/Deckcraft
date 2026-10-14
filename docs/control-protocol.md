# Control protocol

Start the app with `--control <port>`. The server listens on `127.0.0.1` only and speaks JSON
lines: one request object per line, one reply per line.

```json
{"id": 1, "method": "engine.execute", "params": {"command": "shape.insert", "params": {"preset": "roundRect", "rect": [100, 100, 240, 120], "text": "Hello"}}}
{"id": 1, "ok": true, "result": {"id": 258}}
```

Coordinates are slide points (1/72 inch; a 16:9 slide is 960 × 540) unless noted.

| Method | Params | What it does |
|---|---|---|
| `engine.execute` / `ui.menu.invoke` / `command` | `{command, params}` | Run any command (see `engine.commands`; `deckcraft-cli describe ID` documents one) |
| `engine.commands` | — | Every command: id, label, menu path, shortcut, params doc, enabled / disabled reason |
| `document.inspect` | — | Slides, sections, layouts, shapes, selection |
| `ui.inspect` | — | UI state, active tool, window/canvas/slide rects, dialog, `home` (the File tab open over a presentation), `backstage` (its page, also the start screen), slide show state, documents, perf (fps, render ms) |
| `ui.tool.select` | `{tool, preset?}` | `select`, `text`, `shape` + preset, any preset name, `curve`, `freeform`, `scribble`, `pen`, `highlighter`, `eraser` |
| `ui.pointer` | `{events: [{kind: down\|drag\|up\|move\|doubleclick\|tripleclick, x, y, mods?}], mods?}` | Drive the active tool in slide points through the same code path as the mouse |
| `ui.key` / `ui.text` | `{key, shift?, alt?, cmd?}` / `{text}` | Keyboard input (also drives a running slide show) |
| `ui.ime` | `{preedit, active?: [start, end]}` / `{commit}` | An input method's composition through egui, as a real one arrives: shown in place on the slide or in a focused field; `{"preedit": ""}` cancels |
| `ui.move` / `ui.click` / `ui.drag` | screen points | Real egui pointer input — reaches every widget, menu and pane |
| `ui.set` | `{tab?, view?, pane?, notes?, zoom?, brightness?, ruler?, gridlines?, guides?, …}` | UI state (returns the full state) |
| `ui.dialog.open` / `ui.dialog.close` | `{id, fields?}` | Open or close a dialog by id; `fields` pre-fills its fields (About: `{"tab": "contributors"}` or `"models"`) |
| `ui.screenshot` | `{path?}` | PNG of the whole window (base64 when no path) |
| `ui.render` | `{path?, slide?, scale?}` | Render a slide headlessly |
| `ui.resize` / `ui.focus` | | Window control |
| `show.start` / `show.end` | `{from?: slide index, presenter?, reading?}` | Slide show |
| `app.open` / `app.save` / `app.export` / `app.quit` | `{path, …}` | Files (export takes the `file.export` params, e.g. `{"path": "deck.pdf", "layout": "handouts", "perPage": 3}`) |

Cloud storage is reached through `engine.execute` like any other command. Every one of these
answers with the whole cloud state. They do nothing without a CollabCraft service configured
(`DECKCRAFT_CLOUD_URL`). They say so when there is none.

| Command | Params | What it does |
|---|---|---|
| `cloud.status` | — | The account, what the provider can do, the folder being browsed, what other people shared, transfers in flight, where the open presentation came from |
| `cloud.open` | — | Opens File › Open and asks the service who is signed in |
| `cloud.browse` | `{path}` | Lists one folder; an empty path is the root |
| `cloud.shared` | — | Lists what other people shared with this user |
| `cloud.link` | — | Begins linking a cloud account. It takes a browser round trip |
| `cloud.openFile` | `{id?, name?}` | Opens something already listed, by handle or by name |
| `cloud.save` | — | Writes back over the entry the presentation came from, under the version it was read at |
| `cloud.saveAs` | `{folder?, name?}` | Saves a new presentation into a folder; without both it opens the Save As page on the cloud |
| `cloud.reload` | — | Reads the stored copy again, discarding local edits |

The MCP server (`deckcraft-cli mcp`, see [mcp.md](mcp.md)) wraps the same methods for Claude
and other agents; `deckcraft-cli app METHOD [JSON]` sends one request from a shell.
