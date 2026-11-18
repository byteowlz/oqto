//! Generative UI parts (`oqto.ui` entries, session-view revision 2.2): the Oqto profile's core
//! set drawn natively in cells (card, status, table, choice, form), the part's plain-text
//! fallback for everything else. Every string is cleaned; buttons are inert because answers
//! need `oqto.interactions` (slice 3), and rendering a part grants nothing.

use byteowlz_tui_kit::theme::{Theme, Token};
use oqto_openui::{Decision, Node, Value};
use oqto_session_view::parts::{PartState, UiPart};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use serde_json::Value as Json;
use unicode_width::UnicodeWidthStr;

use crate::markdown::{self, Run};
use crate::sanitize;
use crate::ui::{Glyphs, Screen};

const LABEL: usize = 8;

struct Ctx {
    theme: Theme,
    glyphs: Glyphs,
    width: usize,
    lines: Vec<Line<'static>>,
}

fn text_of(value: Option<&Value>) -> String {
    match value {
        Some(Value::Text(t)) => sanitize::clean_line(t),
        Some(Value::Number(n)) => format_number(*n),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

fn status_token(variant: &str) -> Token {
    match variant {
        "info" => Token::Info,
        "warning" => Token::Warning,
        "error" | "danger" => Token::Danger,
        "success" => Token::Success,
        _ => Token::Muted,
    }
}

impl Ctx {
    fn push(&mut self, indent: usize, runs: Vec<Run>) {
        let pad = vec![(" ".repeat(indent), Style::default())];
        self.lines
            .extend(markdown::wrap(&runs, self.width, &pad, &pad));
    }

    fn default_of(node: &Node) -> String {
        text_of(node.prop("defaultValue").or_else(|| node.prop("value")))
    }

    fn choice_line(
        &mut self,
        indent: usize,
        selected: bool,
        kind: char,
        label: &str,
        detail: &str,
    ) {
        let ascii = self.glyphs.dot == "*";
        let mark = match (kind, selected, ascii) {
            ('r', true, false) => "(•)",
            ('r', true, true) => "(*)",
            ('r', false, _) => "( )",
            (_, true, _) => "[x]",
            (_, false, _) => "[ ]",
        };
        let mut runs = vec![
            (format!("{mark} "), self.theme.fg(Token::Accent)),
            (label.to_owned(), self.theme.fg(Token::Primary)),
        ];
        if !detail.is_empty() {
            runs.push((format!("  {detail}"), self.theme.fg(Token::Muted)));
        }
        self.push(indent, runs);
    }

    fn table(&mut self, node: &Node, indent: usize) {
        let columns = node.children("columns");
        let cells: Vec<(String, Vec<String>)> = columns
            .iter()
            .map(|col| {
                let data = match col.prop("data") {
                    Some(Value::List(items)) => items.iter().map(|v| text_of(Some(v))).collect(),
                    _ => Vec::new(),
                };
                (text_of(col.prop("label")), data)
            })
            .collect();
        let rows = cells.iter().map(|(_, d)| d.len()).max().unwrap_or(0);
        let room = self.width.saturating_sub(indent);
        let mut widths: Vec<usize> = cells
            .iter()
            .map(|(h, d)| {
                d.iter()
                    .map(|c| c.width())
                    .chain([h.width()])
                    .max()
                    .unwrap_or(1)
            })
            .collect();
        // Shrink the widest columns until the row fits (V6: truncate, never wrap a table).
        while widths.iter().sum::<usize>() + 2 * widths.len() > room
            && widths.iter().any(|w| *w > 4)
        {
            if let Some(widest) = widths.iter_mut().max() {
                *widest -= 1;
            }
        }
        let fit = |text: &str, width: usize, more: &str| -> String {
            if text.width() <= width {
                return format!("{text}{}", " ".repeat(width - text.width()));
            }
            let mut out = String::new();
            for c in text.chars() {
                if out.width() + more.width() + 1 > width {
                    break;
                }
                out.push(c);
            }
            out.push_str(more);
            format!("{out}{}", " ".repeat(width.saturating_sub(out.width())))
        };
        let header: Vec<Span<'static>> = std::iter::once(Span::raw(" ".repeat(indent)))
            .chain(cells.iter().zip(&widths).map(|((h, _), w)| {
                Span::styled(
                    format!("{}  ", fit(h, *w, self.glyphs.more)),
                    self.theme.fg_bold(Token::Primary),
                )
            }))
            .collect();
        self.lines.push(Line::from(header));
        for row in 0..rows {
            let line: Vec<Span<'static>> = std::iter::once(Span::raw(" ".repeat(indent)))
                .chain(cells.iter().zip(&widths).map(|((_, d), w)| {
                    Span::styled(
                        format!(
                            "{}  ",
                            fit(d.get(row).map_or("", String::as_str), *w, self.glyphs.more)
                        ),
                        self.theme.fg(Token::Primary),
                    )
                }))
                .collect();
            self.lines.push(Line::from(line));
        }
    }

    fn input(&mut self, node: &Node, indent: usize, label: Option<&str>, hint: &str) {
        let placeholder = text_of(node.prop("placeholder"));
        let field = format!(
            "[ {:<24} ]",
            if placeholder.is_empty() {
                ""
            } else {
                placeholder.as_str()
            }
        );
        let mut runs = Vec::new();
        if let Some(label) = label {
            runs.push((format!("{label}  "), self.theme.fg_bold(Token::Primary)));
        }
        runs.push((field, self.theme.fg(Token::Muted)));
        if !hint.is_empty() {
            runs.push((format!("  {hint}"), self.theme.fg(Token::Muted)));
        }
        self.push(indent, runs);
    }

    fn control(&mut self, control: &Node, indent: usize) {
        let label = text_of(control.prop("label"));
        let hint = text_of(control.prop("hint"));
        let Some(Value::Node(input)) = control.prop("input") else {
            self.push(indent, vec![(label, self.theme.fg_bold(Token::Primary))]);
            return;
        };
        if matches!(input.name.as_str(), "Input" | "TextArea") {
            self.input(input, indent, Some(&label), &hint);
            return;
        }
        let mut runs = vec![(label, self.theme.fg_bold(Token::Primary))];
        if !hint.is_empty() {
            runs.push((format!("  {hint}"), self.theme.fg(Token::Muted)));
        }
        self.push(indent, runs);
        self.node(input, indent + 2);
    }

    fn items(&mut self, node: &Node, indent: usize) {
        let selected = Self::default_of(node);
        let kind = if matches!(node.name.as_str(), "RadioGroup" | "OptionCards" | "Select")
            || node.text("type") == Some("single")
        {
            'r'
        } else {
            'c'
        };
        for item in node.children("items") {
            let (label, detail, value) = match item.name.as_str() {
                "OptionCard" => (
                    text_of(item.prop("title")),
                    text_of(item.prop("subtitle")),
                    text_of(item.prop("value")),
                ),
                "ChipItem" | "SelectItem" => (
                    text_of(item.prop("label")),
                    String::new(),
                    text_of(item.prop("value")),
                ),
                _ => (
                    text_of(item.prop("label")),
                    text_of(item.prop("description")),
                    text_of(item.prop("value")),
                ),
            };
            let checked = (!value.is_empty() && value == selected)
                || matches!(item.prop("defaultChecked"), Some(Value::Bool(true)));
            self.choice_line(indent, checked, kind, &label, &detail);
        }
    }

    fn buttons(&mut self, node: &Node, indent: usize) {
        let mut runs = Vec::new();
        for button in node.children("buttons") {
            let primary = !matches!(button.text("variant"), Some("secondary" | "tertiary"));
            let style = if primary {
                self.theme.fg_bold(Token::Accent)
            } else {
                self.theme.fg(Token::Primary)
            };
            runs.push((format!("[ {} ]", text_of(button.prop("label"))), style));
            runs.push(("  ".to_owned(), Style::default()));
        }
        self.push(indent, runs);
    }

    fn node(&mut self, node: &Node, indent: usize) {
        let theme = self.theme;
        match node.name.as_str() {
            "Stack" | "Card" => {
                for child in node.children("children") {
                    self.node(child, indent);
                }
            }
            "CardHeader" => {
                self.push(
                    indent,
                    vec![(text_of(node.prop("title")), theme.fg_bold(Token::Primary))],
                );
                let subtitle = text_of(node.prop("subtitle"));
                if !subtitle.is_empty() {
                    self.push(indent, vec![(subtitle, theme.fg(Token::Muted))]);
                }
            }
            "TextContent" | "Label" => self.push(
                indent,
                vec![(text_of(node.prop("text")), theme.fg(Token::Primary))],
            ),
            "MarkDownRenderer" => {
                let body = markdown::render(
                    &text_of(node.prop("textMarkdown")),
                    theme,
                    self.width.saturating_sub(indent),
                );
                for line in body {
                    let mut spans = vec![Span::raw(" ".repeat(indent))];
                    spans.extend(line.spans);
                    self.lines.push(Line::from(spans));
                }
            }
            "Callout" | "TextCallout" => {
                let token = status_token(node.text("variant").unwrap_or("neutral"));
                self.push(
                    indent,
                    vec![
                        ("! ".to_owned(), theme.fg_bold(token)),
                        (text_of(node.prop("title")), theme.fg_bold(token)),
                        (
                            format!("  {}", text_of(node.prop("description"))),
                            theme.fg(Token::Primary),
                        ),
                    ],
                );
            }
            "Tag" => {
                let token = status_token(node.text("variant").unwrap_or("neutral"));
                self.push(
                    indent,
                    vec![(format!("[{}]", text_of(node.prop("text"))), theme.fg(token))],
                );
            }
            "TagBlock" => {
                let tags = match node.prop("tags") {
                    Some(Value::List(items)) => items
                        .iter()
                        .map(|t| format!("#{}", text_of(Some(t))))
                        .collect::<Vec<_>>()
                        .join(" "),
                    _ => String::new(),
                };
                self.push(indent, vec![(tags, theme.fg(Token::Info))]);
            }
            "Separator" => self.push(
                indent,
                vec![(
                    "─".repeat(self.width.saturating_sub(indent).min(40)),
                    theme.fg(Token::Muted),
                )],
            ),
            "ListBlock" | "Steps" => {
                let numbered = node.name == "Steps" || node.text("variant") == Some("number");
                for (index, item) in node.children("items").into_iter().enumerate() {
                    let title = text_of(item.prop("title"));
                    let detail = text_of(item.prop("subtitle").or_else(|| item.prop("details")));
                    let marker = if numbered {
                        format!("{}. ", index + 1)
                    } else {
                        format!("{} ", self.glyphs.closed)
                    };
                    let mut runs = vec![
                        (marker, theme.fg(Token::Muted)),
                        (title, theme.fg(Token::Primary)),
                    ];
                    if !detail.is_empty() {
                        runs.push((
                            format!(" {} {detail}", self.glyphs.sep),
                            theme.fg(Token::Muted),
                        ));
                    }
                    let action = text_of(item.prop("actionLabel"));
                    if !action.is_empty() {
                        runs.push((format!("  [ {action} ]"), theme.fg(Token::Accent)));
                    }
                    self.push(indent, runs);
                }
            }
            "Table" => self.table(node, indent),
            "Form" => {
                for field in node.children("fields") {
                    self.control(field, indent);
                }
                if let Some(Value::Node(buttons)) = node.prop("buttons") {
                    self.buttons(buttons, indent);
                }
            }
            "FormControl" => self.control(node, indent),
            "Input" | "TextArea" => self.input(node, indent, None, ""),
            "RadioGroup" | "CheckBoxGroup" | "SwitchGroup" | "Chips" | "OptionCards" | "Select" => {
                self.items(node, indent)
            }
            "Buttons" => self.buttons(node, indent),
            "Button" => self.push(
                indent,
                vec![(
                    format!("[ {} ]", text_of(node.prop("label"))),
                    theme.fg_bold(Token::Accent),
                )],
            ),
            other => self.push(
                indent,
                vec![(
                    format!("[{}]", sanitize::clean_line(other)),
                    theme.fg(Token::Muted),
                )],
            ),
        }
    }
}

fn state_word(state: PartState) -> &'static str {
    match state {
        PartState::Shown => "",
        PartState::Answered => "answered",
        PartState::Expired => "expired",
        PartState::Withdrawn => "withdrawn",
    }
}

/// Lines for one part entry's `data`.
pub fn lines(data: &Json, screen: &Screen<'_>, width: usize) -> Vec<Line<'static>> {
    let theme = screen.theme;
    let label = |text: &str| vec![(format!("{text:<LABEL$} "), theme.fg_bold(Token::Info))];
    let rest = vec![(" ".repeat(LABEL + 1), Style::default())];
    let Ok(part) = UiPart::parse(data) else {
        return markdown::wrap(
            &[(
                "[a part this client cannot read]".to_owned(),
                theme.fg(Token::Muted),
            )],
            width,
            &label("part"),
            &rest,
        );
    };
    let decision = oqto_openui::decide(&part);
    let inner = width.saturating_sub(LABEL + 1);
    let mut ctx = Ctx {
        theme,
        glyphs: screen.glyphs,
        width: inner,
        lines: Vec::new(),
    };
    let mut note = None;
    match &decision {
        Decision::Native(root) => ctx.node(root, 0),
        Decision::Fallback(reason) => {
            let fallback = sanitize::clean(&part.part.fallback);
            for line in fallback.lines() {
                ctx.push(0, vec![(line.to_owned(), theme.fg(Token::Primary))]);
            }
            note = Some(format!("shown as text: {reason}"));
        }
    }
    let state = state_word(part.state);
    let mut footer: Vec<String> = Vec::new();
    if !state.is_empty() {
        footer.push(state.to_owned());
    }
    if let Some(note) = note {
        footer.push(note);
    }
    if part.waits() {
        footer.push(
            "answering needs a paired device (oqto.interactions, not in this client yet)"
                .to_owned(),
        );
    }
    if !footer.is_empty() {
        let joined = footer.join(&format!(" {} ", screen.glyphs.sep));
        ctx.push(
            0,
            vec![(
                joined,
                theme.fg(Token::Muted).add_modifier(Modifier::ITALIC),
            )],
        );
    }
    let dim = part.state != PartState::Shown;
    let mut out = Vec::with_capacity(ctx.lines.len());
    for (index, line) in ctx.lines.into_iter().enumerate() {
        let prefix = if index == 0 {
            label("part")
        } else {
            rest.clone()
        };
        let mut spans: Vec<Span<'static>> = prefix
            .into_iter()
            .map(|(t, s)| Span::styled(t, s))
            .collect();
        spans.extend(line.spans.into_iter().map(|s| {
            if dim {
                Span::styled(s.content, s.style.add_modifier(Modifier::DIM))
            } else {
                s
            }
        }));
        out.push(Line::from(spans));
    }
    out
}

/// The part id of an `oqto.ui` entry's data, for newest-state reconciliation.
pub fn part_id(data: &Json) -> Option<&str> {
    data.pointer("/part/partId").and_then(Json::as_str)
}

/// Whether a part waits for an answer this client cannot give (attention line).
pub fn waiting_title(data: &Json) -> Option<String> {
    let part = UiPart::parse(data).ok()?;
    if !part.waits() {
        return None;
    }
    let title = match oqto_openui::decide(&part) {
        Decision::Native(root) => find_title(&root),
        Decision::Fallback(_) => None,
    };
    Some(title.unwrap_or_else(|| {
        part.part
            .fallback
            .lines()
            .next()
            .unwrap_or("a part")
            .to_owned()
    }))
}

fn find_title(node: &Node) -> Option<String> {
    if node.name == "CardHeader" {
        return node.text("title").map(sanitize::clean_line);
    }
    node.props.iter().find_map(|(_, value)| match value {
        Value::Node(child) => find_title(child),
        Value::List(items) => items.iter().find_map(|v| {
            if let Value::Node(n) = v {
                find_title(n)
            } else {
                None
            }
        }),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Editor;
    use crate::ui::UNICODE;
    use serde_json::json;
    use std::time::Duration;

    const ROLLOUT: &str = include_str!("../../oqto-openui/fixtures/core-rollout/input.oui");

    fn data(body: &str, version: &str, state: &str) -> Json {
        json!({"part": {"schema": "oqto.openui.part.v1", "partId": "part_rollout_1", "profile": {"id": "oqto", "version": version},
            "body": body, "fallback": "== Release 0.5.12 ==\nfallback text", "answer": {"schema": {"type": "object"}, "timeoutSeconds": 60}},
            "state": state})
    }

    fn text(data: &Json) -> Vec<String> {
        let editor = Editor::default();
        let screen = Screen {
            theme: Theme::ansi_default(),
            glyphs: UNICODE,
            host: "h",
            session: "s",
            model: None,
            thinking_level: None,
            blocks: &[],
            working: Some(Duration::ZERO),
            offline: None,
            cached: false,
            queued: 0,
            cost: None,
            tokens: None,
            transcript_focus: false,
            top: None,
            show_thinking: false,
            expand_tools: false,
            tool_lines: 3,
            editor: &editor,
            notice: None,
            attention: None,
            images: crate::caps::ImageMode::Text,
            overlay_open: false,
        };
        lines(data, &screen, 90)
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect()
    }

    #[test]
    fn core_part_renders_natively() {
        let out = text(&data(ROLLOUT, "0.2.2", "shown"));
        assert_eq!(out[0], "part     Release 0.5.12");
        assert!(
            out.contains(
                &"         ! 1 host degraded  gamma runs 0.5.9 and failed its last health check."
                    .to_owned()
            )
        );
        assert!(out.contains(&"         Host   Runner  State".to_owned()));
        assert!(out.contains(&"         gamma  0.5.9   degraded".to_owned()));
        assert!(out.contains(&"           (•) Canary  One host first, then the rest".to_owned()));
        assert!(out.contains(&"           ( ) All at once  Every host now".to_owned()));
        assert!(out.iter().any(|l| l.contains("[ Roll out ]  [ Not now ]")));
        assert!(
            out.last()
                .is_some_and(|l| l.contains("answering needs a paired device"))
        );
        assert_eq!(
            waiting_title(&data(ROLLOUT, "0.2.2", "shown")).as_deref(),
            Some("Release 0.5.12")
        );
    }

    #[test]
    fn unknown_profile_bounds_and_components_show_the_fallback() {
        for (body, version, why) in [
            (ROLLOUT.to_owned(), "0.3.0", "profile 0.3.0"),
            (
                "root = Stack([AreaChart([\"a\"], [])])".to_owned(),
                "0.2.2",
                "AreaChart is not in the core set",
            ),
            (
                format!("root = TextContent(\"{}\")", "é".repeat(20_000)),
                "0.2.2",
                "body larger than",
            ),
            (
                format!("root = {}{}", "Stack([".repeat(40), "])".repeat(40)),
                "0.2.2",
                "nesting deeper",
            ),
            ("root = Stack([".to_owned(), "0.2.2", "syntax error"),
        ] {
            let out = text(&data(&body, version, "shown"));
            assert_eq!(out[0], "part     == Release 0.5.12 ==", "{why}");
            assert!(
                out.iter()
                    .any(|l| l.contains("shown as text") && l.contains(why)),
                "{why}: {out:?}"
            );
        }
    }

    #[test]
    fn later_states_say_so_and_stop_waiting() {
        let withdrawn = data(ROLLOUT, "0.2.2", "withdrawn");
        assert!(
            text(&withdrawn)
                .last()
                .is_some_and(|l| l.contains("withdrawn") && !l.contains("paired device"))
        );
        assert_eq!(waiting_title(&withdrawn), None);
    }

    #[test]
    fn part_text_cannot_inject_escapes() {
        let body = "root = Stack([TextContent(\"\\u001b]52;c;AAAA\\u0007 hi\")])";
        let out = text(&data(body, "0.2.2", "shown"));
        assert!(
            out.iter()
                .all(|l| !l.contains('\u{1b}') && !l.contains('\u{7}'))
        );
        assert!(out.iter().any(|l| l.contains("␛]52;c;AAAA␇ hi")));
    }
}
