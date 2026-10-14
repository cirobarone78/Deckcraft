# The collaboration behavior suite (DeckCraft's projections)

> The shared suite every Craft app runs before its CraftHub integration is enabled (the CraftHub
> specification §18.9): each row is one automated test with a written scenario, in the family's
> behaviour-suite tradition. This file is DeckCraft's copy of the shared catalog: the scenarios
> are identical in every app, and the *projections* column is where each app writes what the
> scenario means for its own UI.

## The scenarios

| # | Scenario | The guarantee under test | DeckCraft's projection |
|---|----------|--------------------------|------------------------|
| 1 | Join by code, join by link | One argument that recognizes itself; the file opens in its own tab, other sessions untouched | A joined deck opens as its own tab; the slide sorter and status bar stay per-document |
| 2 | Concurrent editing converges | Snapshot-hash equality after arbitrary interleavings, drops and reconnects (per seed) | Shape properties merge last-writer-wins per property; a group's children merge as their own objects |
| 3 | Read-only, with its reason | The reason is shown (view link, role without edit, region lock), never just a badge | The mode bar shows "view link" / "viewer role" / "locked region" with the locked shapes tinted |
| 4 | Batch refused with fixes | The server's refusal carries the fixes; the client applies them and lands on the server state | A refused shape creation snaps back with a toast explaining why |
| 5 | Per-user undo | My steps are mine alone; later writes by others survive my undo | Undo after a peer's edit undoes only my edit; the peer's shape stays where they put it |
| 6 | Offline, then resume as the same client | The recovery copy reattaches; unsent changes replay or land as a "(recovered copy)" beside the file | The unsent shape edit replays on reconnect; a conflicting copy appears in Home ▸ Online |
| 7 | Cursor chat and its bounds | ≤100 characters, ephemeral, never enters the document or the journal | The bubble renders at the peer's cursor on the shared slide, then is gone |
| 8 | Spotlight and follow | Follow tracks view and page; any view movement of mine stops the follow | **The live presentation rides the spotlight signal** (`hub.follow` — §18.4 names this for DeckCraft): the presenter's slide changes carry the audience along |
| 9 | Presence and per-app projections | Ephemeral, throttled to change-only, bounded sizes | **The active slide** is the projection (`page`): a peer on another slide renders only in the sorter's collaborator badges |
| 10 | Region locks | Locks sync as document properties; locked regions tint; the reason is shown | Locked shapes tint; editing them is refused with "locked by <name>" |
| 11 | Contribution, review, merge | The branch lives inside the live session; merge is one undo step; each side's changes show | The review pane shows both sides' property diffs per shape, Inspect-style |
| 12 | Comments and their anchors | Anchor kinds per app; comments are entries with ids and order | Slide comments anchor to a point on the slide (DeckCraft's native form); they ride as one slide property for the first cut — splitting per entry is the follow-up |
| 13 | Background file notifications | Delivered when the tab is not front; bounded, dismissible | The tab badge shows "3 new" for a backgrounded shared deck |
| 14 | Closing a file ends its session alone | Other sessions keep syncing behind the front tab | Closing one shared tab never drops another deck's peers |
| 15 | The revert guard | `file.revert` is refused while its document is in a live session (including reconnect and pre-welcome) | The toast says "leave the session before reverting"; the deck and its previews are intact |

## DeckCraft's adapter surface (what the suite drives)

* **Property tables** — the 21 synchronized `Shape` fields and the 12 synchronized `Slide`
  fields, each with the compile-time guard. A group's `kind` property carries its payload
  **without its children** — the children sync as their own objects (double-writing them
  duplicates the children on every remote apply; the tests pin this).
* **The tree** — slides under the root, shapes under slides, shapes under groups (recursively),
  ordered by fractional sibling keys kept beside the presentation in the adapter.
* **The id constraint** — DeckCraft's `u32` shape/slide ids have no room for the protocol's
  64-bit client-space ids, so the adapter owns a session translation map: file objects ride as
  space-0 ids, and a shape created during a live session gets a fresh local id with the hub id
  recorded beside it. Widening `ShapeId` to `u64` (the file-format decision) retires the map.
* **The document level** — `slideSize`, `show`, `sections`, the masters and the rest sync as
  whole-value LWW scalars. `media` never syncs as data: media bytes ride the asset channel
  (spec §3.5).
* **Commands** — the unified `hub.*` catalog with the v2 `collab.*` twins
  (`deckcraft-hub::commands`): same names in every Craft app.
* **Deep links** — `deckcraft://join?<code\|token>&server=<url>` (`deckcraft-hub::deeplink`).

## The enable rule

No app is enabled before its protocol core passes its simulation tests in CI for a full week
(nightly, fixed seeds) — and enabling happens through the app's own `crafthub` feature flag,
never through the core, so a regression is one flag flip away from off (§18.4).
