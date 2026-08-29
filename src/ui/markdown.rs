//! Mono markdown subset for task notes (view / peek).
//!
//! Markers are styled with bold / dim / underline only — never color. Edit mode
//! keeps the raw source; this module paints the reading surface.
//!
//! Supported inline: `**strong**`, `*em*` / `_em_`, `` `code` ``.
//! Supported line starts: `#`…`######` headings (bold body), `-` / `*` lists
//! (dim marker). Task-list markers like `- [ ]` stay literal text (structured
//! steps own checklists).

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::present_line;
use super::render::{style_bold, style_dim, style_plain, style_reverse, style_underline};

/// Paint one already-wrapped notes row with mono markdown styling.
pub fn paint_md_line(text: &str, width: usize, base: Style) -> Line<'static> {
    if width == 0 {
        return Line::from("");
    }
    let trimmed = text.trim_end();
    if trimmed.is_empty() {
        return Line::from(Span::styled(" ".repeat(width.min(1)), base));
    }

    let (prefix_spans, body, heading) = line_prefix(trimmed);
    let mut spans = prefix_spans;
    let body_base = if heading { style_bold() } else { base };
    spans.extend(inline_spans(body, body_base));
    bound_styled_line(spans, width)
}

/// Dim list / heading markers; return the remainder and whether it is a heading.
fn line_prefix(text: &str) -> (Vec<Span<'static>>, &str, bool) {
    let lead = text.len() - text.trim_start().len();
    let (indent, rest) = text.split_at(lead);
    let mut spans = Vec::new();
    if !indent.is_empty() {
        spans.push(Span::styled(indent.to_string(), style_plain()));
    }
    let bytes = rest.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && bytes[i] == b'#' && i < 6 {
        i += 1;
    }
    if i > 0 && i < bytes.len() && bytes[i] == b' ' {
        spans.push(Span::styled(rest[..i + 1].to_string(), style_dim()));
        return (spans, &rest[i + 1..], true);
    }
    if bytes.len() >= 2 && matches!(bytes[0], b'-' | b'*') && bytes[1] == b' ' {
        spans.push(Span::styled(rest[..2].to_string(), style_dim()));
        return (spans, &rest[2..], false);
    }
    (spans, rest, false)
}

#[derive(Clone, Copy)]
enum InlineKind {
    Plain,
    Strong,
    Em,
    Code,
}

fn inline_spans(text: &str, base: Style) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    let mut kind = InlineKind::Plain;
    let mut buf = String::new();

    let flush = |buf: &mut String, kind: InlineKind, spans: &mut Vec<Span<'static>>| {
        if buf.is_empty() {
            return;
        }
        let style = match kind {
            InlineKind::Plain => base,
            InlineKind::Strong => style_bold(),
            InlineKind::Em => style_underline(),
            InlineKind::Code => style_reverse(),
        };
        spans.push(Span::styled(std::mem::take(buf), style));
    };

    while i < chars.len() {
        match kind {
            InlineKind::Plain => {
                if chars[i] == '`' {
                    flush(&mut buf, kind, &mut spans);
                    kind = InlineKind::Code;
                    i += 1;
                } else if chars[i] == '*' && i + 1 < chars.len() && chars[i + 1] == '*' {
                    flush(&mut buf, kind, &mut spans);
                    kind = InlineKind::Strong;
                    i += 2;
                } else if chars[i] == '*' || chars[i] == '_' {
                    flush(&mut buf, kind, &mut spans);
                    kind = InlineKind::Em;
                    i += 1;
                } else {
                    buf.push(chars[i]);
                    i += 1;
                }
            }
            InlineKind::Strong => {
                if chars[i] == '*' && i + 1 < chars.len() && chars[i + 1] == '*' {
                    flush(&mut buf, kind, &mut spans);
                    kind = InlineKind::Plain;
                    i += 2;
                } else {
                    buf.push(chars[i]);
                    i += 1;
                }
            }
            InlineKind::Em => {
                if chars[i] == '*' || chars[i] == '_' {
                    flush(&mut buf, kind, &mut spans);
                    kind = InlineKind::Plain;
                    i += 1;
                } else {
                    buf.push(chars[i]);
                    i += 1;
                }
            }
            InlineKind::Code => {
                if chars[i] == '`' {
                    flush(&mut buf, kind, &mut spans);
                    kind = InlineKind::Plain;
                    i += 1;
                } else {
                    buf.push(chars[i]);
                    i += 1;
                }
            }
        }
    }
    flush(&mut buf, kind, &mut spans);
    if spans.is_empty() {
        spans.push(Span::styled(String::new(), base));
    }
    spans
}

fn bound_styled_line(spans: Vec<Span<'static>>, width: usize) -> Line<'static> {
    let flat: String = spans.iter().map(|s| s.content.as_ref()).collect();
    let shown = present_line(&flat, width);
    if shown == flat {
        return Line::from(spans);
    }
    Line::from(Span::styled(shown, style_plain()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Modifier;

    fn has_mod(line: &Line<'_>, m: Modifier) -> bool {
        line.spans.iter().any(|s| s.style.add_modifier.contains(m))
    }

    #[test]
    fn strong_and_em_and_code() {
        let line = paint_md_line("a **b** *c* `d`", 40, style_plain());
        assert!(has_mod(&line, Modifier::BOLD));
        assert!(has_mod(&line, Modifier::UNDERLINED));
        assert!(has_mod(&line, Modifier::REVERSED));
        let flat: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(flat, "a b c d");
    }

    #[test]
    fn heading_and_list_markers_dim() {
        let h = paint_md_line("# Title", 40, style_plain());
        assert_eq!(h.spans[0].content.as_ref(), "# ");
        assert!(h.spans[0].style.add_modifier.contains(Modifier::DIM));
        assert!(has_mod(&h, Modifier::BOLD));
        let indented = paint_md_line("  # Title", 40, style_plain());
        assert!(indented.spans.iter().any(|s| s.content.as_ref() == "# "));
        let list = paint_md_line("- item", 40, style_plain());
        assert_eq!(list.spans[0].content.as_ref(), "- ");
        assert!(list.spans[0].style.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn task_list_stays_literal_text() {
        let line = paint_md_line("- [ ] not a step", 40, style_plain());
        let flat: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(flat.contains("[ ] not a step"));
    }
}
