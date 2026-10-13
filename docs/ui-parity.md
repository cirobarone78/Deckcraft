# UI parity with PowerPoint

> **Last reviewed:** 2026-10-12 · **Last updated:** 2026-10-12 · **Change:** minor (shortcut audit against PowerPoint's Windows and Mac lists) · **Target:** Microsoft PowerPoint for Mac 16.113 (Microsoft 365)

How DeckCraft looks and feels next to PowerPoint: the shell, the ribbon, menus, panes, dialogs,
direct manipulation, keyboard and precision. Observations of PowerPoint are kept locally in
`plan/powerpoint/01-observed-ui.md` and `menu-tree.txt` (gitignored, clean-room rules in
AGENTS.md §2). Dimension score: **~60% ready** (estimated), **30–50 h** to parity.

## Shell

| Element | PowerPoint for Mac | DeckCraft | Gap |
|---|---|---|---|
| Title bar: AutoSave, Home, Save, Undo/Redo, document title, Share, search | yes | yes, no Share (cloud) | #71 macOS window controls misaligned |
| Native macOS menu bar (File, Edit, View, Insert, Format, Arrange, Tools, Slide Show, Window, Help) | yes | **no** (`menus::menu_tree()` is defined but not installed) | 3–5 h |
| Menu-bar items with a DeckCraft equivalent | 217 app-specific items | 128 matched by name (59%, heuristic lower bound) | See [target-app-parity.md](target-app-parity.md) |
| Ribbon tabs: Home, Insert, Draw, Design, Transitions, Animations, Slide Show, Record, Review, View | 10 | 10 | Record tab is a placeholder |
| Contextual tabs: Shape Format, Picture Format, Table Design, Table Layout, Chart Design, Chart Format, Video/Audio Format and Playback | yes | most; checklist P0 row partial | Chart Format, media tabs thinner |
| Start screen: new, themes, templates, recent, open | yes | yes, thin (#17 broken layout, #76 missing overlay) | 3–5 h |
| Status bar: slide counter, language, notes, comments, views, zoom slider | yes | yes | |
| Command search (Tell Me) | yes | command palette | #79 default entry fails |
| Light / dark appearance | follows the system | light or dark; doesn't follow the system (#41) | 1–2 h |
| One window per presentation, New Window, Arrange All | yes | tabs in one window; no New Window | 3–5 h |
| Quick Access Toolbar customization | yes | no | P1 partial |

## Panes and dialogs

| Pane / dialog | PowerPoint | DeckCraft |
|---|---|---|
| Format Shape / Picture / Text pane (Fill & Line, Effects, Size & Properties, Picture, Text Box) | full | partial: fill, gradient editor, line, some effects, size; no 3-D, thin picture/texture options |
| Format Background pane | yes | yes |
| Selection Pane | yes | yes |
| Animation Pane | list + advanced timeline | list, reorder, timing; no timeline |
| Comments pane (threads) | yes | yes |
| Design Ideas (Designer) | yes (cloud) | no |
| Dialogs | ~60 (Font, Paragraph, Bullets and Numbering, Find, Replace, Replace Fonts, Hyperlink, Action Settings, Header and Footer, Slide Size, Set Up Show, Custom Shows, Print, Page Setup, Insert Object, Symbol, Equation, Chart data sheet, SmartArt chooser, Preferences…) | 27 (`crates/ui-egui/src/dialogs.rs`): Table, Slide Size, Header and Footer, Hyperlink, Set Up Show, Zoom, Comment, Alt Text, Outline, Rename (section, shape, layout), Paragraph, About, Preferences, Export, Symbol, Equation (placeholder), Spelling, Accessibility, Language, Custom Shows, Chart Data, Trim Media, SmartArt chooser, Quit, Notes Pages |

## Direct manipulation

| Behaviour | PowerPoint | DeckCraft |
|---|---|---|
| Selection handles, rotation handle, yellow adjustment handles | yes | yes |
| Marquee, Shift/Cmd multi-select, Tab to cycle | yes | yes |
| Smart guides (edges, centres, equal spacing) | yes | yes |
| Snap to grid, grid options, drawing guides | yes | gridlines, guides and snap to grid (engine setting); no Grid Options dialog or menu toggle |
| Nudge (arrows), fine nudge (Option+arrows) | yes | yes |
| Shift-constrained move, Option-drag to duplicate | yes | yes |
| Connector glue and rerouting | yes | yes |
| Edit points on freeforms | yes | **no** |
| Crop handles, crop to shape, aspect ratio | yes | partial |
| Text caret, selection, double/triple click, drag-select | yes | yes; Tab in text boxes ends editing (#78) |
| Exact size/position, lock aspect | yes | yes |
| Rulers with indent markers and tab stops | yes | rulers yes; tab stops partial |
| Context menus on shapes, slides, text, thumbnails | yes | yes, shorter |
| Drag and drop of files and between thumbnails | yes | yes |
| Image paste from the clipboard | yes | macOS yes; Windows and Linux no (#53) |
| Font menu: every installed family with its faces | yes | families yes; only Regular/Bold/Italic faces (#69) |

## Keyboard shortcuts

PowerPoint's Windows and Mac shortcuts both work wherever they don't collide; where they do,
DeckCraft follows the platform it runs on (`Session::mac_keys`). F6 / Shift+F6 cycle the slide,
the notes and the thumbnails. Not yet: Find and Replace (Cmd+F opens command search), the Font
dialog, Print, Paste Special, ribbon KeyTips (Windows), the outline-level keys. The audit below
lists every row.

### Shortcut audit

Checked against Microsoft's published "Keyboard shortcuts in PowerPoint" (Windows and Mac tables)
and "Use keyboard shortcuts to deliver PowerPoint presentations", 2026-10-12. Bindings live in
`crates/engine/src/keys.rs` (`Session::key`: editing, objects, slides and the `EXTRA_SHORTCUTS`
table of platform keys), each command's own `shortcut` in `crates/engine/src/cmd/*`,
`menus::ui_shortcut` (views, panes, dialogs, the show, file dialogs) and `show.rs` (keys during a
show). "Ctrl" means Cmd on a Mac unless the Mac column says otherwise; a shifted symbol matches the
key it's typed on (Ctrl+Shift+] wherever `}` is). Coverage of the 192 listed shortcuts (Windows 87,
Mac 70, slide show 35): **68 before this audit, 150 after** (plus 3 partial). Several "present"
punctuation keys (Ctrl+], Ctrl+Shift+>, Ctrl+Shift+=…) never fired from the keyboard before: egui
names the key `CloseBracket` and the registry `]`.

| Shortcut (Windows) | Mac | Action | Status |
|---|---|---|---|
| Ctrl+N, Ctrl+O, Ctrl+S, Ctrl+W, Ctrl+Z / Y, Ctrl+X / C / V, Ctrl+A, Ctrl+K | Cmd | file, clipboard, undo/redo, select all, hyperlink | present |
| Ctrl+B / I / U, Ctrl+L / E / R / J, Ctrl+Shift+C / V, Shift+F3, Ctrl+Space | Cmd; Control+Space | character and paragraph format, Format Painter, Change Case, clear formatting | present |
| F5, Shift+F5, Shift+F9, Alt+F9, Ctrl+F1 | Cmd+Shift+Return, Cmd+Return, Shift+F9 | show from start / current, gridlines, guides, ribbon | present |
| Tab / Shift+Tab, Esc, arrows, Alt+Shift+C / V, Alt+P | same; Cmd+Option+G / Shift+G / J | cycle objects, deselect, nudge, Animation Painter, play media, group | present |
| Ctrl+M (Windows), Ctrl+Shift+D | Cmd+Shift+N, Cmd+Shift+D | new slide, duplicate slide | **added** (Cmd+M stays Minimize on a Mac) |
| Ctrl+G / Ctrl+Shift+G / Ctrl+Shift+J | Cmd+Option+G / Shift+G / J | group, ungroup, regroup | **added** (Windows keys) |
| Ctrl+Shift+[ / ], Ctrl+[ / ] | Cmd+Shift+B / F, Cmd+Option+Shift+B / F | send to back / bring to front, backward / forward | **added**; Windows Ctrl+Shift+[ / ] keep back / front (Microsoft's table says one step) |
| Ctrl+= / Ctrl+Shift+=, Ctrl+Shift+> / < | Cmd+Control+= / Cmd+Control+Shift+=, Cmd+Shift+> / < | subscript / superscript, font size | **fixed** (Ctrl+= was Zoom In; Ctrl+Plus and Mac Cmd+= still zoom) |
| Ctrl+Shift+Z | Cmd+Shift+Z is Redo | clear character formatting | **added** |
| — | Cmd+Option+Shift+C / V | Animation Painter | **added** |
| Alt+Left / Right, Ctrl+arrows | Option+Left / Right, Cmd+arrows, Shift+arrows | rotate 15°, nudge 1 pt, resize | **added** |
| Ctrl+Alt+5 | — | select the first object | **added** |
| Ctrl+Space | Space | play / pause the selected media | **added** |
| Ctrl+Left / Right, Ctrl+Up / Down, Ctrl+Home / End, Ctrl+Backspace / Delete | Cmd+Left / Right (line), Cmd+Up / Down (text), Cmd+Delete | word, paragraph, text start/end, delete a word | **added** |
| Ctrl+Enter | — | next title or body placeholder (a new slide after the last) | **added** |
| Alt+Shift+Left / Right | Cmd+[ / ] | promote / demote a paragraph | **added** |
| Ctrl+Tab | Option+Shift+Tab | tab character in a table cell | **added** |
| Ctrl+Up / Down, Ctrl+Shift+Up / Down | Cmd+Up / Down | move the slide one place / to the start or end | **added** |
| Alt+F5 | Option+Return | Presenter View | **added** |
| Ctrl+Shift+Tab | Control+Shift+Tab | thumbnails / outline | **added** |
| F6 / Shift+F6 | same | slide → notes → thumbnails (arrows change slides there) | **added** |
| Alt+F10 | Cmd+Shift+1, Cmd+Shift+2 | Selection pane; Format pane, Format Background | **added** |
| Ctrl+Alt+O | Cmd+Option+O, Cmd+Shift+O | fit to window; recent presentations (start screen) | **added** |
| — | Cmd+1 / 2 / 3 / 4, Cmd+Option+1, Cmd+Option+Control+G | Normal, Slide Sorter, Notes Page, Outline; Slide Master; guides | **added** |
| Ctrl+Alt+M, F7, Alt+= | Cmd+Shift+M, Cmd+Option+M | New Comment, Spelling, Equation (placeholder dialog); Paragraph dialog | **added** (they ran without a dialog or errored before) |
| Ctrl+Q | Cmd+Q (system) | quit, asking about unsaved changes | **added** |
| Ctrl+F, Ctrl+H, Shift+F4 | Cmd+F, Cmd+Shift+H | Find, Replace, Find Next | missing: no find in slides yet (Ctrl+F opens command search) |
| Ctrl+T, Ctrl+Shift+F | Cmd+T | Font dialog | missing: no Font dialog |
| Ctrl+P | Cmd+P | Print | missing: no print |
| Ctrl+Alt+V | Cmd+Control+V | Paste Special | missing: no Paste Special dialog |
| F1, Shift+F10, Alt / F10 KeyTips, Shift+F8 | thesaurus, Cmd+Control+Z | help, context menu from the keyboard, KeyTips, Copilot; thesaurus, drawing mode | missing |
| Alt+Shift+Up / Down, Alt+Shift+1 / + / - | — | move a paragraph, outline levels and collapse | missing |
| — | Cmd+Shift+P, Cmd+Option+2 / 3, Cmd+Control+F | template gallery (Cmd+Shift+P is command search), handout / notes masters, full screen | missing |

During a slide show:

| Shortcut (Windows) | Mac | Action | Status |
|---|---|---|---|
| N, Enter, Page Down, Right, Down, Space / P, Page Up, Left, Up, Backspace, number+Enter, Home, End | same; Fn+Left / Right | move between slides | present |
| B or . / W or , / Esc or - | Shift+B / Shift+W | black and white screens, end | present |
| Ctrl+P, Ctrl+A, E | Cmd+P, Cmd+A, Shift+E | pen, arrow, erase the ink | present |
| H | same | the next slide when it's hidden | **added** |
| — | Cmd+. | end the show | **added** |
| Ctrl+H, Ctrl+A | Cmd+I, Control+H, Cmd+A, Cmd+U | hide / show the pointer; hide it when idle | **added** |
| Alt+P, Ctrl+Space, Alt+Q | same | play/pause, stop the slide's media | **added** |
| Alt+Shift+Page Down / Up, Alt+Shift+Right / Left | same | seek 3 s; 0.25 s and pause | **added** |
| Ctrl+L, Ctrl+E, Ctrl+M | Cmd+L | laser, eraser, show/hide ink | missing (open PR #161) |
| Ctrl+S, Ctrl+T, Shift+F10, F1, Tab | same | all slides, taskbar, menu, help, hotspots | missing |
| Alt+Up / Down, Alt+U, Alt+Home / End, Alt+J, Alt+X | same | volume, mute, bookmarks, captions, cameo | missing: no volume or bookmark API |
| T, O, M, R; Presenter View F6, Ctrl+arrows | same | rehearse timings; presenter regions | missing |

Off the Mac, AltGr can arrive as Ctrl+Alt: a Ctrl+Alt key that also types a character is treated
as text, so AltGr symbols never trigger a shortcut, and Ctrl+Alt+5 never fires while editing.

## Visual fidelity

Colours, spacing, ribbon card and tab geometry follow the observations in
`plan/powerpoint/01-observed-ui.md`; icons are original (~240, drawn in code or Lucide) and must not
imitate Microsoft's. No pixel comparison against PowerPoint screenshots has been run since
2026-10-07. Cross-app icon consistency is open (#73, #56).

## Estimates

| Work | Hours |
|---|---|
| Native macOS menu bar from `menu_tree()`, filled out to PowerPoint's menu items | 3–5 |
| Format Shape pane depth | 6–10 |
| Missing dialogs (Font, Bullets and Numbering, Find/Replace, Replace Fonts, Action Settings, Print, Insert Object) | 8–14 |
| Start screen, window chrome, system appearance, palette fixes (#17, #41, #71, #76, #79) | 4–7 |
| Grid Options, edit points UI, crop depth | 4–7 |
| Multiple windows | 3–5 |
| A screenshot comparison pass on the most-used tabs | 2–4 |
| **Total** | **~30–52** |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-12 | minor | Shortcut audit: 150 of 192 PowerPoint shortcuts (68 before); F6 pane cycling; platform keys |
| 2026-10-10 | major | Created: shell, panes and dialogs, direct manipulation, shortcuts and visual fidelity against PowerPoint 16.113 |
