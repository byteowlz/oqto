//! Markdown and plain text to wrapped lines. All text is cleaned first (`sanitize`), so
//! nothing from content reaches the terminal as a control sequence.

use byteowlz_tui_kit::theme::{Theme, Token};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::sanitize;

/// A styled run of text.
pub type Run = (String, Style);

/// Wrap runs to `width` cells; `first` and `rest` are line prefixes (already styled).
pub fn wrap(runs: &[Run], width: usize, first: &[Run], rest: &[Run]) -> Vec<Line<'static>> {
    let prefix_width = |p: &[Run]| p.iter().map(|(t, _)| t.width()).sum::<usize>();
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current: Vec<Span<'static>> = first
        .iter()
        .map(|(t, s)| Span::styled(t.clone(), *s))
        .collect();
    let mut used = prefix_width(first);
    let mut limit = width.max(prefix_width(first) + 4);
    let new_line = |lines: &mut Vec<Line<'static>>,
                    current: &mut Vec<Span<'static>>,
                    used: &mut usize,
                    limit: &mut usize| {
        lines.push(Line::from(std::mem::take(current)));
        *current = rest
            .iter()
            .map(|(t, s)| Span::styled(t.clone(), *s))
            .collect();
        *used = prefix_width(rest);
        *limit = width.max(*used + 4);
    };
    for (text, style) in runs {
        for (index, piece) in text.split('\n').enumerate() {
            if index > 0 {
                new_line(&mut lines, &mut current, &mut used, &mut limit);
            }
            for word in split_keep_spaces(piece) {
                let w = word.width();
                if used + w > limit && used > prefix_width(rest) && !word.trim().is_empty() {
                    new_line(&mut lines, &mut current, &mut used, &mut limit);
                    if word.trim().is_empty() {
                        continue;
                    }
                }
                if used + w > limit {
                    // A word longer than the line: cut by characters.
                    let mut chunk = String::new();
                    for c in word.chars() {
                        let cw = UnicodeWidthStr::width(c.encode_utf8(&mut [0; 4]) as &str);
                        if used + chunk.width() + cw > limit && !chunk.is_empty() {
                            current.push(Span::styled(std::mem::take(&mut chunk), *style));
                            new_line(&mut lines, &mut current, &mut used, &mut limit);
                        }
                        chunk.push(c);
                    }
                    used += chunk.width();
                    current.push(Span::styled(chunk, *style));
                    continue;
                }
                used += w;
                current.push(Span::styled(word.to_owned(), *style));
            }
        }
    }
    lines.push(Line::from(current));
    lines
}

fn split_keep_spaces(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut in_space = None;
    for (index, c) in text.char_indices() {
        let space = c == ' ';
        if in_space.is_some_and(|s| s != space) {
            out.push(&text[start..index]);
            start = index;
        }
        in_space = Some(space);
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// Plain text, cleaned and wrapped.
pub fn plain(
    text: &str,
    style: Style,
    width: usize,
    first: &[Run],
    rest: &[Run],
) -> Vec<Line<'static>> {
    wrap(&[(sanitize::clean(text), style)], width, first, rest)
}

struct State {
    theme: Theme,
    width: usize,
    lines: Vec<Line<'static>>,
    runs: Vec<Run>,
    styles: Vec<Style>,
    lists: Vec<Option<u64>>,
    quote: usize,
    code: Option<Vec<String>>,
    link: Option<String>,
    item_start: bool,
}

impl State {
    fn style(&self) -> Style {
        self.styles.last().copied().unwrap_or_default()
    }

    fn prefix(&self, first: bool) -> (Vec<Run>, Vec<Run>) {
        let muted = self.theme.fg(Token::Muted);
        let mut lead = String::new();
        for _ in 0..self.quote {
            lead.push_str("│ ");
        }
        let depth = self.lists.len().saturating_sub(1);
        let indent = "  ".repeat(depth);
        let marker = match self.lists.last() {
            Some(Some(n)) if first => format!("{indent}{n}. "),
            Some(None) if first => format!("{indent}• "),
            Some(Some(n)) => format!("{indent}{}", " ".repeat(n.to_string().len() + 2)),
            Some(None) => format!("{indent}  "),
            None => String::new(),
        };
        let one = vec![(lead.clone(), muted), (marker.clone(), muted)];
        let rest_marker = if first && !marker.is_empty() {
            " ".repeat(marker.width())
        } else {
            marker
        };
        (one, vec![(lead, muted), (rest_marker, muted)])
    }

    fn flush(&mut self) {
        if self.runs.is_empty() {
            return;
        }
        let (first, rest) = self.prefix(self.item_start);
        self.item_start = false;
        let runs = std::mem::take(&mut self.runs);
        self.lines.extend(wrap(&runs, self.width, &first, &rest));
    }

    fn blank(&mut self) {
        if self.lines.last().is_some_and(|l| l.width() > 0) {
            self.lines.push(Line::default());
        }
    }

    fn start(&mut self, tag: &Tag<'_>) {
        let theme = self.theme;
        match tag {
            Tag::Heading { level, .. } => {
                self.flush();
                self.blank();
                let style = if *level == HeadingLevel::H1 {
                    theme.fg_bold(Token::Accent)
                } else {
                    theme.fg_bold(Token::Primary)
                };
                self.styles.push(style);
            }
            Tag::Strong => self.styles.push(self.style().add_modifier(Modifier::BOLD)),
            Tag::Emphasis => self
                .styles
                .push(self.style().add_modifier(Modifier::ITALIC)),
            Tag::Strikethrough => self
                .styles
                .push(self.style().add_modifier(Modifier::CROSSED_OUT)),
            Tag::BlockQuote(_) => {
                self.flush();
                self.quote += 1;
            }
            Tag::List(start) => {
                self.flush();
                self.lists.push(*start);
            }
            Tag::Item => {
                self.flush();
                self.item_start = true;
            }
            Tag::CodeBlock(kind) => {
                self.flush();
                self.blank();
                let label = match kind {
                    CodeBlockKind::Fenced(lang) if !lang.is_empty() => sanitize::clean_line(lang),
                    _ => String::new(),
                };
                self.code = Some(vec![label]);
            }
            Tag::Link { dest_url, .. } => {
                self.link = Some(sanitize::clean_line(dest_url));
                self.styles
                    .push(self.style().add_modifier(Modifier::UNDERLINED));
            }
            Tag::Paragraph => {
                self.flush();
                if self.lists.is_empty() {
                    self.blank();
                }
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading(_) => {
                self.flush();
                self.styles.pop();
            }
            TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough => {
                self.styles.pop();
            }
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.quote = self.quote.saturating_sub(1);
            }
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
            }
            TagEnd::Item => {
                self.flush();
                if let Some(Some(n)) = self.lists.last_mut() {
                    *n += 1;
                }
            }
            TagEnd::CodeBlock => self.code_block(),
            TagEnd::Link => {
                self.styles.pop();
                if let Some(url) = self.link.take() {
                    self.runs
                        .push((format!(" ({url})"), self.theme.fg(Token::Muted)));
                }
            }
            TagEnd::Paragraph => self.flush(),
            _ => {}
        }
    }

    fn code_block(&mut self) {
        let Some(code) = self.code.take() else { return };
        let muted = self.theme.fg(Token::Muted);
        let text_style = self.theme.fg(Token::Info);
        let label = code.first().cloned().unwrap_or_default();
        if !label.is_empty() {
            self.lines
                .push(Line::from(Span::styled(format!("  {label}"), muted)));
        }
        let body: String = code.iter().skip(1).cloned().collect();
        for line in body.trim_end_matches('\n').split('\n') {
            self.lines.extend(wrap(
                &[(line.to_owned(), text_style)],
                self.width,
                &[("  ".to_owned(), muted)],
                &[("  ".to_owned(), muted)],
            ));
        }
    }

    fn text(&mut self, text: &str) {
        let clean = sanitize::clean(text);
        if let Some(code) = &mut self.code {
            code.push(clean);
            return;
        }
        let style = self.style();
        self.runs.push((clean, style));
    }
}

/// Markdown to wrapped lines.
pub fn render(text: &str, theme: Theme, width: usize) -> Vec<Line<'static>> {
    let mut state = State {
        theme,
        width,
        lines: Vec::new(),
        runs: Vec::new(),
        styles: vec![theme.fg(Token::Primary)],
        lists: Vec::new(),
        quote: 0,
        code: None,
        link: None,
        item_start: false,
    };
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
    for event in Parser::new_ext(text, options) {
        match event {
            Event::Start(tag) => state.start(&tag),
            Event::End(tag) => state.end(tag),
            Event::Text(t) => state.text(&t),
            Event::Code(code) => {
                let style = theme.fg(Token::Info);
                state.runs.push((sanitize::clean(&code), style));
            }
            Event::SoftBreak => state.runs.push((" ".to_owned(), state.style())),
            Event::HardBreak => state.runs.push(("\n".to_owned(), state.style())),
            Event::Rule => {
                state.flush();
                state.lines.push(Line::from(Span::styled(
                    "─".repeat(width.min(40)),
                    theme.fg(Token::Muted),
                )));
            }
            Event::Html(html) | Event::InlineHtml(html) => state
                .runs
                .push((sanitize::clean(&html), theme.fg(Token::Muted))),
            Event::TaskListMarker(done) => state.runs.push((
                if done { "[x] " } else { "[ ] " }.to_owned(),
                theme.fg(Token::Muted),
            )),
            _ => {}
        }
    }
    state.flush();
    while state.lines.first().is_some_and(|l| l.width() == 0) {
        state.lines.remove(0);
    }
    while state.lines.last().is_some_and(|l| l.width() == 0) {
        state.lines.pop();
    }
    state.lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(lines: &[Line<'_>]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    #[test]
    fn wraps_words_and_keeps_prefixes() {
        let lines = wrap(
            &[("one two three four".to_owned(), Style::default())],
            10,
            &[("> ".to_owned(), Style::default())],
            &[("  ".to_owned(), Style::default())],
        );
        assert_eq!(text(&lines), vec!["> one two ", "  three ", "  four"]);
    }

    #[test]
    fn lists_code_and_escapes() {
        let md =
            "# Title\n\n- a\n- b `x`\n\n```rust\nlet a = 1;\n```\n\nevil \u{1b}]52;c;AA\u{7} done";
        let out = text(&render(md, Theme::ansi_default(), 40));
        assert_eq!(out[0], "Title");
        assert!(out.contains(&"• a".to_owned()));
        assert!(out.contains(&"• b x".to_owned()));
        assert!(out.contains(&"  let a = 1;".to_owned()));
        assert!(out.iter().all(|l| !l.contains('\u{1b}')));
    }
}
