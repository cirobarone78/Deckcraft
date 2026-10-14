//! The presence projection (CraftHub spec §6.1, §18.4 row 3): what DeckCraft's live UI state
//! looks like on the wire, and what a peer's state renders as here. Presence is ephemeral — it
//! is never stored, never journalled, never saved (§2.7's rule, kept verbatim in v3).
//!
//! DeckCraft's row in the per-app projection table is **the active slide** (the `page` field):
//! a peer on another slide renders only in the slide sorter's collaborator badges, and the
//! spotlight signal (`hub.follow`) drives the live presentation — the presenter's slide
//! changes carry everyone along, exactly what §18.4 names for this app. DeckCraft has no
//! timeline, no tracks and no generation jobs — those fields belong to the siblings and stay
//! `None` here.

use craft_collab::adapter::UiState;

/// Everything the DeckCraft UI needs to render one peer: the projection is a two-way street —
/// the same fields go out and come back.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PeerView {
    /// The slide the peer is on (the sorter badge; the canvas only shows same-slide peers).
    pub slide: Option<u64>,
    /// The peer's selection (shape outlines, never edit locks — §6.7 does locks).
    pub selection: Vec<craft_collab::ids::NodeId>,
    /// The peer's viewport of that slide, offered to the follow mode.
    pub viewport: Option<(f64, f64, f64, f64)>,
    /// The peer is typing in this shape's text (caret and selection in characters).
    pub editing_text: Option<(craft_collab::ids::NodeId, usize, Option<usize>)>,
}

impl PeerView {
    /// Project DeckCraft's UI state into the fields the hub's `UiState` carries.
    #[must_use]
    pub fn from_ui(ui: &UiState) -> Self {
        Self { slide: ui.page, selection: ui.selection.clone(), viewport: ui.viewport, editing_text: ui.editing_text }
    }

    /// Whether this peer shows anything on `slide` at all.
    #[must_use]
    pub fn on_slide(&self, slide: u64) -> bool {
        self.slide == Some(slide)
    }
}
