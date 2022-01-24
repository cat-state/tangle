use std::convert::TryInto;

use egui::{text::LayoutJob};
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

// use crate::text_buffer::TextBuffer;

pub fn highlight(theme: &CodeTheme, code: &str) -> LayoutJob {
    let mut highlighter = Highlighter::new();
    let mut cfg = HighlightConfiguration::new(
        tree_sitter_python::language(),
        tree_sitter_python::HIGHLIGHT_QUERY,
        "",
        "",
    )
    .unwrap();

    let mut highlight_names: Vec<&str> = theme.map.keys().map(|s| s.as_str()).collect();
    highlight_names.sort();

    cfg.configure(
        &highlight_names
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<String>>(),
    );
    // if let Some(tree) = code.tree.clone() {
    //     let mut job = LayoutJob::default();
    //     tree.
    // }

    let highlights = highlighter
        .highlight(&cfg, code.as_bytes(), None, |_| None)
        .unwrap();
    eprintln!("\n\n");

    let mut job = LayoutJob::default();
    let mut hcur = None;
    let default = egui::TextFormat::simple(
        egui::TextStyle::Monospace,
        egui::TextFormat::default().color,
    );

    for event in highlights {
        match event.unwrap() {
            HighlightEvent::Source { start, end } => {
                job.append(&code[start..end], 0.0, hcur.unwrap_or(default));
            }
            HighlightEvent::HighlightStart(s) => {
                hcur = theme.map.get(&highlight_names[s.0].to_string()).map(|t| *t);
            }
            HighlightEvent::HighlightEnd => {
                hcur.take();
            }
        }
    }
    job
}

/// View some code with syntax highlighing and selection.
pub fn code_view_ui(
    ui: &mut egui::Ui,
    code: &mut String,
    theme: &CodeTheme,
    exec: bool,
    job: &mut LayoutJob,
) -> egui::widgets::text_edit::TextEditOutput {
    let ori = code.clone();
    let mut layouter = |ui: &egui::Ui, string: &str, _wrap_width: f32| {
        // layout_job.wrap_width = wrap_width; // no wrapping
        if string == ori {
            ui.fonts().layout_job(job.clone())
        } else {
            *job = highlight(theme, string);

            ui.fonts().layout_job(job.clone())
        }
    };
    let edit_output = egui::TextEdit::multiline(code)
        .text_style(egui::TextStyle::Monospace) // for cursor height
        .code_editor()
        .desired_rows(1)
        .desired_width(f32::INFINITY)
        .lock_focus(true)
        .layouter(&mut layouter)
        .interactive(!exec)
        .show(ui);
    edit_output
}

#[derive(Clone, Copy, PartialEq, serde::Deserialize, serde::Serialize, enum_map::Enum)]
enum TokenType {
    Comment,
    Keyword,
    Literal,
    StringLiteral,
    Punctuation,
    Whitespace,
}

#[derive(Clone, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct CodeTheme {
    map: std::collections::HashMap<String, egui::TextFormat>,
    default_format: egui::TextFormat,
}

impl Default for CodeTheme {
    fn default() -> Self {
        let highlight_names: std::collections::HashMap<String, egui::TextFormat> = vec![
            "attribute",
            "constant",
            "function.builtin",
            "function",
            "keyword",
            "literal",
            "operator",
            "property",
            "punctuation",
            "punctuation.bracket",
            "punctuation.delimiter",
            "string",
            "string.special",
            "tag",
            "type",
            "type.builtin",
            "variable",
            "variable.builtin",
            "variable.parameter",
        ]
        .into_iter()
        .map(|s| {
            (
                s.to_string(),
                egui::TextFormat::simple(
                    egui::TextStyle::Monospace,
                    egui::TextFormat::default().color,
                ),
            )
        })
        .collect();
        Self {
            map: highlight_names,
            default_format: egui::TextFormat::simple(
                egui::TextStyle::Monospace,
                egui::TextFormat::default().color,
            ),
        }
    }
}

impl CodeTheme {
    pub fn ui(&mut self, ui: &mut egui::Ui) -> bool {
        ui.horizontal_top(|ui| {
            let selected_id = egui::Id::null();
            let mut selected_tt: i64 = ui.memory().data.get_persisted(selected_id).unwrap_or(0);

            ui.vertical(|ui| {
                ui.set_width(150.0);
                egui::widgets::global_dark_light_mode_buttons(ui);

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);
                let mut fonts = ui.ctx().fonts().definitions().clone();
                if let Some((_, size)) = fonts.family_and_size.get_mut(&egui::TextStyle::Monospace)
                {
                    ui.add(egui::Slider::new(size, 8.0..=24.0).text("font size"));
                }
                ui.ctx().set_fonts(fonts);

                ui.scope(|ui| {
                    ui.style_mut().override_text_style = Some(self.default_format.style);
                    ui.visuals_mut().override_text_color = Some(self.default_format.color);
                    ui.radio_value(&mut selected_tt, -1, "base");

                    let mut items = self.map.iter().collect::<Vec<_>>();
                    items.sort_by(|x, y| x.0.cmp(y.0));
                    for (i, (tt, format)) in items.iter().enumerate() {
                        ui.style_mut().override_text_style = Some(format.style);
                        ui.visuals_mut().override_text_color = Some(format.color);
                        ui.radio_value(&mut selected_tt, i.try_into().unwrap(), tt.as_str());
                    }
                });
            });

            ui.add_space(16.0);

            ui.memory().data.insert_persisted(selected_id, selected_tt);
            let sel = selected_tt.clone();

            use std::convert::TryFrom;

            let selected_color = if sel == -1 {
                &mut self.default_format
            } else {
                let mut items = self.map.keys().collect::<Vec<_>>();
                items.sort();
                let selected_syntax = &items[usize::try_from(selected_tt).unwrap()].to_string();
                self.map.get_mut(selected_syntax).unwrap()
            };
            selected_color.style = egui::TextStyle::Monospace;
            egui::Frame::group(ui.style())
                .margin(egui::Vec2::splat(2.0))
                .show(ui, |ui| {
                    // ui.group(|ui| {
                    ui.style_mut().override_text_style = Some(egui::TextStyle::Small);
                    ui.spacing_mut().slider_width = 128.0; // Controls color picker size
                    egui::widgets::color_picker::color_picker_color32(
                        ui,
                        &mut selected_color.color,
                        egui::color_picker::Alpha::Opaque,
                    )
                })
                .inner
        })
        .inner
    }
}
