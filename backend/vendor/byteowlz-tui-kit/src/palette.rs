//! The fuzzy command palette — the type-to-discover path over the action table.
//!
//! Triggered by `:` (vim idiom) or `Ctrl-P`, the palette lists every [`Action`][crate::action::Action]
//! in scope, fuzzy-filtered by the typed query, each with its Key Path shown. It is the
//! replacement for the static, unsearchable WhichKey bar and for action-modes. It is an
//! overlay: `Esc` closes it, `Enter` runs the selected action, `Up`/`Down` move.

use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};

use crate::action::{Action, ActionId, Key};
use crate::fuzzy::fuzzy_indices;
use crate::theme::{Theme, Token};
use crate::widgets::centered_rect;

/// The outcome of feeding a key to the palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteOutcome {
    /// The palette is still open; redraw it.
    Open,
    /// The palette was closed (Escape) with no action.
    Closed,
    /// An action was selected and should be run.
    Run(ActionId),
}

/// The fuzzy command palette state.
///
/// Owns the query buffer and the cursor over filtered results. Construct per-overlay
/// session with [`CommandPalette::new`], then drive with [`CommandPalette::handle`] and
/// render with [`CommandPalette::draw`].
#[derive(Debug)]
pub struct CommandPalette {
    actions: Vec<Action>,
    query: String,
    state: ListState,
}

/// A filtered entry: the action and its match positions (for highlight).
struct Filtered {
    action: ActionId,
    label: String,
    key_hint: String,
    positions: Vec<usize>,
}

impl CommandPalette {
    /// Create a palette over the given actions. The actions are borrowed for the
    /// lifetime of the filter, so pass an owned slice clone if needed.
    #[must_use]
    pub fn new(actions: Vec<Action>) -> Self {
        let mut state = ListState::default();
        state.select(Some(0));
        Self {
            actions,
            query: String::new(),
            state,
        }
    }

    /// Feed a normalized key and return the resulting outcome.
    pub fn handle(&mut self, key: Key) -> PaletteOutcome {
        match key {
            k if k == Key::esc() => PaletteOutcome::Closed,
            k if k == Key::enter() => self
                .selected_action()
                .map_or(PaletteOutcome::Closed, PaletteOutcome::Run),
            k if k == Key::up() => {
                self.move_cursor(-1);
                PaletteOutcome::Open
            }
            k if k == Key::down() => {
                self.move_cursor(1);
                PaletteOutcome::Open
            }
            k if k == Key::backspace() => {
                self.query.pop();
                self.state.select(Some(0));
                PaletteOutcome::Open
            }
            Key {
                code: KeyCode::Char(c),
                ..
            } => {
                self.query.push(c);
                self.state.select(Some(0));
                PaletteOutcome::Open
            }
            _ => PaletteOutcome::Open,
        }
    }

    /// Render the palette as a centered overlay.
    pub fn draw(&mut self, frame: &mut Frame<'_>, theme: Theme) {
        let area = centered_rect(70, 50, frame.area());
        frame.render_widget(Clear, area);
        let filtered = self.filtered();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .split(area);

        self.draw_input(frame, theme, chunks[0]);
        self.draw_results(frame, theme, chunks[1], &filtered);
        self.draw_footer(frame, theme, chunks[2]);
    }

    /// Move the cursor by `delta`, clamping to the filtered result count.
    fn move_cursor(&mut self, delta: i32) {
        let count = self.filtered().len();
        if count == 0 {
            self.state.select(None);
            return;
        }
        let current = self.state.selected().unwrap_or(0);
        let mut next = current as i32 + delta;
        if next < 0 {
            next = 0;
        }
        if next as usize >= count {
            next = (count - 1) as i32;
        }
        self.state.select(Some(next as usize));
    }

    /// The action id under the current cursor, if any.
    fn selected_action(&self) -> Option<ActionId> {
        let filtered = self.filtered();
        let idx = self.state.selected()?;
        filtered.get(idx).map(|f| f.action)
    }

    /// Compute the filtered, query-matched entries.
    fn filtered(&self) -> Vec<Filtered> {
        let mut out: Vec<Filtered> = self
            .actions
            .iter()
            .filter_map(|action| filter_action(action, &self.query))
            .collect();
        // Matches with earlier / shorter labels rank first for a stable feel.
        out.sort_by_key(|f| (f.positions.is_empty(), f.label.len()));
        out
    }

    /// Draw the input row with the query and a blinking-style cursor marker.
    fn draw_input(&self, frame: &mut Frame<'_>, theme: Theme, area: Rect) {
        let block = Block::default()
            .borders(Borders::BOTTOM)
            .border_style(theme.fg(Token::Muted));
        let prompt = Line::from(vec![
            Span::styled(": ", theme.fg_bold(Token::Accent)),
            Span::raw(&self.query),
            Span::styled("▏", theme.fg_bold(Token::Accent)),
        ]);
        frame.render_widget(Paragraph::new(prompt).block(block), area);
    }

    /// Draw the filtered result list with match highlighting.
    fn draw_results(
        &mut self,
        frame: &mut Frame<'_>,
        theme: Theme,
        area: Rect,
        filtered: &[Filtered],
    ) {
        let items: Vec<ListItem<'_>> = filtered.iter().map(|f| highlight_line(f, theme)).collect();
        let list = List::new(items)
            .highlight_style(theme.focus())
            .highlight_symbol("▶ ");
        frame.render_stateful_widget(list, area, &mut self.state);
    }

    /// Draw the footer hint line.
    fn draw_footer(&self, frame: &mut Frame<'_>, theme: Theme, area: Rect) {
        let count = self.filtered().len();
        let footer = Line::from(vec![
            Span::styled(
                format!("{count} of {} actions", self.actions.len()),
                theme.fg(Token::Muted),
            ),
            Span::raw("   "),
            Span::styled("↑↓ select · ⏎ run · esc close", theme.fg(Token::Muted)),
        ]);
        let para = Paragraph::new(footer).alignment(Alignment::Left);
        frame.render_widget(para, area);
    }
}

/// Build one filtered entry for an action, or `None` if it does not match the query.
fn filter_action(action: &Action, query: &str) -> Option<Filtered> {
    let positions = fuzzy_indices(action.label(), query)?;
    Some(Filtered {
        action: action.id(),
        label: action.label().to_string(),
        key_hint: action.key_hint(),
        positions,
    })
}

/// Build a highlighted list line for a filtered entry.
fn highlight_line(entry: &Filtered, theme: Theme) -> ListItem<'static> {
    let label_spans = highlight_positions(&entry.label, &entry.positions, theme);
    let key_span = if entry.key_hint.is_empty() {
        Span::raw("")
    } else {
        Span::styled(format!("  {}", entry.key_hint), theme.fg(Token::Muted))
    };
    let mut spans = label_spans;
    spans.push(key_span);
    let line = Line::from(spans);
    ListItem::new(line)
}

/// Render a label with matched characters highlighted in the accent color.
fn highlight_positions(label: &str, positions: &[usize], theme: Theme) -> Vec<Span<'static>> {
    let chars: Vec<char> = label.chars().collect();
    let pos_set: std::collections::HashSet<usize> = positions.iter().copied().collect();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut current = String::new();
    let mut current_match = false;
    for (i, ch) in chars.iter().enumerate() {
        let is_match = pos_set.contains(&i);
        if is_match == current_match && !current.is_empty() {
            current.push(*ch);
        } else {
            if !current.is_empty() {
                spans.push(make_span(&current, current_match, theme));
            }
            current = ch.to_string();
            current_match = is_match;
        }
    }
    if !current.is_empty() {
        spans.push(make_span(&current, current_match, theme));
    }
    spans
}

/// Build a styled span for a run of matched or unmatched text.
fn make_span(text: &str, matched: bool, theme: Theme) -> Span<'static> {
    if matched {
        Span::styled(text.to_string(), theme.fg_bold(Token::Accent))
    } else {
        Span::styled(text.to_string(), theme.fg(Token::Primary))
    }
}
