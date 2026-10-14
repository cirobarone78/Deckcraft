//! The adapter's convergence suite (CraftHub spec §3.6 phase C: the ported tests, the family's
//! most valuable testing asset), over DeckCraft's real presentation: the roundtrip guarantee
//! with positions, the one-key insertion, the group tree, two-client convergence and the
//! tolerance rules.

// Tests may unwrap: a failing assertion is the point of a test (the family's clippy rule).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use craft_collab::adapter::HubAdapter;
use deckcraft_hub::DeckcraftHub;
use deckcraft_model::{Presentation, Shape, ShapeKind, Slide};

fn sample() -> Presentation {
    let mut doc = Presentation::default();
    let mut first = Slide { id: deckcraft_model::SlideId(1), name: "Title".into(), ..Default::default() };
    first.shapes.push(Shape { id: deckcraft_model::ShapeId(10), name: "Headline".into(), ..Default::default() });
    first.shapes.push(Shape { id: deckcraft_model::ShapeId(11), name: "Subtitle".into(), ..Default::default() });
    let group = Shape {
        id: deckcraft_model::ShapeId(12),
        name: "Group".into(),
        kind: ShapeKind::Group {
            children: vec![
                Shape { id: deckcraft_model::ShapeId(13), name: "Inside A".into(), ..Default::default() },
                Shape { id: deckcraft_model::ShapeId(14), name: "Inside B".into(), ..Default::default() },
            ],
            child: Default::default(),
        },
        ..Default::default()
    };
    first.shapes.push(group);
    doc.slides.push(std::sync::Arc::new(first));
    doc.slides.push(std::sync::Arc::new(Slide { id: deckcraft_model::SlideId(2), name: "Agenda".into(), ..Default::default() }));
    doc.next_id = 100;
    doc
}

fn assert_same_state(hub: &DeckcraftHub, a: &Presentation, b: &Presentation, what: &str) {
    let (va, vb) = (hub.view(a), hub.view(b));
    assert_eq!(va.nodes.len(), vb.nodes.len(), "{what}: node count");
    for (na, nb) in va.nodes.iter().zip(vb.nodes.iter()) {
        assert_eq!(na.id, nb.id, "{what}: node id");
        assert_eq!(na.kind, nb.kind, "{what}: kind of {na:?}");
        assert_eq!(na.parent, nb.parent, "{what}: parent of {na:?}");
        assert_eq!(na.position, nb.position, "{what}: position of {na:?}");
        assert_eq!(na.props, nb.props, "{what}: props of {na:?}");
    }
    assert_eq!(va.doc, vb.doc, "{what}: doc props");
}

/// The core promise, kept: applying the diff of two presentations to the first makes it the
/// second — edits, creations, deletions, group children, and the document level.
#[test]
fn apply_diff_roundtrip() {
    let hub = DeckcraftHub::new();
    let a = sample();
    let mut b = a.clone();

    // Field edits on a top-level shape and a shape inside the group.
    slide_mut(&mut b, 1, |s| {
        s.shapes[0].name = "Renamed headline".into();
    });
    slide_mut(&mut b, 1, |s| {
        if let ShapeKind::Group { children, .. } = &mut s.shapes[2].kind {
            children[0].hidden = true;
        }
    });
    // A creation inside the group, a deletion of a top-level shape, a slide-level edit.
    slide_mut(&mut b, 1, |s| {
        if let ShapeKind::Group { children, .. } = &mut s.shapes[2].kind {
            children.push(Shape { id: deckcraft_model::ShapeId(99), name: "Inside C".into(), ..Default::default() });
        }
    });
    slide_mut(&mut b, 1, |s| {
        s.shapes.remove(1);
    });
    slide_mut(&mut b, 2, |s| {
        s.notes = Default::default();
        s.name = "Agenda, revised".into();
    });
    // A new slide, and the document level.
    b.slides.push(std::sync::Arc::new(Slide { id: deckcraft_model::SlideId(3), name: "Close".into(), ..Default::default() }));
    b.first_slide_number = 7;

    let changes = hub.diff(&a, &b);
    assert!(!changes.is_empty());

    let mut applied = a.clone();
    hub.apply(&mut applied, &changes);
    assert_same_state(&hub, &applied, &b, "roundtrip");
    assert!(hub.diff(&applied, &b).is_empty(), "the applied state diffs clean against the target");
}

/// An insertion in the middle costs one position key — the siblings keep theirs.
#[test]
fn insertion_in_the_middle_costs_one_key() {
    let hub = DeckcraftHub::new();
    let a = sample();
    let mut b = a.clone();
    slide_mut(&mut b, 1, |s| {
        s.shapes.insert(1, Shape { id: deckcraft_model::ShapeId(98), name: "Middle".into(), ..Default::default() });
    });

    let changes = hub.diff(&a, &b);
    let positions = changes.iter().filter(|c| matches!(c, craft_collab::change::Change::Set { prop, .. } if prop.as_ref() == "position")).count();
    let creates = changes.iter().filter(|c| matches!(c, craft_collab::change::Change::Create { .. })).count();
    assert_eq!(creates, 1, "one created shape");
    assert_eq!(positions, 1, "exactly one position write: the inserted shape");

    let mut applied = a.clone();
    hub.apply(&mut applied, &changes);
    assert_same_state(&hub, &applied, &b, "insert");
    // And the inserted shape really sits second.
    let second = slide_get(&applied, 1, |s| s.shapes[1].name.clone());
    assert_eq!(second, "Middle");
}

/// Two clients editing different shapes of the same slide converge (the server orders the
/// batches; last-writer-wins per property, and neither side loses the other's write).
#[test]
fn two_clients_converge() {
    let hub = DeckcraftHub::new();
    let base = sample();

    let mut local = base.clone();
    let mut remote = base.clone();
    slide_mut(&mut local, 1, |s| s.shapes[0].name = "Local name".into());
    slide_mut(&mut remote, 1, |s| s.shapes[1].name = "Remote name".into());

    let local_batch = hub.diff(&base, &local);
    let remote_batch = hub.diff(&base, &remote);
    assert_eq!(local_batch.len(), 1);
    assert_eq!(remote_batch.len(), 1);

    hub.apply(&mut local, &remote_batch);
    hub.apply(&mut remote, &local_batch);
    assert_same_state(&hub, &local, &remote, "convergence");
}

/// A deleted group is one change (its children leave with it), and deleting a deleted object
/// does nothing.
#[test]
fn group_delete_is_one_change_and_tolerant() {
    let hub = DeckcraftHub::new();
    let a = sample();
    let mut b = a.clone();
    slide_mut(&mut b, 1, |s| {
        s.shapes.remove(2); // the group
    });

    let changes = hub.diff(&a, &b);
    let deletes = changes.iter().filter(|c| matches!(c, craft_collab::change::Change::Delete { .. })).count();
    assert_eq!(deletes, 1, "one Delete for the whole group subtree");

    let mut applied = a.clone();
    hub.apply(&mut applied, &changes);
    assert_same_state(&hub, &applied, &b, "delete");
    let report = hub.apply(&mut applied, &changes);
    assert_eq!(report.applied, 0, "deleting a deleted object does nothing");
}

/// The command catalog resolves the v2 names onto their `hub.*` twins, and the deep link parses
/// the family format with the deckcraft scheme.
#[test]
fn catalog_and_deeplink() {
    assert_eq!(deckcraft_hub::commands::resolve("collab.share").map(|c| c.name), Some("hub.share"));
    assert_eq!(deckcraft_hub::commands::resolve("hub.peers").map(|c| c.name), Some("hub.peers"));
    let link = deckcraft_hub::deeplink::JoinLink::parse("deckcraft://join?code=AB12CD34&server=wss%3A%2F%2Fhub.example.com");
    assert_eq!(link.map(|l| (l.code, l.server)), Some((Some("AB12CD34".into()), Some("wss://hub.example.com".into()))));
}

/// A small helper: mutate one slide through its Arc (the document shares what didn't change).
fn slide_mut(doc: &mut Presentation, slide_id: u32, f: impl FnOnce(&mut Slide)) {
    let slot = doc.slides.iter_mut().find(|s| s.id.0 == slide_id).unwrap();
    f(std::sync::Arc::make_mut(slot));
}

/// A small helper: read one slide's derived value.
fn slide_get(doc: &Presentation, slide_id: u32, f: impl FnOnce(&Slide) -> String) -> String {
    let slot = doc.slides.iter().find(|s| s.id.0 == slide_id).unwrap();
    f(slot)
}
