//! # deckcraft-hub — DeckCraft's CraftHub client path
//!
//! The [`HubAdapter`] that maps DeckCraft's [`Presentation`](deckcraft_model::Presentation)
//! onto `craft-collab`'s generalized object/property model (the CraftHub specification §3.6
//! phase C, §18.4 row 3). DeckCraft is "frames like FigCraft": slides hold shapes, groups hold
//! shapes, every shape carries a persistent id — so the tree maps directly:
//!
//! * the **root** holds one `slide` object per slide; the presentation-level scalars sync as
//!   whole-value LWW (`slideSize`, `show`, `sections`, …). `media` never syncs as data: media
//!   bytes ride the asset channel (spec §3.5). The masters sync as one whole-list value for
//!   the first cut — master edits during a live session are rare, and splitting them per
//!   entry is the documented follow-up.
//! * a **slide** holds shape objects of every kind; its own fields (`layout`, `background`,
//!   `transition`, `notes`, speaker-note comments, …) sync as properties. Slide comments ride
//!   as one property on the slide for the first cut (they anchor to a point on the slide, not
//!   a shape — splitting them per entry is the follow-up).
//! * a **group** holds shapes, recursively; the child coordinate space rides inside the
//!   group's `kind` property.
//! * the sibling position keys live beside the presentation in the adapter, exactly where the
//!   figcraft pilot keeps them: assigned by current order at first sync, kept by the longest
//!   rising run under edits (an insertion in the middle costs one key — tested).
//!
//! ## The id constraint (the honest limitation)
//!
//! DeckCraft's shape and slide ids are `u32` with a file-wide `next_id` — there is no room
//! for the 64-bit client-space ids the protocol needs for concurrently created objects. The
//! adapter therefore owns a **session translation map**: existing file objects ride as space-0
//! ids (both clients compute the same ids for the same file), and a shape created during a
//! live session gets a fresh local `u32` from `next_id` with the hub id recorded beside it —
//! every later `Set`/`Delete` for that object translates through the map, in both directions
//! (`apply` and `diff`). The document's id space stays untouched; the session's ids stay
//! collision-free. Widening `ShapeId` to `u64` (the file-format decision) retires the map —
//! noted in the PR, not smuggled in.
//!
//! What deliberately does **not** live here: networking, sessions, the UI. The session wiring
//! (a `NetLink` speaking protocol v3, installed by the engine behind the app's `crafthub`
//! feature flag) lands as the follow-up once this contract is reviewed; nothing here changes
//! any user-visible behavior. The presence projection is the adoption table's row for
//! DeckCraft: **the active slide** (`page`) — and the live presentation rides the spotlight
//! signal (`hub.follow`), which §18.4 names explicitly for this app.

pub mod commands;
pub mod deeplink;
pub mod presence;
pub mod props;
pub mod schema;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use craft_collab::adapter::{ApplyReport, DocumentView, HubAdapter, UiState, ViewNode};
use craft_collab::change::Change;
use craft_collab::frac;
use craft_collab::ids::NodeId as HubId;
use craft_collab::value::Value;
use deckcraft_model::{Presentation, Shape, ShapeKind, Slide};

/// The root every slide hangs under (space 0, counter 0 — never a real id).
pub const ROOT: HubId = HubId(0);

/// The object kinds: the slide plus the `ShapeKind` discriminants.
pub const KINDS: &[&str] = &["slide", "shape", "picture", "group", "connector", "table", "chart", "media", "ink", "model3d", "opaque"];

/// The kinds that accept children.
pub const CONTAINERS: &[&str] = &["slide", "group"];

/// The adapter: sibling keys, the session id translation, and the presence projection.
pub struct DeckcraftHub {
    schema: schema::DeckSchema,
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// Sibling position keys by hub id: slides under the root, shapes under their parent.
    positions: BTreeMap<HubId, String>,
    /// The session translation: a hub id minted in a client space → the local `u32` id it
    /// landed on (see the module docs — the id constraint).
    local_ids: BTreeMap<HubId, u32>,
    /// The reverse translation: a local id → the hub id it belongs to.
    hub_ids: BTreeMap<u32, HubId>,
}

impl DeckcraftHub {
    /// A fresh adapter with the default limits and empty state.
    #[must_use]
    pub fn new() -> Self {
        Self { schema: schema::DeckSchema::default(), inner: Mutex::new(Inner::default()) }
    }

    /// The sibling key of `id`, if one has been assigned.
    #[must_use]
    pub fn position_key(&self, id: HubId) -> Option<String> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).positions.get(&id).cloned()
    }

    /// The hub id of a local slide/shape id (identity for file objects; the translation for
    /// session-created ones).
    #[must_use]
    pub fn hub_id_of(&self, local: u32) -> HubId {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).hub_ids.get(&local).copied().unwrap_or(HubId(u64::from(local)))
    }

    /// The local id of a hub id (identity for space-0 file ids; the translation otherwise).
    #[must_use]
    pub fn local_id_of(&self, hub: HubId) -> u32 {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).local_ids.get(&hub).copied().unwrap_or(hub.0 as u32)
    }

    /// The kind string of a shape (the discriminant; the payload syncs inside `kind`).
    #[must_use]
    pub fn kind_name(kind: &ShapeKind) -> &'static str {
        match kind {
            ShapeKind::Shape => "shape",
            ShapeKind::Picture { .. } => "picture",
            ShapeKind::Group { .. } => "group",
            ShapeKind::Connector { .. } => "connector",
            ShapeKind::Table(_) => "table",
            ShapeKind::Chart(_) => "chart",
            ShapeKind::Media(_) => "media",
            ShapeKind::Ink { .. } => "ink",
            ShapeKind::Model3d { .. } => "model3d",
            ShapeKind::Opaque { .. } => "opaque",
        }
    }

    /// A default shape kind for a kind string (used when a `Create` arrives without a `kind`
    /// property — the diff always sends one, so this only feeds lenient applies and tests).
    #[must_use]
    pub fn default_kind(name: &str) -> Option<ShapeKind> {
        let kind = match name {
            "shape" => ShapeKind::Shape,
            "picture" => return None, // a picture always carries its fill inside the kind
            "group" => ShapeKind::Group { children: Vec::new(), child: Default::default() },
            "connector" => ShapeKind::Connector { start: None, end: None },
            "table" => ShapeKind::Table(Default::default()),
            "chart" => ShapeKind::Chart(Box::default()),
            "media" => return None, // a media clip always carries its ids inside the kind
            "ink" => ShapeKind::Ink { strokes: Vec::new() },
            "model3d" => return None, // a 3-D model always carries its media ids
            "opaque" => return None,  // opaque XML is never defaulted
            _ => return None,
        };
        Some(kind)
    }

    /// First-sync rule: children without keys take them by current order.
    fn assign_missing_keys(&self, doc: &Presentation) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let missing: Vec<HubId> = doc.slides.iter().map(|s| HubId(u64::from(s.id.0))).filter(|id| !inner.positions.contains_key(id)).collect();
        if !missing.is_empty()
            && let Ok(keys) = frac::n_between(None, None, missing.len())
        {
            for (id, key) in missing.into_iter().zip(keys) {
                inner.positions.insert(id, key);
            }
        }
        for slide in &doc.slides {
            let missing: Vec<HubId> =
                slide.shapes.iter().map(|sh| hub_id_locked(&inner, sh.id.0)).filter(|id| !inner.positions.contains_key(id)).collect();
            if !missing.is_empty()
                && let Ok(keys) = frac::n_between(None, None, missing.len())
            {
                for (id, key) in missing.into_iter().zip(keys) {
                    inner.positions.insert(id, key);
                }
            }
        }
    }

    /// Record `key` for `id`.
    fn set_key(&self, id: HubId, key: String) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).positions.insert(id, key);
    }

    /// Record the session translation for a created object.
    fn translate(&self, hub: HubId, local: u32) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.local_ids.insert(hub, local);
        inner.hub_ids.insert(local, hub);
    }

    /// The insertion index for `(key, id)` among `siblings` ordered by `(key, id)`.
    fn insertion_index(keys: &BTreeMap<HubId, String>, siblings: &[HubId], id: HubId, key: &str) -> usize {
        siblings.iter().filter(|s| (keys.get(*s).cloned().unwrap_or_default(), **s) < (key.to_owned(), id)).count()
    }
}

impl Default for DeckcraftHub {
    fn default() -> Self {
        Self::new()
    }
}

/// The hub id of a local id under a held lock.
fn hub_id_locked(inner: &Inner, local: u32) -> HubId {
    inner.hub_ids.get(&local).copied().unwrap_or(HubId(u64::from(local)))
}

/// Walk a slide's shape tree, parents-first, visiting every shape (a borrow of the whole
/// slide outlives the walk, so one lifetime covers the shapes and their group parents).
fn walk_shapes<'a>(shapes: &'a [Shape], out: &mut Vec<(&'a Shape, Option<&'a Shape>)>) {
    for shape in shapes {
        out.push((shape, None));
        if let ShapeKind::Group { children, .. } = &shape.kind {
            walk_group(shape, children, out);
        }
    }
}

/// Walk a group's children (recording their parent for the view).
fn walk_group<'a>(parent: &'a Shape, children: &'a [Shape], out: &mut Vec<(&'a Shape, Option<&'a Shape>)>) {
    for shape in children {
        out.push((shape, Some(parent)));
        if let ShapeKind::Group { children, .. } = &shape.kind {
            walk_group(shape, children, out);
        }
    }
}

/// Find a shape by local id anywhere in the tree (the slide it lives on, and a mutable path).
fn find_shape_mut(doc: &mut Presentation, id: u32) -> Option<&mut Shape> {
    for slide in &mut doc.slides {
        // Arc::make_mut through the slide list; the persistent sharing keeps the diff
        // edit-sized even though the walk is whole-tree.
        let slide = Arc::make_mut(slide);
        if let Some(found) = in_shapes_mut(&mut slide.shapes, id) {
            return Some(found);
        }
    }
    None
}

/// Find a shape by id in a shape list, descending into groups.
fn in_shapes_mut(shapes: &mut [Shape], id: u32) -> Option<&mut Shape> {
    for shape in shapes.iter_mut() {
        if shape.id.0 == id {
            return Some(shape);
        }
        if let ShapeKind::Group { children, .. } = &mut shape.kind
            && let Some(found) = in_shapes_mut(children, id)
        {
            return Some(found);
        }
    }
    None
}

/// Remove a shape by local id from its owning container (the slide or group that holds it).
fn remove_shape(doc: &mut Presentation, id: u32) -> Option<Shape> {
    fn from_shapes(shapes: &mut Vec<Shape>, id: u32) -> Option<Shape> {
        if let Some(at) = shapes.iter().position(|s| s.id.0 == id) {
            return Some(shapes.remove(at));
        }
        for shape in shapes.iter_mut() {
            if let ShapeKind::Group { children, .. } = &mut shape.kind
                && let Some(found) = from_shapes(children, id)
            {
                return Some(found);
            }
        }
        None
    }
    for slide in &mut doc.slides {
        let slide = Arc::make_mut(slide);
        if let Some(found) = from_shapes(&mut slide.shapes, id) {
            return Some(found);
        }
    }
    None
}

/// Insert a shape into a parent (a slide or a group) at `at` (clamped).
fn insert_shape(doc: &mut Presentation, parent: HubId, shape: Shape, at: usize) -> bool {
    if parent == ROOT {
        return false; // slides attach via insert_slide, not here
    }
    let local_parent = parent.0 as u32;
    for slide in &mut doc.slides {
        if slide.id.0 == local_parent {
            let slide = Arc::make_mut(slide);
            let at = at.min(slide.shapes.len());
            slide.shapes.insert(at, shape);
            return true;
        }
    }
    if let Some(group) = find_shape_mut(doc, local_parent)
        && let ShapeKind::Group { children, .. } = &mut group.kind
    {
        let at = at.min(children.len());
        children.insert(at, shape);
        return true;
    }
    false
}

/// The children of `parent` as hub ids, in document order.
fn children_of(inner: &Inner, doc: &Presentation, parent: HubId) -> Vec<HubId> {
    if parent == ROOT {
        doc.slides.iter().map(|s| HubId(u64::from(s.id.0))).collect()
    } else {
        let local = parent.0 as u32;
        let mut out = Vec::new();
        for slide in &doc.slides {
            if slide.id.0 == local {
                out.extend(slide.shapes.iter().map(|sh| hub_id_locked(inner, sh.id.0)));
                return out;
            }
        }
        if let Some(shape) = find_shape_ref(doc, local)
            && let ShapeKind::Group { children, .. } = &shape.kind
        {
            out.extend(children.iter().map(|sh| hub_id_locked(inner, sh.id.0)));
        }
        out
    }
}

/// Find a shape by local id (read-only).
fn find_shape_ref(doc: &Presentation, id: u32) -> Option<&Shape> {
    fn in_shapes(shapes: &[Shape], id: u32) -> Option<&Shape> {
        for shape in shapes {
            if shape.id.0 == id {
                return Some(shape);
            }
            if let ShapeKind::Group { children, .. } = &shape.kind
                && let Some(found) = in_shapes(children, id)
            {
                return Some(found);
            }
        }
        None
    }
    for slide in &doc.slides {
        if let Some(found) = in_shapes(&slide.shapes, id) {
            return Some(found);
        }
    }
    None
}

impl HubAdapter for DeckcraftHub {
    type Doc = Presentation;

    fn view(&self, doc: &Presentation) -> DocumentView {
        self.assign_missing_keys(doc);
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let key = |id: &HubId| inner.positions.get(id).cloned().unwrap_or_default();
        let mut nodes = Vec::new();
        for slide in &doc.slides {
            let sid = HubId(u64::from(slide.id.0));
            nodes.push(ViewNode {
                id: sid,
                kind: Arc::from("slide"),
                parent: ROOT,
                position: key(&sid),
                props: props::SLIDE_PROPS.iter().filter_map(|p| props::slide_value(slide, p).map(|v| (Arc::from(*p), v))).collect(),
            });
            let mut walked = Vec::new();
            walk_shapes(&slide.shapes, &mut walked);
            for (shape, group_parent) in walked {
                let hid = hub_id_locked(&inner, shape.id.0);
                let parent = group_parent.map_or(sid, |p| hub_id_locked(&inner, p.id.0));
                nodes.push(ViewNode {
                    id: hid,
                    kind: Arc::from(Self::kind_name(&shape.kind)),
                    parent,
                    position: key(&hid),
                    props: props::SHAPE_PROPS.iter().filter_map(|p| props::shape_value(shape, p).map(|v| (Arc::from(*p), v))).collect(),
                });
            }
        }
        let doc_props = props::DOC_PROPS.iter().filter_map(|p| props::doc_prop_value(doc, p).map(|v| ((*p).to_owned(), v))).collect();
        DocumentView { nodes, entries: Vec::new(), doc: doc_props }
    }

    fn apply(&self, doc: &mut Presentation, changes: &[Change]) -> ApplyReport {
        let mut report = ApplyReport::default();
        for change in changes {
            match change {
                Change::Create { id, kind, parent, position, props } => {
                    if kind.as_ref() == "slide" {
                        // A slide: local id from next_id (or the file id when space 0 fits).
                        let local = if id.space() == 0 { id.0 as u32 } else { doc.next_id };
                        if doc.slides.iter().any(|s| s.id.0 == local) && id.space() != 0 {
                            // A translated id that already landed: idempotent skip.
                            report.skipped += 1;
                            continue;
                        }
                        let mut slide = Slide { id: deckcraft_model::SlideId(local), ..Default::default() };
                        for (p, v) in props {
                            match props::set_slide(&mut slide, p, v) {
                                Some(Ok(())) => report.applied += 1,
                                Some(Err(_)) | None => report.skipped += 1,
                            }
                        }
                        self.set_key(*id, position.clone());
                        if id.space() != 0 {
                            self.translate(*id, local);
                            doc.next_id = doc.next_id.max(local + 1);
                        }
                        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
                        let at =
                            Self::insertion_index(&inner.positions, &children_of(&inner, doc, ROOT).into_iter().collect::<Vec<_>>(), *id, position);
                        drop(inner);
                        let at = at.min(doc.slides.len());
                        doc.slides.insert(at, Arc::new(slide));
                        report.applied += 1;
                    } else {
                        // A shape: the full kind rides the `kind` property.
                        let node_kind = props
                            .iter()
                            .find(|(p, _)| p.as_ref() == "kind")
                            .and_then(|(_, v)| serde_json::from_value::<ShapeKind>(v.to_json()).ok())
                            .or_else(|| Self::default_kind(kind));
                        let Some(node_kind) = node_kind else {
                            report.skipped += 1;
                            continue;
                        };
                        let local = if id.space() == 0 { id.0 as u32 } else { doc.next_id };
                        if find_shape_ref(doc, local).is_some() && id.space() != 0 {
                            report.skipped += 1;
                            continue;
                        }
                        let mut shape = Shape { id: deckcraft_model::ShapeId(local), kind: node_kind, ..Default::default() };
                        for (p, v) in props {
                            if p.as_ref() == "parent" || p.as_ref() == "position" {
                                continue;
                            }
                            match props::set_shape(&mut shape, p, v) {
                                Some(Ok(())) => report.applied += 1,
                                Some(Err(_)) | None => report.skipped += 1,
                            }
                        }
                        self.set_key(*id, position.clone());
                        if id.space() != 0 {
                            self.translate(*id, local);
                            doc.next_id = doc.next_id.max(local + 1);
                        }
                        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
                        let siblings = children_of(&inner, doc, *parent);
                        let at = Self::insertion_index(&inner.positions, &siblings, *id, position);
                        drop(inner);
                        if insert_shape(doc, *parent, shape, at) {
                            report.applied += 1;
                        } else {
                            report.skipped += 1;
                        }
                    }
                }
                Change::Set { id, prop, value } => {
                    let local = self.local_id_of(*id);
                    if prop.as_ref() == "position" {
                        if let Value::Json(serde_json::Value::String(key)) = value {
                            self.set_key(*id, key.clone());
                            report.applied += 1; // the order lands with the next full reorder
                        } else {
                            report.skipped += 1;
                        }
                    } else if prop.as_ref() == "parent" {
                        report.skipped += 1; // reparenting a shape is a move + key write; the
                    // session wiring composes them (documented)
                    } else if find_shape_ref(doc, local).is_some() {
                        let Some(shape) = find_shape_mut(doc, local) else {
                            report.skipped += 1;
                            continue;
                        };
                        match props::set_shape(shape, prop, value) {
                            Some(Ok(())) => report.applied += 1,
                            Some(Err(_)) | None => report.skipped += 1,
                        }
                    } else if let Some(slide) = doc.slides.iter_mut().find(|s| s.id.0 == local).map(Arc::make_mut) {
                        match props::set_slide(slide, prop, value) {
                            Some(Ok(())) => report.applied += 1,
                            Some(Err(_)) | None => report.skipped += 1,
                        }
                    } else {
                        report.skipped += 1;
                    }
                }
                Change::Delete { id } => {
                    let local = self.local_id_of(*id);
                    if let Some(at) = doc.slides.iter().position(|s| s.id.0 == local) {
                        let _ = doc.slides.remove(at);
                        report.applied += 1;
                    } else if remove_shape(doc, local).is_some() {
                        report.applied += 1;
                    }
                    // Deleting a deleted object does nothing (v2 tolerance, kept).
                }
                Change::SetDoc { prop, value } => match props::set_doc_prop(doc, prop, value) {
                    Some(Ok(())) => report.applied += 1,
                    Some(Err(_)) | None => report.skipped += 1,
                },
                Change::Put { .. } | Change::Remove { .. } => {
                    report.skipped += 1; // no document-level entry lists in the first cut
                }
            }
        }
        report
    }

    fn diff(&self, before: &Presentation, after: &Presentation) -> Vec<Change> {
        let mut sets = Vec::new();
        let mut creates = Vec::new();

        // Slides: property sets and creations, in slide order (parents first by construction).
        for slide in &after.slides {
            let hid = self.hub_id_of(slide.id.0);
            match before.slides.iter().find(|s| s.id.0 == slide.id.0) {
                Some(b) if Arc::ptr_eq(b, slide) => {}
                Some(b) => {
                    for prop in props::SLIDE_PROPS {
                        if props::slide_differs(b, slide, prop).unwrap_or(false)
                            && let Some(v) = props::slide_value(slide, prop)
                        {
                            sets.push(Change::Set { id: hid, prop: Arc::from(*prop), value: v });
                        }
                    }
                }
                None => creates.push(Change::Create {
                    id: hid,
                    kind: Arc::from("slide"),
                    parent: ROOT,
                    position: String::new(),
                    props: props::SLIDE_PROPS.iter().filter_map(|p| props::slide_value(slide, p).map(|v| (Arc::from(*p), v))).collect(),
                }),
            }
        }

        // Shapes: property sets and creations, walking each after-slide parents-first.
        for slide in &after.slides {
            let sid = self.hub_id_of(slide.id.0);
            let bslide = before.slides.iter().find(|s| s.id.0 == slide.id.0);
            let mut walked = Vec::new();
            walk_shapes(&slide.shapes, &mut walked);
            for (shape, group_parent) in walked {
                let hid = self.hub_id_of(shape.id.0);
                let parent = group_parent.map_or(sid, |p| self.hub_id_of(p.id.0));
                let before_shape = bslide.and_then(|s| find_shape_ref_in(s, shape.id.0));
                match before_shape {
                    Some(b) if b == shape => {}
                    Some(b) => {
                        for prop in props::SHAPE_PROPS {
                            if props::shape_differs(b, shape, prop).unwrap_or(false)
                                && let Some(v) = props::shape_value(shape, prop)
                            {
                                sets.push(Change::Set { id: hid, prop: Arc::from(*prop), value: v });
                            }
                        }
                    }
                    None => creates.push(Change::Create {
                        id: hid,
                        kind: Arc::from(Self::kind_name(&shape.kind)),
                        parent,
                        position: String::new(),
                        props: props::SHAPE_PROPS.iter().filter_map(|p| props::shape_value(shape, p).map(|v| (Arc::from(*p), v))).collect(),
                    }),
                }
            }
        }

        // Deletions: one Delete per subtree root gone from the after tree — a group's children
        // leave with it, so a shape whose group parent is also gone needs no Delete of its own.
        let mut deletes = Vec::new();
        for slide in &before.slides {
            let after_slide = after.slides.iter().find(|s| s.id.0 == slide.id.0);
            let Some(after_slide) = after_slide else {
                deletes.push(Change::Delete { id: self.hub_id_of(slide.id.0) });
                continue;
            };
            let mut walked = Vec::new();
            walk_shapes(&slide.shapes, &mut walked);
            let gone: std::collections::HashSet<u32> =
                walked.iter().map(|(shape, _)| shape.id.0).filter(|id| find_shape_ref_in(after_slide, *id).is_none()).collect();
            for (shape, group_parent) in walked {
                let id = shape.id.0;
                if !gone.contains(&id) {
                    continue;
                }
                let parent_also_gone = group_parent.is_some_and(|p| gone.contains(&p.id.0));
                if !parent_also_gone {
                    deletes.push(Change::Delete { id: self.hub_id_of(id) });
                }
            }
        }

        // Order: assign keys per parent whose child list changed (the LIS discipline — an
        // insertion in the middle costs one key).
        let mut order_changes = Vec::new();
        {
            let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            let mut parents: Vec<HubId> = vec![ROOT];
            for slide in before.slides.iter().chain(after.slides.iter()) {
                let id = HubId(u64::from(slide.id.0));
                if !parents.contains(&id) {
                    parents.push(id);
                }
            }
            for parent in parents {
                let old: Vec<HubId> = children_of(&inner, before, parent);
                let new: Vec<HubId> = children_of(&inner, after, parent);
                if old == new {
                    continue;
                }
                // First-sync: the before children without keys take them by order.
                let unkeyed: Vec<HubId> = old.iter().copied().filter(|c| !inner.positions.contains_key(c)).collect();
                if !unkeyed.is_empty()
                    && let Ok(fresh) = frac::n_between(None, None, unkeyed.len())
                {
                    for (id, key) in unkeyed.into_iter().zip(fresh) {
                        inner.positions.insert(id, key);
                    }
                }
                // The longest strictly-rising run over the new order keeps its keys.
                let n = new.len();
                let mut len = vec![0usize; n];
                let mut prev = vec![usize::MAX; n];
                let mut best_end = None;
                let mut best_len = 0usize;
                for i in 0..n {
                    if inner.positions.contains_key(&new[i]) && old.contains(&new[i]) {
                        len[i] = 1;
                        for j in 0..i {
                            if inner.positions.contains_key(&new[j])
                                && old.contains(&new[j])
                                && inner.positions.get(&new[j]).is_some_and(|a| inner.positions.get(&new[i]).is_some_and(|b| a < b))
                                && len[j] + 1 > len[i]
                            {
                                len[i] = len[j] + 1;
                                prev[i] = j;
                            }
                        }
                        if len[i] > best_len {
                            best_len = len[i];
                            best_end = Some(i);
                        }
                    }
                }
                let mut kept = vec![false; n];
                let mut walk = best_end;
                while let Some(i) = walk {
                    kept[i] = true;
                    walk = if prev[i] == usize::MAX { None } else { Some(prev[i]) };
                }
                let mut last_kept: Option<String> = None;
                let mut pending: Vec<HubId> = Vec::new();
                for (i, c) in new.iter().enumerate() {
                    if kept[i] {
                        let next = inner.positions.get(c).cloned();
                        flush_keys(&mut pending, last_kept.as_deref(), next.as_deref(), &mut inner, &mut order_changes);
                        last_kept = next;
                    } else {
                        pending.push(*c);
                    }
                }
                flush_keys(&mut pending, last_kept.as_deref(), None, &mut inner, &mut order_changes);
            }
        }

        // The creations' keys, now that the order pass has assigned them.
        {
            let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            for create in &mut creates {
                if let Change::Create { id, position, .. } = create {
                    *position = inner.positions.get(id).cloned().unwrap_or_default();
                }
            }
        }

        // Document-level scalars.
        let mut doc_level = Vec::new();
        for prop in props::DOC_PROPS {
            if props::doc_prop_value(before, prop) != props::doc_prop_value(after, prop)
                && let Some(v) = props::doc_prop_value(after, prop)
            {
                doc_level.push(Change::SetDoc { prop: Arc::from(*prop), value: v });
            }
        }

        // Batch order (§2.3): field writes, creations (parents first), order, deletes, doc level.
        let mut batch = sets;
        batch.append(&mut creates);
        batch.append(&mut order_changes);
        batch.extend(deletes);
        batch.append(&mut doc_level);
        batch
    }

    fn presence(&self, doc: &Presentation, ui: &UiState) -> Option<craft_collab::presence::PresenceMsg> {
        // The adoption table's row for DeckCraft: the active slide is the projection, and the
        // live presentation rides the spotlight signal (hub.follow).
        let page = ui.page.or_else(|| doc.slides.first().map(|s| u64::from(s.id.0)))?;
        Some(craft_collab::presence::PresenceMsg {
            cursor: ui.cursor,
            page: Some(page),
            selection: ui.selection.clone(),
            viewport: ui.viewport,
            editing_text: ui.editing_text,
            playhead: None,    // DeckCraft has no timeline
            track_range: None, // DeckCraft has no tracks
            generating: None,  // DeckCraft runs no generation jobs
        })
    }

    fn schema(&self) -> &dyn craft_collab::adapter::DocSchema {
        &self.schema
    }
}

/// Flush a run of pending keys between `prev` and `next` (the LIS discipline's helper).
fn flush_keys(pending: &mut Vec<HubId>, prev: Option<&str>, next: Option<&str>, inner: &mut Inner, out: &mut Vec<Change>) {
    if pending.is_empty() {
        return;
    }
    let count = pending.len();
    if let Ok(fresh) = frac::n_between(prev, next, count) {
        for (id, key) in pending.drain(..).zip(fresh) {
            inner.positions.insert(id, key.clone());
            out.push(Change::Set { id, prop: Arc::from("position"), value: Value::Json(serde_json::Value::String(key)) });
        }
    }
}

/// Find a shape by local id within one slide (read-only helper for the diff).
fn find_shape_ref_in(slide: &Slide, id: u32) -> Option<&Shape> {
    fn in_shapes(shapes: &[Shape], id: u32) -> Option<&Shape> {
        for shape in shapes {
            if shape.id.0 == id {
                return Some(shape);
            }
            if let ShapeKind::Group { children, .. } = &shape.kind
                && let Some(found) = in_shapes(children, id)
            {
                return Some(found);
            }
        }
        None
    }
    in_shapes(&slide.shapes, id)
}
