//! The Equation tab: shown while the Equation dialog is open. Its galleries (fractions, scripts,
//! radicals, integrals, large operators, brackets, functions, accents, limits and logs, operators,
//! matrices), the symbol sets and the ready-made equations are equaSHion's; they insert into the
//! dialog's editor at the caret, so the dialog stays where it is and the ribbon does the choosing.

use egui::{Align2, CornerRadius, Rect, Sense, Ui, pos2, vec2};
use equashion_egui::EquationEditor;

use crate::SlideApp;
use crate::theme::{self, Tokens};
use crate::widgets::{RIBBON_H, group};

/// The structure galleries, in tab order: (gallery id, button label).
const GALLERIES: [(&str, &str); 11] = [
    ("fraction", "Fraction"),
    ("script", "Script"),
    ("radical", "Radical"),
    ("integral", "Integral"),
    ("largeOperator", "Large\nOperator"),
    ("bracket", "Bracket"),
    ("function", "Function"),
    ("accent", "Accent"),
    ("limitLog", "Limit and\nLog"),
    ("operator", "Operator"),
    ("matrix", "Matrix"),
];

/// A ribbon button: the gallery's face above its label. Returns the click response, for the
/// popup that holds the gallery.
fn button(ui: &mut Ui, ed: &mut EquationEditor, face: Option<&str>, label: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let font = theme::font(11.0);
    let lines: Vec<&str> = label.split('\n').collect();
    let text_w = lines.iter().map(|l| ui.fonts_mut(|f| f.layout_no_wrap(l.to_string(), font.clone(), t.text).size().x)).fold(0.0f32, f32::max);
    let (rect, resp) = ui.allocate_exact_size(vec2((text_w + 14.0).max(46.0), RIBBON_H - 2.0), Sense::click());
    let name = label.replace('\n', " ");
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &name));
    let painter = ui.painter_at(rect.expand(1.0));
    if resp.hovered() {
        painter.rect_filled(rect, CornerRadius::same(4), t.hover);
    }
    let face_box = Rect::from_center_size(pos2(rect.center().x, rect.min.y + 19.0), vec2(34.0, 30.0));
    if let Some((tex, size)) = face.and_then(|id| ed.gallery_face(ui, id, 16.0)) {
        let k = (face_box.width() / size.x.max(1.0)).min(face_box.height() / size.y.max(1.0)).min(1.0);
        let r = Rect::from_center_size(face_box.center(), size * k);
        painter.image(tex, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), t.text);
    } else {
        painter.text(face_box.center(), Align2::CENTER_CENTER, "\u{03A3}", theme::font(18.0), t.text);
    }
    let mut y = rect.min.y + 39.0;
    for l in &lines {
        painter.text(pos2(rect.center().x, y), Align2::CENTER_TOP, l, font.clone(), t.text);
        y += 12.5;
    }
    resp
}

pub fn show(app: &mut SlideApp, ui: &mut Ui) {
    let Some(ed) = app.equation.widget_mut() else { return };
    group(ui, |ui| {
        for (id, label) in GALLERIES {
            let r = button(ui, ed, Some(id), label).on_hover_text(label.replace('\n', " "));
            egui::Popup::menu(&r).width(360.0).show(|ui| {
                if ed.gallery_ui(ui, id) {
                    ui.close();
                }
            });
        }
    });
    group(ui, |ui| {
        let r = button(ui, ed, None, "Symbols").on_hover_text("Symbols");
        egui::Popup::menu(&r).width(360.0).show(|ui| {
            let key = ui.id().with("symbol-set");
            let names = EquationEditor::symbol_set_names();
            let mut set = ui.data_mut(|d| d.get_temp::<usize>(key)).unwrap_or(0).min(names.len().saturating_sub(1));
            ui.horizontal_wrapped(|ui| {
                for (i, n) in names.iter().enumerate() {
                    if ui.selectable_label(set == i, *n).clicked() {
                        set = i;
                    }
                }
            });
            ui.data_mut(|d| d.insert_temp(key, set));
            ui.separator();
            // Stay open: several symbols go in one visit.
            ed.symbols_ui(ui, set);
        });
        let r = button(ui, ed, None, "Ready-made\nEquations").on_hover_text("Replace the equation with a ready-made one");
        egui::Popup::menu(&r).width(260.0).show(|ui| {
            if ed.builtins_ui(ui) {
                ui.close();
            }
        });
    });
}
