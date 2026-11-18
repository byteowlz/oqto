//! Drawing (DESIGN.md 7, Studio `oqt/terminal-client-r1` anatomy): header bar, the
//! conversation panel, an attention line only when something needs you, the composer panel
//! and one status bar. Tokens only, no raw colours; content floats on `Surface`, bars use
//! `Bar`; the focused panel has the accent border.

use std::time::Duration;

use byteowlz_tui_kit::theme::{Theme, Token};
use byteowlz_tui_kit::widgets::panel;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::caps::ImageMode;
use crate::editor::Editor;
use crate::kitty;
use crate::markdown::{self, Run};
use crate::model::{Block, DiffLine, ToolCard, ToolStatus};
use crate::parts;
use crate::sanitize;

/// Glyphs with ASCII fallbacks (tui-design V8).
#[derive(Debug, Clone, Copy)]
pub struct Glyphs {
    /// Collapsed card.
    pub closed: &'static str,
    /// Expanded card.
    pub open: &'static str,
    /// Running dot.
    pub dot: &'static str,
    /// Separator.
    pub sep: &'static str,
    /// Streaming cursor.
    pub caret: &'static str,
    /// Ellipsis.
    pub more: &'static str,
}

/// Unicode glyphs.
pub const UNICODE: Glyphs = Glyphs {
    closed: "▸",
    open: "▾",
    dot: "●",
    sep: "·",
    caret: "▍",
    more: "…",
};
/// ASCII-only glyphs.
pub const ASCII: Glyphs = Glyphs {
    closed: ">",
    open: "v",
    dot: "*",
    sep: "-",
    caret: "_",
    more: "...",
};

/// Everything one frame shows.
pub struct Screen<'a> {
    /// Theme tokens.
    pub theme: Theme,
    /// Glyph set.
    pub glyphs: Glyphs,
    /// Header: host label.
    pub host: &'a str,
    /// Header: Session id.
    pub session: &'a str,
    /// Header: model.
    pub model: Option<String>,
    /// Header: thinking level.
    pub thinking_level: Option<String>,
    /// Transcript blocks.
    pub blocks: &'a [Block],
    /// A run is working, and since when.
    pub working: Option<Duration>,
    /// Link state in words; `None` when online.
    pub offline: Option<String>,
    /// The transcript shows the cache.
    pub cached: bool,
    /// Queued inputs.
    pub queued: usize,
    /// Cost so far.
    pub cost: Option<f64>,
    /// Tokens so far.
    pub tokens: Option<u64>,
    /// Transcript has focus.
    pub transcript_focus: bool,
    /// First transcript line shown; `None` follows the newest line.
    pub top: Option<usize>,
    /// Show thinking text.
    pub show_thinking: bool,
    /// Show full tool output.
    pub expand_tools: bool,
    /// Lines of output on a collapsed card.
    pub tool_lines: usize,
    /// The composer.
    pub editor: &'a Editor,
    /// A transient message for the status bar, and whether it is an error.
    pub notice: Option<(&'a str, bool)>,
    /// Something that needs the user.
    pub attention: Option<String>,
    /// How images are drawn.
    pub images: ImageMode,
    /// An overlay is open (images are hidden under overlays).
    pub overlay_open: bool,
}

/// Where an image goes on screen (Kitty mode) or which id it has (placeholders).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    /// Image key.
    pub key: String,
    /// Column and row on screen.
    pub x: u16,
    /// Row on screen.
    pub y: u16,
    /// Cells wide.
    pub cols: u16,
    /// Cells high.
    pub rows: u16,
}

/// What a frame produced besides cells.
#[derive(Debug, Default)]
pub struct Output {
    /// Images to place after the frame.
    pub placements: Vec<Placement>,
    /// Total transcript lines (for scrolling).
    pub transcript_lines: usize,
    /// Visible transcript rows.
    pub transcript_rows: usize,
    /// First line shown.
    pub start: usize,
    /// The earlier row is visible.
    pub earlier_visible: bool,
}

/// Image ids for placeholder cells, by key; assigned by the app.
pub type ImageIds<'a> = &'a dyn Fn(&str) -> Option<u32>;

const NAME_WIDTH: usize = 8;

fn label_runs(theme: Theme, name: &str, style: Style) -> (Vec<Run>, Vec<Run>) {
    let pad = NAME_WIDTH.saturating_sub(name.width());
    let first = vec![
        (name.to_owned(), style),
        (" ".repeat(pad + 1), theme.fg(Token::Primary)),
    ];
    let rest = vec![(" ".repeat(NAME_WIDTH + 1), theme.fg(Token::Primary))];
    (first, rest)
}

fn status_word(status: ToolStatus, duration: Option<u64>) -> (String, Token) {
    let time = duration
        .map(|ms| format!(" · {:.1}s", ms as f64 / 1000.0))
        .unwrap_or_default();
    match status {
        ToolStatus::Pending => ("pending".to_owned(), Token::Muted),
        ToolStatus::Running => ("running".to_owned(), Token::Accent),
        ToolStatus::Done => (format!("done{time}"), Token::Muted),
        ToolStatus::Failed => (format!("failed{time}"), Token::Danger),
        ToolStatus::Stopped => ("stopped".to_owned(), Token::Warning),
    }
}

fn right_aligned(
    left: Vec<Span<'static>>,
    right: Vec<Span<'static>>,
    width: usize,
) -> Line<'static> {
    let used: usize = left
        .iter()
        .chain(right.iter())
        .map(|s| s.content.width())
        .sum();
    let mut spans = left;
    if used < width {
        spans.push(Span::raw(" ".repeat(width - used)));
    }
    spans.extend(right);
    Line::from(spans)
}

fn tool_lines(card: &ToolCard, screen: &Screen<'_>, width: usize) -> Vec<Line<'static>> {
    let theme = screen.theme;
    let g = screen.glyphs;
    let expanded = screen.expand_tools;
    let (word, token) = status_word(card.status, card.duration_ms);
    let marker = if expanded || !card.diff.is_empty() {
        g.open
    } else {
        g.closed
    };
    let name = format!(" {} ", sanitize::clean_line(&card.name));
    let right = vec![Span::styled(word, theme.fg(token))];
    let room = width.saturating_sub(
        name.width() + 4 + right.iter().map(|s| s.content.width()).sum::<usize>() + 2,
    );
    let summary = truncate(&sanitize::clean_line(&card.summary), room, g.more);
    let left = vec![
        Span::styled(format!(" {marker} "), theme.fg(Token::Muted)),
        Span::styled(name, theme.on_bar_bold(Token::Primary)),
        Span::styled(format!(" {summary}"), theme.fg(Token::Primary)),
    ];
    let mut lines = vec![right_aligned(left, right, width)];
    let indent = vec![("     ".to_owned(), theme.fg(Token::Muted))];
    for line in &card.diff {
        let (sign, text, token) = match line {
            DiffLine::Removed(t) => ("- ", t, Token::Danger),
            DiffLine::Added(t) => ("+ ", t, Token::Success),
        };
        let runs = [(
            format!("{sign}{}", sanitize::clean_line(text)),
            theme.fg(token),
        )];
        lines.extend(markdown::wrap(&runs, width, &indent, &indent));
    }
    let output = sanitize::strip_ansi(&card.output);
    let output_lines: Vec<&str> = output.lines().collect();
    let keep = if expanded {
        output_lines.len()
    } else {
        screen.tool_lines
    };
    let skipped = output_lines.len().saturating_sub(keep);
    if skipped > 0 {
        lines.push(Line::from(Span::styled(
            format!("     {} {skipped} more lines · ctrl+o", g.more),
            theme.fg(Token::Muted),
        )));
    }
    for line in &output_lines[skipped..] {
        lines.extend(markdown::wrap(
            &[((*line).to_owned(), theme.fg(Token::Muted))],
            width,
            &indent,
            &indent,
        ));
    }
    lines
}

fn truncate(text: &str, width: usize, more: &str) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    let mut out = String::new();
    for c in text.chars() {
        if out.width() + more.width() + 1 > width {
            break;
        }
        out.push(c);
    }
    out.push_str(more);
    out
}

/// Lines of one block; `images` collects (line index, key, cols, rows) for image blocks.
fn block_lines(
    block: &Block,
    screen: &Screen<'_>,
    width: usize,
    images: &mut Vec<(usize, String, u16, u16)>,
    at: usize,
    ids: ImageIds<'_>,
) -> Vec<Line<'static>> {
    let theme = screen.theme;
    let g = screen.glyphs;
    let muted = theme.fg(Token::Muted);
    match block {
        Block::Earlier { loading } => {
            let text = if *loading {
                format!("{} loading earlier messages", g.sep)
            } else {
                format!(
                    "{} earlier messages · pgup or leader e reads older pages",
                    g.sep
                )
            };
            vec![Line::from(Span::styled(text, muted))]
        }
        Block::Gap { min, max, loading } => {
            let what = if *loading {
                "loading"
            } else {
                "leader g fills them"
            };
            vec![Line::from(Span::styled(
                format!(
                    "{} messages missing here (entries {min} to {max}) · {what}",
                    g.sep
                ),
                theme.fg(Token::Warning),
            ))]
        }
        Block::User {
            text,
            status,
            pending,
        } => {
            let style = if *pending {
                theme.fg(Token::Muted)
            } else {
                theme.fg(Token::Primary)
            };
            let (first, rest) = label_runs(theme, "you", theme.fg_bold(Token::Accent));
            let mut runs = vec![(sanitize::clean(text), style)];
            if !status.is_empty() {
                runs.push((format!("  {} {status}", g.sep), muted));
            }
            markdown::wrap(&runs, width, &first, &rest)
        }
        Block::Assistant {
            text,
            thinking,
            streaming,
        } => {
            let (first, rest) = label_runs(theme, "agent", theme.fg_bold(Token::Primary));
            let mut lines = Vec::new();
            if !thinking.is_empty() {
                let italic = muted.add_modifier(Modifier::ITALIC);
                let runs = if screen.show_thinking {
                    vec![(sanitize::clean(thinking), italic)]
                } else {
                    vec![(
                        format!(
                            "thinking {} {} chars · ctrl+t",
                            g.sep,
                            thinking.chars().count()
                        ),
                        italic,
                    )]
                };
                lines.extend(markdown::wrap(&runs, width, &first, &rest));
            }
            let body = markdown::render(text, theme, width.saturating_sub(NAME_WIDTH + 1));
            for (index, line) in body.into_iter().enumerate() {
                let prefix = if index == 0 && lines.is_empty() {
                    &first
                } else {
                    &rest
                };
                let mut spans: Vec<Span<'static>> = prefix
                    .iter()
                    .map(|(t, s)| Span::styled(t.clone(), *s))
                    .collect();
                spans.extend(line.spans);
                lines.push(Line::from(spans));
            }
            if *streaming {
                match lines.last_mut() {
                    Some(line) if !text.is_empty() => line
                        .spans
                        .push(Span::styled(g.caret, theme.fg(Token::Accent))),
                    _ => lines.push(Line::from(vec![
                        Span::styled(first[0].0.clone(), first[0].1),
                        Span::raw(" ".repeat(NAME_WIDTH + 1 - first[0].0.width())),
                        Span::styled(g.caret, theme.fg(Token::Accent)),
                    ])),
                }
            }
            lines
        }
        Block::Tool(card) => tool_lines(card, screen, width),
        Block::Image(image) => {
            let size = kitty::png_size(&image.bytes);
            let mime = image.mime.trim_start_matches("image/");
            let text_line = |why: &str| {
                let dims = size.map(|(w, h)| format!(" {w}x{h}")).unwrap_or_default();
                vec![Line::from(Span::styled(
                    format!("     [image {mime}{dims} {} {why}]", g.sep),
                    muted,
                ))]
            };
            match (screen.images, size) {
                (ImageMode::Text, _) => text_line("text terminal"),
                (_, None) => text_line("not a PNG"),
                (_, _) if screen.overlay_open && screen.images == ImageMode::Kitty => {
                    let (w, h) = size.unwrap_or((1, 1));
                    let (_, rows) = kitty::cells(
                        w,
                        h,
                        u16::try_from(width.saturating_sub(6)).unwrap_or(40).min(40),
                    );
                    (0..rows).map(|_| Line::default()).collect()
                }
                (mode, Some((w, h))) => {
                    let (cols, rows) = kitty::cells(
                        w,
                        h,
                        u16::try_from(width.saturating_sub(6)).unwrap_or(40).min(40),
                    );
                    images.push((at, image.key.clone(), cols, rows));
                    if mode == ImageMode::Placeholders
                        && let Some(id) = ids(&image.key)
                    {
                        return (0..rows)
                            .map(|row| {
                                let mut spans = vec![Span::raw("     ")];
                                for col in 0..cols {
                                    let (cell, color) = kitty::placeholder_cell(id, row, col);
                                    spans.push(Span::styled(cell, Style::new().fg(color)));
                                }
                                Line::from(spans)
                            })
                            .collect();
                    }
                    (0..rows).map(|_| Line::default()).collect()
                }
            }
        }
        Block::Note {
            kind,
            text,
            pending,
        } => {
            let (first, rest) = label_runs(theme, "note", theme.fg_bold(Token::Info));
            let mut runs = vec![(
                sanitize::clean(text),
                if *pending {
                    muted
                } else {
                    theme.fg(Token::Primary)
                },
            )];
            runs.push((
                format!(
                    "  {} {}{}",
                    g.sep,
                    sanitize::clean_line(kind),
                    if *pending { ", saving" } else { "" }
                ),
                muted,
            ));
            markdown::wrap(&runs, width, &first, &rest)
        }
        Block::Part { data } => parts::lines(data, screen, width),
        Block::Queued { text, mode } => {
            let (first, rest) = label_runs(theme, "queued", muted.add_modifier(Modifier::BOLD));
            let runs = [
                (sanitize::clean(text), muted),
                (format!("  {} {mode}", g.sep), muted),
            ];
            markdown::wrap(&runs, width, &first, &rest)
        }
        Block::Failure { reason, detail } => {
            let mut text = format!("run stopped: {}", sanitize::clean_line(reason));
            if !detail.is_empty() {
                text.push_str(&format!(" {} {}", g.sep, sanitize::clean_line(detail)));
            }
            markdown::wrap(&[(text, theme.fg(Token::Danger))], width, &[], &[])
        }
        Block::Other { label } => vec![Line::from(Span::styled(
            format!("{} {}", g.sep, sanitize::clean_line(label)),
            muted,
        ))],
    }
}

fn header(frame: &mut Frame<'_>, area: Rect, screen: &Screen<'_>) {
    let theme = screen.theme;
    frame.render_widget(Paragraph::new("").style(theme.bg(Token::Bar)), area);
    let g = screen.glyphs;
    let left = vec![
        Span::styled(" oqt ", theme.on_bar_bold(Token::Accent)),
        Span::styled(
            format!(" {} ", sanitize::clean_line(screen.session)),
            theme.on_bar_bold(Token::Primary),
        ),
        Span::styled(
            format!("{} {}", g.sep, sanitize::clean_line(screen.host)),
            theme.on_bar(Token::Muted),
        ),
    ];
    let model = screen
        .model
        .as_deref()
        .map(sanitize::clean_line)
        .unwrap_or_else(|| "model unknown".to_owned());
    let thinking = screen
        .thinking_level
        .as_deref()
        .map(|t| format!(" {} {}", g.sep, sanitize::clean_line(t)))
        .unwrap_or_default();
    let right = vec![Span::styled(
        format!("{model}{thinking} "),
        theme.on_bar(Token::Primary),
    )];
    frame.render_widget(
        Paragraph::new(right_aligned(left, right, usize::from(area.width)))
            .style(theme.bg(Token::Bar)),
        area,
    );
}

fn status_bar(frame: &mut Frame<'_>, area: Rect, screen: &Screen<'_>, narrow: bool) {
    let theme = screen.theme;
    let g = screen.glyphs;
    frame.render_widget(Paragraph::new("").style(theme.bg(Token::Bar)), area);
    let mut left: Vec<Span<'static>> = Vec::new();
    if let Some(reason) = &screen.offline {
        left.push(Span::styled(
            format!(" {} offline ", g.dot),
            theme.on_bar_bold(Token::Warning),
        ));
        left.push(Span::styled(
            truncate(&sanitize::clean_line(reason), 36, g.more),
            theme.on_bar(Token::Muted),
        ));
    } else if let Some(elapsed) = screen.working {
        let secs = elapsed.as_secs();
        left.push(Span::styled(
            format!(" {} working {}:{:02} ", g.dot, secs / 60, secs % 60),
            theme.on_bar_bold(Token::Accent),
        ));
    } else {
        left.push(Span::styled(
            " idle ".to_owned(),
            theme.on_bar(Token::Muted),
        ));
    }
    if screen.cached {
        left.push(Span::styled(
            "  cached".to_owned(),
            theme.on_bar(Token::Info),
        ));
    }
    if narrow {
        left.push(Span::styled(
            format!(" {}", sanitize::clean_line(screen.session)),
            theme.on_bar(Token::Primary),
        ));
    }
    if let Some(tokens) = screen.tokens.filter(|t| *t > 0) {
        left.push(Span::styled(
            format!("  {tokens} tok"),
            theme.on_bar(Token::Primary),
        ));
    }
    if let Some(cost) = screen.cost.filter(|c| *c > 0.0) {
        left.push(Span::styled(
            format!("  ${cost:.2}"),
            theme.on_bar(Token::Primary),
        ));
    }
    if screen.queued > 0 {
        left.push(Span::styled(
            format!("  queued {}", screen.queued),
            theme.on_bar(Token::Muted),
        ));
    }
    let right: Vec<Span<'static>> = match screen.notice {
        Some((text, danger)) => {
            let token = if danger { Token::Danger } else { Token::Info };
            vec![Span::styled(
                format!(
                    "{} ",
                    truncate(
                        &sanitize::clean_line(text),
                        usize::from(area.width) / 2,
                        g.more
                    )
                ),
                theme.on_bar(token),
            )]
        }
        None if screen.working.is_some() => vec![Span::styled(
            "esc stop  / commands ".to_owned(),
            theme.on_bar(Token::Muted),
        )],
        None => vec![Span::styled(
            "/ commands  ^space more ".to_owned(),
            theme.on_bar(Token::Muted),
        )],
    };
    frame.render_widget(
        Paragraph::new(right_aligned(left, right, usize::from(area.width)))
            .style(theme.bg(Token::Bar)),
        area,
    );
}

fn empty_state(screen: &Screen<'_>) -> Vec<Line<'static>> {
    let theme = screen.theme;
    let muted = theme.fg(Token::Muted);
    let mut lines = vec![
        Line::from(Span::styled(
            format!("Session {}", sanitize::clean_line(screen.session)),
            theme.fg_bold(Token::Primary),
        )),
        Line::from(Span::styled(
            format!("host {}", sanitize::clean_line(screen.host)),
            muted,
        )),
        Line::default(),
    ];
    let state = match &screen.offline {
        Some(reason) => format!(
            "Not connected: {}. Retrying; what you type is sent once the host is back.",
            sanitize::clean_line(reason)
        ),
        None => "No messages yet.".to_owned(),
    };
    lines.push(Line::from(Span::styled(state, theme.fg(Token::Primary))));
    lines.push(Line::default());
    for (key, what) in [
        ("enter", "send a message"),
        ("alt+enter", "queue a follow-up"),
        ("/", "commands and keys"),
        ("ctrl+space ?", "all keys"),
    ] {
        lines.push(Line::from(vec![
            Span::styled(format!("{key:>14}  "), theme.fg_bold(Token::Accent)),
            Span::styled(what, muted),
        ]));
    }
    lines
}

/// Draw the main screen.
pub fn draw(frame: &mut Frame<'_>, screen: &Screen<'_>, ids: ImageIds<'_>) -> Output {
    let area = frame.area();
    let narrow = area.height < 15;
    let width = usize::from(area.width.saturating_sub(4)).max(10);
    let (draft_lines, cursor) = screen.editor.layout(width);
    let composer_height = u16::try_from(draft_lines.len().clamp(1, 6)).unwrap_or(1) + 2;
    let attention = u16::from(screen.attention.is_some());
    let header_height = u16::from(!narrow);
    let [head, body, attention_area, composer, status] = Layout::vertical([
        Constraint::Length(header_height),
        Constraint::Min(3),
        Constraint::Length(attention),
        Constraint::Length(composer_height),
        Constraint::Length(1),
    ])
    .areas(area);
    if !narrow {
        header(frame, head, screen);
    }
    let output = transcript(frame, body, screen, ids);
    if let Some(text) = &screen.attention {
        let room = usize::from(attention_area.width).saturating_sub(3);
        let text = truncate(&sanitize::clean_line(text), room, screen.glyphs.more);
        let line = Line::from(vec![Span::styled(
            format!(" ! {text}"),
            screen.theme.fg_bold(Token::Warning),
        )]);
        frame.render_widget(Paragraph::new(line), attention_area);
    }
    composer_panel(frame, composer, screen, &draft_lines, cursor);
    status_bar(frame, status, screen, narrow);
    output
}

fn transcript(frame: &mut Frame<'_>, area: Rect, screen: &Screen<'_>, ids: ImageIds<'_>) -> Output {
    let theme = screen.theme;
    let block = panel("conversation ", theme, screen.transcript_focus);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = usize::from(inner.width).max(10);
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut images = Vec::new();
    let mut earlier_line = None;
    for (index, block) in screen.blocks.iter().enumerate() {
        if index > 0
            && !matches!(
                block,
                Block::Tool(_) | Block::Image(_) | Block::Queued { .. }
            )
        {
            lines.push(Line::default());
        }
        if matches!(block, Block::Earlier { .. }) {
            earlier_line = Some(lines.len());
        }
        let at = lines.len();
        lines.extend(block_lines(block, screen, width, &mut images, at, ids));
    }
    if screen.blocks.is_empty() {
        lines = empty_state(screen);
    }
    let rows = usize::from(inner.height);
    let total = lines.len();
    let max_start = total.saturating_sub(rows);
    let start = screen.top.map_or(max_start, |top| top.min(max_start));
    let end = (start + rows).min(total);
    let visible: Vec<Line<'static>> = lines[start..end].to_vec();
    frame.render_widget(Paragraph::new(visible), inner);
    let mut placements = Vec::new();
    if screen.images == ImageMode::Kitty && !screen.overlay_open {
        for (line, key, cols, image_rows) in images {
            // Only fully visible images are placed; a cut image shows its empty rows.
            if line >= start && line + usize::from(image_rows) <= end {
                let y = inner.y + u16::try_from(line - start).unwrap_or(0);
                placements.push(Placement {
                    key,
                    x: inner.x + 5,
                    y,
                    cols,
                    rows: image_rows,
                });
            }
        }
    }
    Output {
        placements,
        transcript_lines: total,
        transcript_rows: rows,
        start,
        earlier_visible: earlier_line.is_some_and(|l| l >= start && l < end),
    }
}

fn composer_panel(
    frame: &mut Frame<'_>,
    area: Rect,
    screen: &Screen<'_>,
    draft: &[String],
    cursor: (u16, u16),
) {
    let theme = screen.theme;
    let hint = if screen.working.is_some() {
        " enter steer · alt+enter follow-up "
    } else {
        " enter send · alt+enter queue "
    };
    let block = panel("message ", theme, !screen.transcript_focus)
        .title_top(Line::from(Span::styled(hint, theme.fg(Token::Muted))).right_aligned());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let rows = usize::from(inner.height).max(1);
    let first = usize::from(cursor.1).saturating_sub(rows - 1);
    let visible: Vec<Line<'static>> = draft
        .iter()
        .skip(first)
        .take(rows)
        .map(|l| Line::from(l.clone()))
        .collect();
    frame.render_widget(
        Paragraph::new(visible).style(theme.fg(Token::Primary)),
        inner,
    );
    if !screen.transcript_focus && !screen.overlay_open {
        let y = inner.y + cursor.1.saturating_sub(u16::try_from(first).unwrap_or(0));
        frame.set_cursor_position((inner.x + cursor.0.min(inner.width.saturating_sub(1)), y));
    }
}

/// A centered overlay with lines (help, model).
pub fn overlay(frame: &mut Frame<'_>, theme: Theme, title: &str, lines: Vec<Line<'static>>) {
    let area = frame.area();
    let width = area.width.saturating_sub(4).min(80);
    let height = u16::try_from(lines.len() + 2)
        .unwrap_or(u16::MAX)
        .min(area.height.saturating_sub(2));
    let rect = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = panel(title, theme, true).style(theme.bg(Token::Surface));
    let block =
        block.title_top(Line::from(Span::styled(" esc ", theme.fg(Token::Muted))).right_aligned());
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    frame.render_widget(Paragraph::new(lines), inner);
}

/// A titled panel behind the kit's palette (which draws no frame of its own), one cell
/// larger than the area the palette uses, so the overlay reads as one region (V1).
pub fn palette_frame(frame: &mut Frame<'_>, theme: Theme) {
    let inner = byteowlz_tui_kit::widgets::centered_rect(70, 50, frame.area());
    let area = frame.area();
    let outer = Rect {
        x: inner.x.saturating_sub(2).max(area.x),
        y: inner.y.saturating_sub(1).max(area.y),
        width: (inner.width + 4).min(area.width),
        height: (inner.height + 2).min(area.height),
    };
    frame.render_widget(Clear, outer);
    frame.render_widget(panel("commands ", theme, true), outer);
}

/// The WhichKey hint above the status bar.
pub fn hint_area(area: Rect) -> Rect {
    Rect {
        x: area.x,
        y: area.bottom().saturating_sub(2),
        width: area.width,
        height: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Image, ToolCard};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;
    use std::sync::Arc;

    fn fixture() -> Vec<Block> {
        vec![
            Block::Earlier { loading: false },
            Block::User {
                text: "Fix the flaky test".into(),
                status: "answered".into(),
                pending: false,
            },
            Block::Assistant {
                text: "The test races the **fork** worker.".into(),
                thinking: "hmm".into(),
                streaming: false,
            },
            Block::Tool(ToolCard {
                call_id: "c1".into(),
                name: "edit".into(),
                summary: "src/fork.rs  +1 -1".into(),
                status: ToolStatus::Done,
                output: "ok".into(),
                diff: vec![
                    DiffLine::Removed("let id = fork.id();".into()),
                    DiffLine::Added("let id = observed.pin()?;".into()),
                ],
                duration_ms: Some(2100),
            }),
            Block::Tool(ToolCard {
                call_id: "c2".into(),
                name: "bash".into(),
                summary: "cargo test".into(),
                status: ToolStatus::Running,
                output: "line 1\nline 2\nline 3\nline 4\n\u{1b}]52;c;AAAA\u{7}line 5".into(),
                diff: vec![],
                duration_ms: None,
            }),
            Block::Image(Image {
                key: "9:0".into(),
                mime: "image/png".into(),
                bytes: Arc::new(vec![
                    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, 0x49, 0x48, 0x44,
                    0x52, 0, 0, 2, 0x80, 0, 0, 1, 0x40,
                ]),
            }),
            Block::Queued {
                text: "then commit".into(),
                mode: "follow-up".into(),
            },
            Block::Assistant {
                text: "Running".into(),
                thinking: String::new(),
                streaming: true,
            },
        ]
    }

    fn screen<'a>(blocks: &'a [Block], editor: &'a Editor) -> Screen<'a> {
        Screen {
            theme: Theme::ansi_default(),
            glyphs: UNICODE,
            host: "local (127.0.0.1:8787)",
            session: "ses_demo",
            model: Some("demo/scripted".into()),
            thinking_level: Some("high".into()),
            blocks,
            working: Some(Duration::from_secs(41)),
            offline: None,
            cached: false,
            queued: 1,
            cost: Some(0.82),
            tokens: Some(1200),
            transcript_focus: false,
            top: None,
            show_thinking: false,
            expand_tools: false,
            tool_lines: 3,
            editor,
            notice: None,
            attention: None,
            images: ImageMode::Text,
            overlay_open: false,
        }
    }

    fn render(
        screen: &Screen<'_>,
        width: u16,
        height: u16,
    ) -> (Vec<String>, ratatui::buffer::Buffer) {
        let mut terminal = match Terminal::new(TestBackend::new(width, height)) {
            Ok(t) => t,
            Err(e) => panic!("{e}"),
        };
        if let Err(e) = terminal.draw(|f| {
            draw(f, screen, &|_| Some(3));
        }) {
            panic!("{e}");
        }
        let buffer = terminal.backend().buffer().clone();
        let lines = (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol().to_owned())
                    .collect::<String>()
            })
            .collect();
        (lines, buffer)
    }

    #[test]
    fn normal_screen_has_bars_panels_and_content() {
        let blocks = fixture();
        let mut editor = Editor::default();
        editor.insert("Also check macOS");
        let (lines, buffer) = render(&screen(&blocks, &editor), 100, 36);
        let text = lines.join("\n");
        assert!(lines[0].starts_with(" oqt  ses_demo"), "{}", lines[0]);
        assert!(lines[0].contains("demo/scripted · high"));
        assert!(text.contains("╭ conversation"));
        assert!(text.contains("earlier messages"));
        assert!(text.contains("you      Fix the flaky test  · answered"));
        assert!(text.contains("thinking · 3 chars · ctrl+t"));
        assert!(text.contains("- let id = fork.id();"));
        assert!(text.contains("+ let id = observed.pin()?;"));
        assert!(text.contains("… 2 more lines · ctrl+o"));
        assert!(text.contains("[image png 640x320 · text terminal]"));
        assert!(text.contains("queued   then commit  · follow-up"));
        assert!(text.contains("Also check macOS"));
        assert!(lines[35].contains("● working 0:41"));
        assert!(lines[35].contains("$0.82"));
        // Content escapes never reach the cells.
        assert!(!text.contains('\u{1b}'));
        // Bars are filled with the Bar token, content floats on Surface.
        assert_eq!(buffer[(50, 0)].bg, Color::Black);
        assert_eq!(buffer[(50, 35)].bg, Color::Black);
        assert_eq!(buffer[(50, 5)].bg, Color::Reset);
        // The focused composer has the accent border, the transcript the muted one.
        assert_eq!(buffer[(0, 1)].fg, Color::DarkGray);
        assert_eq!(buffer[(0, 32)].fg, Color::Blue);
    }

    #[test]
    fn empty_state_is_never_blank() {
        let editor = Editor::default();
        let mut s = screen(&[], &editor);
        s.offline = Some("connect: connection refused".into());
        s.working = None;
        let (lines, _) = render(&s, 80, 24);
        let text = lines.join("\n");
        assert!(text.contains("Session ses_demo"));
        assert!(text.contains("Not connected: connect: connection refused"));
        assert!(text.contains("send a message"));
        assert!(lines[23].contains("offline"));
    }

    #[test]
    fn narrow_terminal_merges_the_header_into_the_status_bar() {
        let blocks = fixture();
        let editor = Editor::default();
        let (lines, _) = render(&screen(&blocks, &editor), 80, 12);
        assert!(lines[0].starts_with("╭ conversation"), "{}", lines[0]);
        assert!(lines[11].contains("ses_demo"));
    }

    #[test]
    fn placeholders_draw_unicode_cells_with_the_image_id() {
        let blocks = fixture();
        let editor = Editor::default();
        let mut s = screen(&blocks, &editor);
        s.images = ImageMode::Placeholders;
        let (lines, buffer) = render(&s, 100, 60);
        let row = lines.iter().position(|l| l.contains('\u{10EEEE}'));
        let Some(row) = row else {
            panic!("no placeholder row")
        };
        let x = lines[row]
            .chars()
            .position(|c| c == '\u{10EEEE}')
            .unwrap_or_default();
        let cell = &buffer[(
            u16::try_from(x).unwrap_or(0),
            u16::try_from(row).unwrap_or(0),
        )];
        assert!(cell.symbol().starts_with('\u{10EEEE}'));
        assert_eq!(cell.fg, Color::Indexed(3));
    }

    #[test]
    fn kitty_images_are_placed_only_without_overlays() {
        let blocks = fixture();
        let editor = Editor::default();
        let mut s = screen(&blocks, &editor);
        s.images = ImageMode::Kitty;
        let mut terminal = match Terminal::new(TestBackend::new(100, 60)) {
            Ok(t) => t,
            Err(e) => panic!("{e}"),
        };
        let mut placed = Output::default();
        let _ = terminal.draw(|f| placed = draw(f, &s, &|_| None));
        assert_eq!(placed.placements.len(), 1);
        s.overlay_open = true;
        let _ = terminal.draw(|f| placed = draw(f, &s, &|_| None));
        assert!(placed.placements.is_empty());
    }
}
