//! The synchronized property tables (spec §2.3: "every field the file saves is a property").
//!
//! Two tables — [`SHAPE_PROPS`] over [`Shape`] and [`SLIDE_PROPS`] over [`Slide`] — each with
//! the compile-time guard: a struct gaining a field that doesn't sync is a build error, not a
//! silent divergence. The tree structure (`shapes`, a slide's children) is not a property: the
//! adapter owns the sibling keys and the tree lives in the presentation. `Slide::id` and
//! `Shape::id` are identity, not data.

use craft_collab::Value;
use deckcraft_model::{Shape, ShapeKind, Slide};

/// Declares one property table over a struct's fields: the name list, the getters/setters, the
/// differs, and the every-field-is-a-property guard.
macro_rules! props_table {
    ($ty:ident, $props:ident, $value:ident, $set:ident, $differs:ident, $guard:ident, ($($skip:ident),*), ($(($field:ident, $name:literal)),+ $(,)?)) => {
        /// Every field that syncs, by property name.
        pub const $props: &[&str] = &[$($name),+];

        /// The value of field property `prop`; `None` when `prop` is not a field.
        pub fn $value(obj: &$ty, prop: &str) -> Option<Value> {
            match prop {
                $($name => Some(Value::Json(serde_json::to_value(&obj.$field).ok()?)),)+
                _ => None,
            }
        }

        /// Write field property `prop`; `None` when `prop` is not a field.
        ///
        /// A value of the wrong shape is an error, never a panic — shipped code never crashes.
        pub fn $set(obj: &mut $ty, prop: &str, value: &Value) -> Option<Result<(), String>> {
            match prop {
                $($name => Some(
                    serde_json::from_value(value.to_json())
                        .map(|v| obj.$field = v)
                        .map_err(|e| e.to_string()),
                ),)+
                _ => None,
            }
        }

        /// Whether field property `prop` differs between `a` and `b`; `None` if not a field.
        pub fn $differs(a: &$ty, b: &$ty, prop: &str) -> Option<bool> {
            match prop {
                $($name => Some(a.$field != b.$field),)+
                _ => None,
            }
        }

        /// Fails to compile when the struct gains a field that isn't listed as a property above
        /// (or one of the explicitly skipped ones) — a new saved field must decide how it syncs.
        #[allow(dead_code)]
        fn $guard(obj: &$ty) {
            let $ty { $($field: _,)+ $($skip: _),* } = obj;
        }
    };
}

props_table!(
    Shape,
    SHAPE_PROPS,
    shape_field_value,
    set_shape_field,
    shape_field_differs,
    every_shape_field_is_a_property,
    (id),
    (
        (name, "name"),
        (xfrm, "xfrm"),
        (kind, "kind"),
        (geom, "geom"),
        (ph, "ph"),
        (fill, "fill"),
        (line, "line"),
        (effects, "effects"),
        (style, "style"),
        (text, "text"),
        (hidden, "hidden"),
        (locked, "locked"),
        (text_box, "textBox"),
        (descr, "descr"),
        (title, "title"),
        (decorative, "decorative"),
        (click, "click"),
        (hover, "hover"),
        (tags, "tags"),
        (raw_ext, "rawExt"),
        (diagram, "diagram"),
    )
);

/// The value of the `kind` property: a group's children are **stripped** — they sync as child
/// objects, not inside their parent's kind payload (double-writing them duplicates the
/// children on every remote apply).
#[must_use]
pub fn shape_value(obj: &Shape, prop: &str) -> Option<Value> {
    if prop == "kind" {
        return group_kind_value(obj);
    }
    shape_field_value(obj, prop)
}

/// Write the `kind` property: a group keeps its current children (the incoming kind carries
/// none — they arrive as their own changes).
pub fn set_shape(obj: &mut Shape, prop: &str, value: &Value) -> Option<Result<(), String>> {
    if prop == "kind" {
        return set_group_kind(obj, value);
    }
    set_shape_field(obj, prop, value)
}

/// Whether the `kind` property differs (children-stripped, per [`shape_value`]).
pub fn shape_differs(a: &Shape, b: &Shape, prop: &str) -> Option<bool> {
    if prop == "kind" {
        return Some(group_kind_value(a)? != group_kind_value(b)?);
    }
    shape_field_differs(a, b, prop)
}

/// A group's kind without its children (the child coordinate space and every other payload
/// field sync; the children are the tree, owned by the adapter).
fn group_kind_value(obj: &Shape) -> Option<Value> {
    let mut json = serde_json::to_value(&obj.kind).ok()?;
    if let Some(map) = json.as_object_mut()
        && map.get("type").and_then(serde_json::Value::as_str) == Some("group")
    {
        map.remove("children");
    }
    Some(Value::Json(json))
}

/// Write a kind value, preserving a group's current children when the incoming value carries
/// none (the normal wire form).
fn set_group_kind(obj: &mut Shape, value: &Value) -> Option<Result<(), String>> {
    let mut json = value.to_json();
    let existing_children = if let ShapeKind::Group { children, .. } = &obj.kind { serde_json::to_value(children).ok() } else { None };
    if let (Some(map), Some(children)) = (json.as_object_mut(), existing_children)
        && map.get("type").and_then(serde_json::Value::as_str) == Some("group")
        && !map.contains_key("children")
    {
        map.insert("children".to_string(), children);
    }
    Some(serde_json::from_value::<ShapeKind>(json).map(|k| obj.kind = k).map_err(|e| e.to_string()))
}

props_table!(
    Slide,
    SLIDE_PROPS,
    slide_value,
    set_slide,
    slide_differs,
    every_slide_field_is_a_property,
    (id, shapes),
    (
        (layout, "layout"),
        (name, "name"),
        (background, "background"),
        (hidden, "hidden"),
        (show_master_shapes, "showMasterShapes"),
        (transition, "transition"),
        (animations, "animations"),
        (notes, "notes"),
        (comments, "comments"),
        (color_map, "colorMap"),
        (tags, "tags"),
        (raw_ext, "rawExt"),
    )
);

/// The presentation-level properties (whole-document scalars). `media` deliberately does **not**
/// appear: media bytes ride the asset channel, never a batch (spec §3.5). The masters sync as
/// one whole-list value for the first cut — master edits during a live session are rare, and
/// splitting them per entry is the documented follow-up.
pub const DOC_PROPS: &[&str] =
    &["slideSize", "notesSize", "masters", "sections", "customShows", "show", "headerFooter", "props", "defaultTextStyle", "firstSlideNumber"];

/// Write a presentation-level property; `None` when `prop` is not one of the synced scalars.
pub fn set_doc_prop(doc: &mut deckcraft_model::Presentation, prop: &str, value: &Value) -> Option<Result<(), String>> {
    let json = value.to_json();
    match prop {
        "slideSize" => Some(parse(json).map(|v| doc.slide_size = v)),
        "notesSize" => Some(parse(json).map(|v| doc.notes_size = v)),
        "masters" => Some(parse(json).map(|v| doc.masters = v)),
        "sections" => Some(parse(json).map(|v| doc.sections = v)),
        "customShows" => Some(parse(json).map(|v| doc.custom_shows = v)),
        "show" => Some(parse(json).map(|v| doc.show = v)),
        "headerFooter" => Some(parse(json).map(|v| doc.header_footer = v)),
        "props" => Some(parse(json).map(|v| doc.props = v)),
        "defaultTextStyle" => Some(parse(json).map(|v| doc.default_text_style = v)),
        "firstSlideNumber" => Some(parse(json).map(|v| doc.first_slide_number = v)),
        _ => None,
    }
}

/// The value of a presentation-level property; `None` when `prop` is not a synced scalar.
#[must_use]
pub fn doc_prop_value(doc: &deckcraft_model::Presentation, prop: &str) -> Option<Value> {
    match prop {
        "slideSize" => Some(Value::Json(serde_json::to_value(doc.slide_size).ok()?)),
        "notesSize" => Some(Value::Json(serde_json::to_value(doc.notes_size).ok()?)),
        "masters" => Some(Value::Json(serde_json::to_value(&doc.masters).ok()?)),
        "sections" => Some(Value::Json(serde_json::to_value(&doc.sections).ok()?)),
        "customShows" => Some(Value::Json(serde_json::to_value(&doc.custom_shows).ok()?)),
        "show" => Some(Value::Json(serde_json::to_value(&doc.show).ok()?)),
        "headerFooter" => Some(Value::Json(serde_json::to_value(&doc.header_footer).ok()?)),
        "props" => Some(Value::Json(serde_json::to_value(&doc.props).ok()?)),
        "defaultTextStyle" => Some(Value::Json(serde_json::to_value(&doc.default_text_style).ok()?)),
        "firstSlideNumber" => Some(Value::Json(serde_json::to_value(doc.first_slide_number).ok()?)),
        _ => None,
    }
}

/// Deserialize a value, mapped to the adapter's error shape (wrong shapes are errors, never
/// panics).
fn parse<T: serde::de::DeserializeOwned>(json: serde_json::Value) -> Result<T, String> {
    serde_json::from_value(json).map_err(|e| e.to_string())
}
