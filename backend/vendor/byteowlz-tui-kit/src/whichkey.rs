//! The transient WhichKey hint shown during a Key Progression.
//!
//! This is **not** the always-on bar from older TUIs. It appears only when a prefix is in
//! progress (`s d`-style) and reveals the next keys that complete an action. The user
//! never has to memorize the second key; they type `s`, see the options, and continue.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use crate::action::{Key, key_path_hint};
use crate::theme::{Theme, Token};

/// Render the WhichKey hint as a bottom-anchored overlay line.
///
/// `prefix_hint` is the in-progress path rendered as a compact string (e.g. `"s"`),
/// and `options` are the `(next key, label)` pairs returned by
/// [`KeyRouter::feed`][crate::action::KeyRouter].
pub fn draw_hint(
    frame: &mut Frame<'_>,
    area: Rect,
    theme: Theme,
    prefix: &[Key],
    options: &[(Key, &str)],
) {
    frame.render_widget(Clear, area);
    let line = hint_line(theme, prefix, options);
    let block = Block::default().style(Style::new());
    let para = Paragraph::new(line).block(block);
    frame.render_widget(para, area);
}

/// Build the single-line hint from a prefix and its next-key options.
fn hint_line<'a>(theme: Theme, prefix: &'a [Key], options: &'a [(Key, &str)]) -> Line<'a> {
    let mut spans: Vec<Span<'a>> = Vec::new();
    let prefix_text = key_path_hint(prefix);
    if !prefix_text.is_empty() {
        spans.push(Span::styled(
            format!("{prefix_text} "),
            theme.fg_bold(Token::Accent),
        ));
    }
    for (i, (key, label)) in options.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        let key_text = key_path_hint(std::slice::from_ref(key));
        spans.push(Span::styled(
            format!("[{key_text}]"),
            theme.fg_bold(Token::Primary),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(*label, theme.fg(Token::Muted)));
    }
    Line::from(spans)
}
