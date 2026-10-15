//! In-place linear equations and delimiter-triggered math shortcuts.
//!
//! Equation runs remain ordinary editable characters for layout, selection and undo. They are
//! deliberately separate from imported OMML, which this editor cannot structurally edit.

use deckcraft_geom::Xfrm;
use deckcraft_model::edit::{self as ed, Pos};
use deckcraft_model::text::{RunKind, RunProps, TextBody};
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session, TextSel};

pub const PROMPT: &str = "Type equation here";

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "insert.equation",
        "Equation",
        ["Insert", "Symbols"],
        None,
        "{} — insert an editable equation at the text caret, or in a new text box",
        has_slide,
        insert
    )]
}

fn insert(s: &mut Session, _: &Value) -> Result<Value> {
    let new_box = s.doc()?.selection.text.is_none();
    if new_box {
        let size = s.doc()?.doc.slide_size;
        let mut shape = super::insert::new_text_box(Xfrm::new((size.width - 320.0) / 2.0, (size.height - 50.0) / 2.0, 320.0, 50.0), "");
        shape.name = "Equation".into();
        let id = super::insert::add_shape(s, "insert.equation", shape, true)?;
        s.select(|_, sel| sel.text = Some(TextSel { shape: id, ..Default::default() }))?;
    }
    super::text::edit_text(s, |body, t| {
        let (a, b) = t.ordered();
        let mut props = t.plain_props().cloned().unwrap_or_else(|| ed::props_at(body, a));
        t.after_equation = None;
        props.font = Some("Liberation Serif".into());
        if new_box {
            props.size = Some(28.0);
        }
        props.italic = Some(true);
        props.no_proof = Some(true);
        props.link = None;
        let start = ed::delete(body, a, b);
        t.caret = ed::insert_as(body, start, PROMPT, Some(props), RunKind::Equation { placeholder: true });
        t.anchor = start;
    })?;
    let t = s.doc()?.selection.text.as_ref().ok_or_else(|| bad("insert.equation", NO_TEXT_POINT))?;
    Ok(json!({"id": t.shape, "anchor": t.anchor, "caret": t.caret}))
}

pub(super) struct Context {
    start: usize,
    end: usize,
    placeholder: bool,
    props: RunProps,
}

/// The complete active equation, for contextual UI and editor-only field shading.
pub fn active(st: &crate::DocState) -> Option<TextSel> {
    let t = st.selection.text.as_ref()?;
    let body = super::text::body_of(st, t)?;
    let ctx = context(body, t)?;
    let mut range = t.clone();
    range.anchor = (t.ordered().0.0, ctx.start);
    range.caret = (range.anchor.0, ctx.end);
    Some(range)
}

pub(super) fn context(body: &TextBody, t: &TextSel) -> Option<Context> {
    if t.plain_props().is_some() {
        return None;
    }
    let (a, b) = t.ordered();
    let ctx = at(body, a, t.is_range())?;
    // Selecting normal text as well does not turn the whole selection into an equation.
    (a.0 == b.0 && b.1 <= ctx.end).then_some(ctx)
}

/// An exit is a caret affinity change, not a document edit or an inserted space.
pub(super) fn exit_at_end(body: &TextBody, t: &TextSel) -> Option<crate::EquationExit> {
    if t.is_range() {
        return None;
    }
    let ctx = context(body, t)?;
    if t.caret.1 != ctx.end {
        return None;
    }
    let para = body.paragraphs.get(t.caret.0)?;
    let mut before = None;
    let mut after = None;
    let mut offset = 0;
    for run in &para.runs {
        let end = offset + run.char_len();
        if run.kind == RunKind::Text {
            if end <= ctx.start {
                before = Some(run.props.clone());
            } else if offset >= ctx.end && after.is_none() {
                after = Some(run.props.clone());
            }
        }
        offset = end;
    }
    let mut props = before.or(after).unwrap_or_else(|| {
        // A standalone equation has no neighbouring prose. Use paragraph defaults, retaining
        // its size, but don't inherit the equation's font, italics or spell-check suppression.
        let mut props = para.def_props.clone();
        props.size = props.size.or(ctx.props.size);
        props
    });
    props.link = None;
    Some(crate::EquationExit { at: t.caret, props })
}

pub(super) fn restore_empty(body: &mut TextBody, t: &mut TextSel, ctx: Option<Context>) {
    if let Some(ctx) = ctx.filter(|_| body.is_empty()) {
        t.anchor = (0, 0);
        t.caret = ed::insert_as(body, (0, 0), PROMPT, Some(ctx.props), RunKind::Equation { placeholder: true });
    }
}

/// A selection starts in the run to its right; a caret types with the run on its left.
pub(super) fn at(body: &TextBody, pos: Pos, selection: bool) -> Option<Context> {
    let para = body.paragraphs.get(pos.0)?;
    let mut start = 0;
    for (i, run) in para.runs.iter().enumerate() {
        let end = start + run.char_len();
        let inside = if selection || pos.1 == 0 { pos.1 >= start && pos.1 < end } else { pos.1 > start && pos.1 <= end };
        if inside {
            let RunKind::Equation { placeholder } = run.kind else { return None };
            // Formatting splits a field into runs; the field itself still has one boundary.
            let mut lo = start;
            let mut hi = end;
            for adjacent in para.runs.iter().take(i).rev().take_while(|r| r.kind == run.kind) {
                lo -= adjacent.char_len();
            }
            for adjacent in para.runs.iter().skip(i + 1).take_while(|r| r.kind == run.kind) {
                hi += adjacent.char_len();
            }
            return Some(Context { start: lo, end: hi, placeholder, props: run.props.clone() });
        }
        start = end;
    }
    None
}

pub(super) fn type_text(body: &mut TextBody, t: &mut TextSel, text: &str, props: Option<RunProps>, ctx: Context) -> Pos {
    t.after_equation = None;
    let (mut a, mut b) = t.ordered();
    // Clicking anywhere inside the prompt and typing replaces the whole prompt.
    if ctx.placeholder && a.0 == b.0 && a.1 >= ctx.start && b.1 <= ctx.end {
        a.1 = ctx.start;
        b.1 = ctx.end;
    }
    let start = ed::delete(body, a, b);
    let end = ed::insert_as(body, start, text, Some(props.unwrap_or(ctx.props)), RunKind::Equation { placeholder: false });
    let end = convert(body, start, end);
    t.anchor = end;
    t.caret = end;
    end
}

/// Convert only tokens whose delimiter arrived with this edit. Work backwards so earlier
/// offsets stay valid, including Unicode and pasted expressions containing several shortcuts.
fn convert(body: &mut TextBody, start: Pos, mut end: Pos) -> Pos {
    for pi in (start.0..=end.0).rev() {
        let Some(para) = body.paragraphs.get(pi) else { continue };
        let chars: Vec<char> = para.text().chars().collect();
        let from = if pi == start.0 { start.1 } else { 0 };
        let to = if pi == end.0 { end.1 } else { chars.len() };
        let mut ranges = Vec::new();
        let mut offset = 0;
        for run in &para.runs {
            let next = offset + run.char_len();
            if matches!(run.kind, RunKind::Equation { placeholder: false }) {
                if let Some((_, last)) = ranges.last_mut().filter(|(_, last)| *last == offset) {
                    *last = next;
                } else {
                    ranges.push((offset, next));
                }
            }
            offset = next;
        }
        let mut edits = Vec::new();
        for (lo, hi) in ranges {
            for i in from.max(lo)..to.min(hi) {
                let Some(&delimiter) = chars.get(i) else { continue };
                if delimiter != ' ' && !"+-=*/^_,;:()[]<>".contains(delimiter) {
                    continue;
                }
                if let Some((a, replacement)) = replacement(&chars, lo, i) {
                    let b = i + usize::from(delimiter == ' ');
                    edits.push((a, b, replacement));
                }
            }
        }
        let mut next_start = chars.len();
        for (a, b, replacement) in edits.into_iter().rev() {
            if b > next_start {
                continue;
            }
            let props = ed::props_at(body, (pi, a + 1));
            ed::delete(body, (pi, a), (pi, b));
            ed::insert_as(body, (pi, a), &replacement, Some(props), RunKind::Equation { placeholder: false });
            if pi == end.0 {
                end.1 = end.1.saturating_sub(b - a) + replacement.chars().count();
            }
            next_start = a;
        }
    }
    end
}

fn replacement(chars: &[char], lo: usize, end: usize) -> Option<(usize, String)> {
    let mut start = end;
    while start > lo && end - start < 64 && chars.get(start - 1).is_some_and(char::is_ascii_alphabetic) {
        start -= 1;
    }
    if start > lo && chars.get(start - 1) == Some(&'\\') && (start < 2 || chars.get(start - 2) != Some(&'\\')) {
        let name: String = chars.get(start..end)?.iter().collect();
        return symbol(&name).map(|s| (start - 1, s.to_string()));
    }
    // Simple Unicode super/subscripts, including braced multi-character scripts. Unsupported
    // scripts are left verbatim, rather than silently dropping a character or its marker.
    let braced = end > lo && chars.get(end - 1) == Some(&'}');
    let script_end = end.saturating_sub(usize::from(braced));
    start = script_end;
    while start > lo && end - start < 64 {
        let c = *chars.get(start - 1)?;
        if c == '^' || c == '_' || c == '{' {
            break;
        }
        if c.is_whitespace() || "=\\*/".contains(c) {
            return None;
        }
        start -= 1;
    }
    let marker = start.checked_sub(1 + usize::from(braced))?;
    if marker < lo || (braced && chars.get(start - 1) != Some(&'{')) {
        return None;
    }
    let sub = match chars.get(marker)? {
        '_' => true,
        '^' => false,
        _ => return None,
    };
    let script = chars.get(start..script_end)?;
    if script.is_empty() {
        return None;
    }
    let converted: Option<String> = script.iter().map(|&c| script_char(c, sub)).collect();
    converted.map(|s| (marker, s))
}

fn script_char(c: char, sub: bool) -> Option<char> {
    let (plain, raised) = if sub {
        ("0123456789+-=()aehijklmnoprstuvx", "₀₁₂₃₄₅₆₇₈₉₊₋₌₍₎ₐₑₕᵢⱼₖₗₘₙₒₚᵣₛₜᵤᵥₓ")
    } else {
        ("0123456789+-=()in", "⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻⁼⁽⁾ⁱⁿ")
    };
    plain.chars().position(|x| x == c).and_then(|i| raised.chars().nth(i))
}

fn symbol(name: &str) -> Option<&'static str> {
    Some(match name {
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" => "ε",
        "varepsilon" => "ϵ",
        "zeta" => "ζ",
        "eta" => "η",
        "theta" => "θ",
        "vartheta" => "ϑ",
        "iota" => "ι",
        "kappa" => "κ",
        "lambda" => "λ",
        "mu" => "μ",
        "nu" => "ν",
        "xi" => "ξ",
        "omicron" => "ο",
        "pi" => "π",
        "varpi" => "ϖ",
        "rho" => "ρ",
        "varrho" => "ϱ",
        "sigma" => "σ",
        "varsigma" => "ς",
        "tau" => "τ",
        "upsilon" => "υ",
        "phi" => "φ",
        "varphi" => "ϕ",
        "chi" => "χ",
        "psi" => "ψ",
        "omega" => "ω",
        "Alpha" => "Α",
        "Beta" => "Β",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Xi" => "Ξ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Upsilon" => "Υ",
        "Phi" => "Φ",
        "Psi" => "Ψ",
        "Omega" => "Ω",
        "pm" => "±",
        "mp" => "∓",
        "times" => "×",
        "cdot" => "⋅",
        "div" => "÷",
        "le" | "leq" => "≤",
        "ge" | "geq" => "≥",
        "ne" | "neq" => "≠",
        "approx" => "≈",
        "equiv" => "≡",
        "sim" => "∼",
        "simeq" => "≃",
        "cong" => "≅",
        "propto" => "∝",
        "ll" => "≪",
        "gg" => "≫",
        "infty" | "infinity" => "∞",
        "partial" => "∂",
        "nabla" => "∇",
        "sum" => "∑",
        "prod" => "∏",
        "int" => "∫",
        "iint" => "∬",
        "iiint" => "∭",
        "oint" => "∮",
        "sqrt" => "√",
        "in" => "∈",
        "notin" => "∉",
        "ni" => "∋",
        "subset" => "⊂",
        "supset" => "⊃",
        "subseteq" => "⊆",
        "supseteq" => "⊇",
        "cup" => "∪",
        "cap" => "∩",
        "emptyset" | "varnothing" => "∅",
        "forall" => "∀",
        "exists" => "∃",
        "neg" => "¬",
        "land" | "wedge" => "∧",
        "lor" | "vee" => "∨",
        "to" | "rightarrow" => "→",
        "leftarrow" => "←",
        "leftrightarrow" => "↔",
        "mapsto" => "↦",
        "Rightarrow" | "implies" => "⇒",
        "Leftarrow" => "⇐",
        "Leftrightarrow" | "iff" => "⇔",
        "uparrow" => "↑",
        "downarrow" => "↓",
        "ldots" | "dots" => "…",
        "cdots" => "⋯",
        "vdots" => "⋮",
        "ddots" => "⋱",
        "angle" => "∠",
        "perp" => "⊥",
        "parallel" => "∥",
        "degree" => "°",
        "ell" => "ℓ",
        "hbar" => "ℏ",
        _ => return None,
    })
}

#[cfg(test)]
#[path = "equation_tests.rs"]
mod tests;
