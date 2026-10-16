use eframe::egui::{
    self,
    text::{CCursor, CCursorRange, LayoutJob},
    Color32, FontId, Stroke, TextFormat,
};
use camel_audio_runtime::language::{Diagnostic, Severity};

pub fn character_at(text: &str, line: usize, column: usize) -> usize {
    let mut row = 1;
    let mut col = 1;
    for (index, ch) in text.chars().enumerate() {
        if row == line && col >= column {
            return index;
        }
        if ch == '\n' {
            if row == line {
                return index;
            }
            row += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    text.chars().count()
}
pub fn show(
    ui: &mut egui::Ui,
    text: &mut String,
    id: egui::Id,
    jump: Option<(usize, usize)>,
    diagnostics: &[Diagnostic],
) -> (bool, (usize, usize)) {
    if let Some((line, column)) = jump {
        let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(CCursorRange::one(CCursor::new(character_at(
                text, line, column,
            )))));
        state.store(ui.ctx(), id);
        ui.memory_mut(|m| m.request_focus(id));
    }
    let font = FontId::monospace(14.0);
    let height = ui.fonts_mut(|f| f.row_height(&font));
    let lines = text.bytes().filter(|c| *c == b'\n').count() + 1;
    let gutter = (lines.to_string().len() as f32 * 9.0 + 12.0).max(38.0);
    let mut changed = false;
    let mut position = (1, 1);
    egui::ScrollArea::both()
        .id_salt((id, "scroll"))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(gutter, height * lines as f32),
                    egui::Sense::hover(),
                );
                let clip = ui.clip_rect();
                for line in 0..lines {
                    let y = rect.top() + height * line as f32;
                    if y + height >= clip.top() && y <= clip.bottom() {
                        ui.painter().text(
                            egui::pos2(rect.right() - 5.0, y),
                            egui::Align2::RIGHT_TOP,
                            (line + 1).to_string(),
                            font.clone(),
                            Color32::from_rgb(145, 151, 159),
                        );
                    }
                }
                let mut layout = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, _: f32| {
                    ui.fonts_mut(|f| f.layout_job(highlight(buf.as_str(), diagnostics)))
                };
                let out = egui::TextEdit::multiline(text)
                    .id(id)
                    .font(font)
                    .code_editor()
                    .frame(false)
                    .margin(0.0)
                    .desired_width((ui.available_width() - 4.0).max(480.0))
                    .desired_rows(lines.max(24))
                    .layouter(&mut layout)
                    .show(ui);
                changed = out.response.changed();
                if let Some(range) = out.cursor_range {
                    let prefix: String = text.chars().take(range.primary.index).collect();
                    position = (
                        prefix.chars().filter(|c| *c == '\n').count() + 1,
                        prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1,
                    );
                }
                if let Some((line, _)) = jump {
                    let y = out.response.rect.top() + height * line.saturating_sub(1) as f32;
                    ui.scroll_to_rect(
                        egui::Rect::from_min_size(
                            egui::pos2(out.response.rect.left(), y),
                            egui::vec2(20.0, height),
                        ),
                        Some(egui::Align::Center),
                    );
                }
            });
        });
    (changed, position)
}
fn highlight(source: &str, diagnostics: &[Diagnostic]) -> LayoutJob {
    let mut job = LayoutJob::default();
    let mut index = 0;
    while index < source.len() {
        let rest = &source[index..];
        let ch = rest.chars().next().unwrap();
        let (len, color) = if rest.starts_with("//") {
            (
                rest.find('\n').unwrap_or(rest.len()),
                Color32::from_rgb(112, 123, 113),
            )
        } else if rest.starts_with("/*") {
            (
                rest.find("*/").map(|n| n + 2).unwrap_or(rest.len()),
                Color32::from_rgb(112, 123, 113),
            )
        } else if ch == '"' || rest.starts_with("$\"") {
            let start = if ch == '$' { 2 } else { 1 };
            let mut end = rest.len();
            let mut escaped = false;
            for (offset, c) in rest[start..].char_indices() {
                if c == '"' && !escaped {
                    end = start + offset + 1;
                    break;
                }
                escaped = c == '\\' && !escaped;
            }
            (end, Color32::from_rgb(132, 77, 42))
        } else if ch.is_alphabetic() || ch == '_' {
            let len = rest
                .char_indices()
                .find(|(_, c)| !c.is_alphanumeric() && *c != '_')
                .map(|(i, _)| i)
                .unwrap_or(rest.len());
            let keyword = [
                "workflow",
                "agent",
                "tool",
                "policy",
                "record",
                "import",
                "return",
                "if",
                "else",
                "while",
                "for",
                "foreach",
                "in",
                "try",
                "catch",
                "finally",
                "throw",
                "spawn",
                "await",
                "break",
                "continue",
                "void",
                "int",
                "double",
                "string",
                "bool",
                "var",
                "Task",
                "Agent",
                "AgentResult",
                "Audio",
                "true",
                "false",
                "null",
            ]
            .contains(&&rest[..len]);
            (
                len,
                if keyword {
                    Color32::from_rgb(46, 91, 147)
                } else {
                    Color32::from_rgb(44, 49, 57)
                },
            )
        } else if ch.is_ascii_digit() {
            let len = rest
                .find(|c: char| !c.is_ascii_digit() && c != '.')
                .unwrap_or(rest.len());
            (len, Color32::from_rgb(111, 79, 143))
        } else {
            (ch.len_utf8(), Color32::from_rgb(73, 80, 89))
        };
        job.append(
            &rest[..len],
            0.0,
            TextFormat {
                font_id: FontId::monospace(14.0),
                color,
                ..Default::default()
            },
        );
        index += len;
    }
    for diagnostic in diagnostics {
        let start = diagnostic.primary_span.start.offset.min(source.len());
        let end = diagnostic.primary_span.end.offset.min(source.len());
        if start >= end || !source.is_char_boundary(start) || !source.is_char_boundary(end) {
            continue;
        }
        let color = if diagnostic.severity == Severity::Error {
            Color32::from_rgb(255, 226, 224)
        } else {
            Color32::from_rgb(255, 245, 204)
        };
        for section in &mut job.sections {
            if section.byte_range.start < end && section.byte_range.end > start {
                section.format.background = color;
                section.format.underline = Stroke::new(1.0, Color32::from_rgb(190, 55, 48));
            }
        }
    }
    job.wrap.max_width = f32::INFINITY;
    job
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn positions_use_characters_not_utf8_bytes() {
        assert_eq!(character_at("é漢\r\nx", 2, 1), 4);
        assert_eq!(character_at("é漢", 1, 2), 1);
        assert_eq!(character_at("a", 100, 1), 1);
    }
    #[test]
    fn highlighting_preserves_all_source_bytes() {
        let s = "// 漢\nagent A { system = $\"hé {x}\"; /*x*/ }";
        assert_eq!(highlight(s, &[]).text, s);
    }
}
