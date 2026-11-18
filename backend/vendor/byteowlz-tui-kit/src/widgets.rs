//! Modern, reusable ratatui widgets and helpers.
//!
//! These embody the visual rules: [`Selection`] state is first-class and orthogonal;
//! [`draw_status_bar`] is one quiet line; [`draw_empty_state`] never shows blank;
//! [`centered_rect`] is the popup primitive. All take a [`Theme`][crate::theme::Theme]
//! and reference [`Token`][crate::theme::Token]s, never raw colors.

use std::collections::HashSet;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Padding, Paragraph,
};

use crate::theme::{Theme, Token};

/// First-class, cursor-orthogonal selection state for a list.
///
/// Holds the cursor (`index`), the viewport (`offset`), and the multi-select set. The
/// selection set is **independent** of the cursor, so `Space`-toggle, `Ctrl-A`-all, and
/// `V`-range all work regardless of where the cursor sits.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    /// Cursor position.
    index: usize,
    /// Viewport scroll offset.
    offset: usize,
    /// The multi-selected indices.
    selected: HashSet<usize>,
}

impl Selection {
    /// The cursor index.
    #[must_use]
    pub const fn index(&self) -> usize {
        self.index
    }

    /// The viewport offset.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    /// The selected indices, sorted ascending.
    #[must_use]
    pub fn selected(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self.selected.iter().copied().collect();
        v.sort_unstable();
        v
    }

    /// Whether any items are selected.
    #[must_use]
    pub fn has_selection(&self) -> bool {
        !self.selected.is_empty()
    }

    /// Move the cursor down, keeping it in `[0, max)`.
    pub const fn next(&mut self, max: usize) {
        if max == 0 {
            return;
        }
        self.index = (self.index + 1) % max;
    }

    /// Move the cursor up, keeping it in `[0, max)`.
    pub const fn previous(&mut self, max: usize) {
        if max == 0 {
            return;
        }
        self.index = (self.index + max - 1) % max;
    }

    /// Jump the cursor to the top.
    pub const fn top(&mut self) {
        self.index = 0;
        self.offset = 0;
    }

    /// Jump the cursor to the bottom.
    pub const fn bottom(&mut self, max: usize) {
        if max == 0 {
            return;
        }
        self.index = max - 1;
    }

    /// Toggle selection on the current cursor index.
    pub fn toggle(&mut self) {
        if self.selected.contains(&self.index) {
            self.selected.remove(&self.index);
        } else {
            self.selected.insert(self.index);
        }
    }

    /// Select every index in `[0, max)`.
    pub fn select_all(&mut self, max: usize) {
        self.selected.clear();
        self.selected.extend(0..max);
    }

    /// Clear the selection set (cursor is untouched).
    pub fn deselect_all(&mut self) {
        self.selected.clear();
    }

    /// Whether the given index is selected.
    #[must_use]
    pub fn is_selected(&self, index: usize) -> bool {
        self.selected.contains(&index)
    }

    /// A [`ListState`] tracking this cursor, for rendering with ratatui's `List`.
    #[must_use]
    pub fn state(&self) -> ListState {
        let mut state = ListState::default();
        state.select(Some(self.index));
        state
    }
}

/// Build a [`List`] of plain strings, applying the theme's focus style to the cursor row
/// and a subtle marker to multi-selected rows.
///
/// The list itself has no block; render it inside a [`panel`] to get the titled rounded
/// border. Pass the [`Selection`] so the cursor and selection highlight correctly.
#[must_use]
pub fn styled_list<'a>(items: &'a [&'a str], selection: &Selection, theme: Theme) -> List<'a> {
    let list_items: Vec<ListItem<'a>> = items
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let marker = if selection.is_selected(i) {
                "● "
            } else {
                "  "
            };
            ListItem::new(format!("{marker}{text}"))
        })
        .collect();
    List::new(list_items)
        .highlight_style(theme.focus())
        .highlight_symbol("▌ ")
}

/// A titled, rounded-border panel — the primary container for a region.
///
/// `active` controls the border color: `Accent` for the focused panel, `Muted` otherwise.
/// The title sits in the top border. Returns the block; render your content inside it.
///
/// This is the modern byteowlz panel: thin styled border + a fill-free interior, not a
/// heavy double box and not a borderless void.
#[must_use]
pub fn panel(title: &str, theme: Theme, active: bool) -> Block<'_> {
    let border = if active {
        theme.border_focus()
    } else {
        theme.border()
    };
    let title_span = Line::from(vec![
        Span::styled(" ", border),
        Span::styled(title, theme.fg_bold(Token::Primary)),
    ]);
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border)
        .title_top(title_span)
        .padding(Padding::horizontal(1))
}

/// A full-width header or status strip with a `Bar` background fill.
///
/// Pass a pre-built [`Line`] (usually of [`crate::theme::Theme::on_bar`] spans) so the
/// text contrasts on the filled strip. Returns the paragraph ready to render.
#[must_use]
pub fn bar(line: Line<'_>, theme: Theme) -> Paragraph<'_> {
    Paragraph::new(line).style(theme.bg(Token::Bar))
}

/// Render a filled status line: counts on the left, key hints on the right, on a `Bar`
/// background. The single quiet chrome line at the bottom of the app (rule V5).
pub fn draw_status_bar(
    frame: &mut Frame<'_>,
    area: Rect,
    theme: Theme,
    left: &str,
    hints: &[(&str, &str)],
) {
    frame.render_widget(Paragraph::new("").style(theme.bg(Token::Bar)), area);
    let [left_area, right_area] = split_status(area);

    let left_line = Line::from(vec![Span::styled(left, theme.on_bar(Token::Muted))]);
    frame.render_widget(Paragraph::new(left_line), left_area);

    let mut spans: Vec<Span<'_>> = Vec::with_capacity(hints.len() * 3);
    for (i, (key, label)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("   "));
        }
        spans.push(Span::styled(*key, theme.on_bar_bold(Token::Primary)));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(*label, theme.on_bar(Token::Muted)));
    }
    let right_line = Line::from(spans).right_aligned();
    frame.render_widget(Paragraph::new(right_line), right_area);
}

/// Split a status area into left/right halves along the horizontal axis.
fn split_status(area: Rect) -> [Rect; 2] {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);
    [chunks[0], chunks[1]]
}

/// Render an empty state — never show a blank pane (rule IA4).
///
/// `prompt` is the call to action, e.g. `"No items yet — press 'a' to add."`.
pub fn draw_empty_state(frame: &mut Frame<'_>, area: Rect, theme: Theme, prompt: &str) {
    let block = Block::default().borders(Borders::NONE);
    let para = Paragraph::new(prompt)
        .block(block)
        .alignment(ratatui::layout::Alignment::Center)
        .style(theme.fg(Token::Muted).add_modifier(Modifier::DIM));
    frame.render_widget(para, area);
}

/// Clear a region before drawing an overlay on top (so the base shows through).
pub fn clear_area(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(Clear, area);
}

/// A centered rectangle of `(percent_x, percent_y)` of the available area.
///
/// The popup primitive used by all overlays (palette, confirm, help, menus).
#[must_use]
pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(u16::saturating_sub(100, percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage(u16::saturating_sub(100, percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(u16::saturating_sub(100, percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage(u16::saturating_sub(100, percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
