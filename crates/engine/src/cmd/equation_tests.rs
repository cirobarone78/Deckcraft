use super::*;

fn session() -> Session {
    let mut s = Session::with_new();
    s.execute("insert.equation", &json!({})).unwrap();
    s
}

fn body(s: &Session) -> &TextBody {
    let st = s.doc().unwrap();
    let sel = st.selection.text.as_ref().unwrap();
    super::super::text::body_of(st, sel).unwrap()
}

fn text(s: &Session) -> String {
    body(s).text()
}

#[test]
fn equation_inserts_selected_prompt_and_undoes_as_one_action() {
    let mut s = Session::with_new();
    let before = s.doc().unwrap().doc.clone();
    let result = s.execute("insert.equation", &json!({})).unwrap();
    assert_eq!(text(&s), PROMPT);
    let st = s.doc().unwrap();
    assert_eq!(st.selection.text.as_ref().unwrap().ordered(), ((0, 0), (0, PROMPT.chars().count())));
    let sh = st.shape(serde_json::from_value(result["id"].clone()).unwrap()).unwrap();
    assert!(sh.xfrm.unwrap().w >= 300.0);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(*s.doc().unwrap().doc, *before);
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(text(&s), PROMPT);
}

#[test]
fn equation_converts_on_space_and_preserves_caret_and_undo() {
    let mut s = session();
    for c in "\\alpha".chars() {
        s.type_text(&c.to_string()).unwrap();
    }
    assert_eq!(text(&s), "\\alpha");
    s.type_text(" ").unwrap();
    assert_eq!(text(&s), "α");
    assert_eq!(s.doc().unwrap().selection.text.as_ref().unwrap().caret, (0, 1));
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(text(&s), "\\alpha");
    s.execute("edit.redo", &json!({})).unwrap();
    s.type_text(" ").unwrap();
    s.type_text("+ \\beta ").unwrap();
    assert_eq!(text(&s), "α + β");
}

#[test]
fn equation_is_inline_and_does_not_convert_normal_text() {
    let mut s = Session::with_new();
    s.execute("insert.textBox", &json!({"text": "some normal text  afterward \\alpha"})).unwrap();
    s.execute("text.edit", &json!({"at": [0, 17]})).unwrap();
    let count = s.doc().unwrap().shapes().len();
    s.execute("insert.equation", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().shapes().len(), count);
    s.type_text("\\alpha ").unwrap();
    assert_eq!(text(&s), "some normal text α afterward \\alpha");
    s.execute("text.move", &json!({"to": "end"})).unwrap();
    s.type_text(" ").unwrap();
    assert!(text(&s).ends_with("\\alpha "));
    assert!(matches!(body(&s).paragraphs[0].runs[0].kind, RunKind::Text));
}

#[test]
fn equation_keeps_unknown_commands_and_plain_spaces() {
    let mut s = session();
    s.type_text("\\unknown \\ALPHA \\alphaX \\alpha  + z ").unwrap();
    assert_eq!(text(&s), "\\unknown \\ALPHA \\alphaX α + z ");
    s.type_text("\\\\beta ").unwrap();
    assert!(text(&s).ends_with("\\\\beta "));
}

#[test]
fn equation_handles_scripts_operators_and_multiline_input() {
    let mut s = session();
    s.type_text("x^2 + a_{12} = \\Gamma+\\beta \n\\infty ").unwrap();
    assert_eq!(text(&s), "x²+ a₁₂= Γ+β\n∞");
    s.type_text(" x^{ab} ").unwrap();
    assert!(text(&s).ends_with("x^{ab} "), "unsupported scripts remain editable source");
}

#[test]
fn equation_edit_delete_retype_and_paste_stay_math() {
    let mut s = session();
    s.type_text("\\alpha ").unwrap();
    s.execute("text.delete", &json!({})).unwrap();
    assert_eq!(text(&s), PROMPT);
    s.execute("edit.paste", &json!({"text": "\\beta + \\gamma "})).unwrap();
    assert_eq!(text(&s), "β+ γ");
    s.execute("text.select", &json!({"anchor": [0, 0], "caret": [0, 1]})).unwrap();
    s.type_text("\\delta ").unwrap();
    assert_eq!(text(&s), "δ+ γ");
    assert!(body(&s).paragraphs[0].runs.iter().all(|r| matches!(r.kind, RunKind::Equation { placeholder: false })));
}

#[test]
fn equation_click_inside_prompt_replaces_all_of_it() {
    let mut s = session();
    s.execute("text.select", &json!({"anchor": [0, 5], "caret": [0, 5]})).unwrap();
    s.type_text("x").unwrap();
    assert_eq!(text(&s), "x");
}

#[test]
fn equation_region_and_formatting_survive_native_and_pptx_roundtrips() {
    let mut s = session();
    s.type_text("\\alpha + x^2 ").unwrap();
    let original = s.doc().unwrap().doc.clone();
    let native = deckcraft_format::load(&deckcraft_format::save(&original).unwrap()).unwrap();
    let pptx = deckcraft_pptx::import(&deckcraft_pptx::export(&original).unwrap()).unwrap();
    for doc in [native, pptx] {
        let eq = doc.slides[0].shapes.iter().find(|sh| sh.name == "Equation").unwrap();
        let tb = eq.text.as_ref().unwrap();
        assert_eq!(tb.text(), "α+ x²");
        assert!(tb.paragraphs[0].runs.iter().all(|r| matches!(r.kind, RunKind::Equation { placeholder: false })));
        assert_eq!(tb.paragraphs[0].runs[0].props.size, Some(28.0));
        let mut b = tb.clone();
        let end = ed::end(&b);
        let mut sel = TextSel { anchor: end, caret: end, ..Default::default() };
        let ctx = at(&b, end, false).unwrap();
        type_text(&mut b, &mut sel, " + \\beta ", None, ctx);
        assert_eq!(b.text(), "α+ x² + β");
    }
}

fn arrow(s: &mut Session, key: &str) {
    s.key(key, crate::tools::Mods::default()).unwrap();
}

#[test]
fn equation_right_exits_without_a_character_and_restores_plain_typing() {
    let mut s = session();
    s.prefs.autocorrect = false;
    s.type_text("\\alpha ").unwrap();
    let before = s.doc().unwrap().doc.clone();
    arrow(&mut s, "ArrowRight");
    assert!(active(s.doc().unwrap()).is_none());
    assert_eq!(*s.doc().unwrap().doc, *before, "exiting is not a document edit");
    assert_eq!(s.doc().unwrap().selection.text.as_ref().unwrap().caret, (0, 1));
    assert_ne!(s.execute("format.state", &json!({})).unwrap()["italic"], true);
    // Repeated Right at the paragraph end keeps the caret outside the field.
    arrow(&mut s, "ArrowRight");
    s.type_text(" normal \\alpha ").unwrap();
    assert_eq!(text(&s), "α normal \\alpha ");
    let runs = &body(&s).paragraphs[0].runs;
    assert!(matches!(runs[0].kind, RunKind::Equation { .. }));
    assert_eq!(runs[0].props.italic, Some(true));
    assert_eq!(runs[1].kind, RunKind::Text);
    assert_ne!(runs[1].props.italic, Some(true));
    assert_ne!(runs[1].props.no_proof, Some(true));
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(text(&s), "α");
    assert!(active(s.doc().unwrap()).is_none(), "undo restores the outside caret");
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(text(&s), "α normal \\alpha ");
}

#[test]
fn equation_exit_preserves_surrounding_style_including_intentional_italics() {
    for italic in [false, true] {
        let mut s = Session::with_new();
        s.execute("insert.textBox", &json!({"text": "before  after"})).unwrap();
        s.execute("format.italic", &json!({"on": italic})).unwrap();
        s.execute("format.bold", &json!({"on": true})).unwrap();
        s.execute("text.edit", &json!({"at": [0, 7]})).unwrap();
        let expected = ed::props_at(body(&s), (0, 7));
        s.execute("insert.equation", &json!({})).unwrap();
        s.type_text("x+y").unwrap();
        arrow(&mut s, "ArrowRight");
        assert_eq!(s.doc().unwrap().selection.text.as_ref().unwrap().caret, (0, 10));
        assert!(active(s.doc().unwrap()).is_none());
        s.type_text(" prose").unwrap();
        assert_eq!(text(&s), "before x+y prose after");
        assert_eq!(ed::props_at(body(&s), (0, 16)), expected);
        assert_eq!(body(&s).paragraphs[0].runs.last().unwrap().kind, RunKind::Text);
    }
}

#[test]
fn equation_arrows_and_click_reenter_without_skipping_characters() {
    let mut s = session();
    s.type_text("xy").unwrap();
    arrow(&mut s, "ArrowRight");
    arrow(&mut s, "ArrowLeft");
    assert!(active(s.doc().unwrap()).is_some());
    assert_eq!(s.doc().unwrap().selection.text.as_ref().unwrap().caret, (0, 2));
    arrow(&mut s, "ArrowLeft");
    assert_eq!(s.doc().unwrap().selection.text.as_ref().unwrap().caret, (0, 1));
    arrow(&mut s, "ArrowRight");
    assert!(active(s.doc().unwrap()).is_some(), "reaching the end still edits the equation");
    arrow(&mut s, "ArrowRight");
    assert!(active(s.doc().unwrap()).is_none());
    s.execute("text.select", &json!({"anchor": [0, 2]})).unwrap();
    assert!(active(s.doc().unwrap()).is_some(), "clicking the field clears outside affinity");
    s.type_text("\\beta ").unwrap();
    assert_eq!(text(&s), "xyβ");
}

#[test]
fn equation_boundaries_span_formatting_runs_and_shift_right_keeps_selection() {
    let mut s = session();
    s.type_text("xy").unwrap();
    s.execute("text.select", &json!({"anchor": [0, 0], "caret": [0, 1]})).unwrap();
    s.execute("format.bold", &json!({"on": true})).unwrap();
    s.execute("text.select", &json!({"anchor": [0, 1]})).unwrap();
    assert_eq!(active(s.doc().unwrap()).unwrap().ordered(), ((0, 0), (0, 2)));
    arrow(&mut s, "ArrowRight");
    assert_eq!(s.doc().unwrap().selection.text.as_ref().unwrap().caret, (0, 2));
    assert!(active(s.doc().unwrap()).is_some(), "formatting does not create an exit boundary");
    arrow(&mut s, "ArrowRight");
    s.type_text(" normal").unwrap();
    s.execute("text.select", &json!({"anchor": [0, 2]})).unwrap();
    s.key("ArrowRight", crate::tools::Mods { shift: true, ..Default::default() }).unwrap();
    assert_eq!(s.doc().unwrap().selection.text.as_ref().unwrap().ordered(), ((0, 2), (0, 3)));
    assert!(active(s.doc().unwrap()).is_none());
}

#[test]
fn equation_exit_allows_plain_paste_and_independent_formatting() {
    let mut s = session();
    s.type_text("x").unwrap();
    arrow(&mut s, "ArrowRight");
    s.execute("format.bold", &json!({"on": true})).unwrap();
    s.type_text("bold").unwrap();
    assert_eq!(body(&s).paragraphs[0].runs[1].props.bold, Some(true));
    assert_ne!(body(&s).paragraphs[0].runs[0].props.bold, Some(true));
    s.execute("text.select", &json!({"anchor": [0, 1]})).unwrap();
    arrow(&mut s, "ArrowRight");
    s.execute("edit.paste", &json!({"text": "\\alpha "})).unwrap();
    assert_eq!(text(&s), "x\\alpha bold");
    assert_eq!(body(&s).paragraphs[0].runs[1].kind, RunKind::Text);
    assert_ne!(body(&s).paragraphs[0].runs[1].props.italic, Some(true));
}
