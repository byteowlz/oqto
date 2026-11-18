//! The interactive client: state, keys, the event loop and the terminal around it.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use base64::Engine;
use byteowlz_tui_kit::action::{Action, Key, KeyRouter, Route};
use byteowlz_tui_kit::palette::{CommandPalette, PaletteOutcome};
use byteowlz_tui_kit::terminal::TerminalGuard;
use byteowlz_tui_kit::theme::{Theme, Token};
use byteowlz_tui_kit::whichkey;
use crossterm::event::{
    DisableBracketedPaste, EnableBracketedPaste, Event, EventStream, KeyEvent, KeyEventKind,
    KeyboardEnhancementFlags, MouseEventKind, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use futures::StreamExt;
use oqto_session_client::{
    FileCache, KeySource, Link, Notice, Options as ClientOptions, Secret, SessionConfig,
    SessionHandle,
};
use oqto_session_view::frames::InputKind;
use ratatui::text::{Line, Span};
use serde_json::json;

use crate::caps::{self, Caps, ImageMode};
use crate::config::{CacheKeyStore, Config, HostConfig};
use crate::editor::Editor;
use crate::keys::{self, Scope};
use crate::kitty;
use crate::model::{self, Block};
use crate::status::{Reporter, State};
use crate::ui::{self, Glyphs, Screen};

/// What the binary hands to the interactive client.
pub struct Launch {
    /// Effective configuration.
    pub config: Config,
    /// The host to attach to.
    pub host: HostConfig,
    /// Its secret.
    pub secret: Secret,
    /// Write start-up timings here (tests and measurements).
    pub timing_file: Option<PathBuf>,
    /// Command that reopens this view (herdr resume).
    pub resume: Vec<String>,
    /// ASCII glyphs.
    pub ascii: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Composer,
    Transcript,
}

enum Overlay {
    Palette(CommandPalette),
    Info {
        title: &'static str,
        lines: Vec<Line<'static>>,
    },
}

/// The terminal: the kit's guard plus what this client turns on, all undone on drop.
struct Term {
    guard: Option<TerminalGuard>,
    kitty_keyboard: bool,
    cleanup: String,
}

impl Term {
    fn enter(kitty_keyboard: bool) -> Result<Self> {
        let guard = TerminalGuard::enter().context("enter the terminal")?;
        let mut out = std::io::stdout();
        crossterm::execute!(out, EnableBracketedPaste).context("bracketed paste")?;
        if kitty_keyboard {
            crossterm::execute!(
                out,
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
            )
            .context("keyboard flags")?;
        }
        Ok(Self {
            guard: Some(guard),
            kitty_keyboard,
            cleanup: String::new(),
        })
    }

    fn guard(&mut self) -> Result<&mut TerminalGuard> {
        self.guard.as_mut().context("terminal suspended")
    }

    fn write(&mut self, bytes: &str) -> Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        let mut out = std::io::stdout();
        out.write_all(bytes.as_bytes())
            .context("write to the terminal")?;
        out.flush().context("flush the terminal")
    }

    fn release(&mut self) {
        let mut out = std::io::stdout();
        let _ = out.write_all(self.cleanup.as_bytes());
        if self.kitty_keyboard {
            let _ = crossterm::execute!(out, PopKeyboardEnhancementFlags);
        }
        let _ = crossterm::execute!(out, DisableBracketedPaste);
        let _ = out.flush();
        self.guard = None;
    }

    /// Leave the terminal to run another program (the external editor), then come back.
    fn suspend<T>(&mut self, run: impl FnOnce() -> T) -> Result<T> {
        self.release();
        let result = run();
        let mut again = Self::enter(self.kitty_keyboard)?;
        self.guard = again.guard.take();
        Ok(result)
    }
}

impl Drop for Term {
    fn drop(&mut self) {
        if self.guard.is_some() {
            self.release();
        }
    }
}

struct Images {
    ids: HashMap<String, u32>,
    next: u32,
    sent: HashMap<String, u32>,
    last: Vec<ui::Placement>,
}

impl Images {
    fn id(&mut self, key: &str) -> u32 {
        if let Some(id) = self.ids.get(key) {
            return *id;
        }
        let id = self.next;
        self.next = if self.next >= 255 { 1 } else { self.next + 1 };
        self.ids.retain(|_, v| *v != id);
        self.sent.retain(|_, v| *v != id);
        self.ids.insert(key.to_owned(), id);
        id
    }
}

struct App {
    handle: SessionHandle,
    config: Config,
    host_label: String,
    session: String,
    theme: Theme,
    glyphs: Glyphs,
    caps: Caps,
    composer_actions: &'static [Action],
    router: KeyRouter<'static>,
    transcript_router: KeyRouter<'static>,
    composer_starts: Vec<Key>,
    transcript_starts: Vec<Key>,
    focus: Focus,
    overlays: Vec<Overlay>,
    editor: Editor,
    top: Option<usize>,
    last_start: usize,
    last_lines: usize,
    last_rows: usize,
    show_thinking: bool,
    expand_tools: bool,
    hint: Option<(Vec<Key>, Vec<(Key, &'static str)>)>,
    notice: Option<(String, bool, Instant)>,
    clear_armed: Option<Instant>,
    stop_armed: Option<Instant>,
    quit: bool,
    dirty: bool,
    run_since: Option<Instant>,
    was_working: bool,
    reporter: Reporter,
    images: Images,
    pending_output: String,
    blocks: Vec<Block>,
    attention: Option<String>,
}

fn starts(actions: &[Action]) -> Vec<Key> {
    let mut out: Vec<Key> = Vec::new();
    for action in actions {
        if let Some(first) = action.key_path().first()
            && !out.contains(first)
        {
            out.push(*first);
        }
    }
    out
}

const SLASH: &[(&str, &str)] = &[
    ("quit", "oqt.quit"),
    ("stop", "oqt.session.stop"),
    ("copy", "oqt.copy.last"),
    ("hotkeys", "oqt.help"),
    ("help", "oqt.help"),
    ("model", "oqt.model.show"),
    ("earlier", "oqt.transcript.earlier"),
];

impl App {
    fn notice(&mut self, text: impl Into<String>, danger: bool) {
        self.notice = Some((text.into(), danger, Instant::now()));
        self.dirty = true;
    }

    fn working(&self) -> bool {
        self.handle.read(|s| s.client.view().working())
    }

    fn submit(&mut self, kind: InputKind) {
        if kind != InputKind::FollowUp && self.editor.backslash_newline() {
            return;
        }
        let text = self.editor.take();
        if text.trim().is_empty() {
            return;
        }
        if let Some(command) = text.strip_prefix('/') {
            let word = command.split_whitespace().next().unwrap_or_default();
            if let Some((_, id)) = SLASH.iter().find(|(name, _)| *name == word) {
                self.run(id);
                return;
            }
            if word == "note" {
                let note = command.trim_start_matches("note").trim().to_owned();
                self.handle
                    .update(|c| c.write("app.note", Some(json!({"text": note}))));
                return;
            }
        }
        let kind = match kind {
            InputKind::Prompt if self.working() => InputKind::Steer,
            other => other,
        };
        self.handle.update(|c| c.submit(kind, text));
        self.top = None;
    }

    fn abort(&mut self) {
        self.handle.update(|c| c.abort());
        self.stop_armed = None;
        self.notice("stopping the run", false);
    }

    fn interrupt(&mut self) {
        self.hint = None;
        if self.focus == Focus::Transcript {
            self.focus = Focus::Composer;
            return;
        }
        if !self.working() {
            return;
        }
        let queued = self.handle.read(|s| s.client.view().inbox().len());
        let armed = self
            .stop_armed
            .is_some_and(|t| t.elapsed() < Duration::from_secs(3));
        if queued > 0 && !armed {
            self.stop_armed = Some(Instant::now());
            self.notice(
                format!("escape again stops the run and withdraws {queued} queued"),
                false,
            );
            return;
        }
        self.abort();
    }

    fn info(&self, title: &'static str, rows: Vec<(String, String)>) -> Overlay {
        let theme = self.theme;
        let lines = rows
            .into_iter()
            .map(|(k, v)| {
                Line::from(vec![
                    Span::styled(format!("{k:<22}"), theme.fg_bold(Token::Primary)),
                    Span::styled(v, theme.fg(Token::Muted)),
                ])
            })
            .collect();
        Overlay::Info { title, lines }
    }

    fn help(&self) -> Overlay {
        let mut rows: Vec<(String, String)> = Vec::new();
        for spec in keys::SPECS {
            let paths: Vec<String> = self
                .config
                .keys
                .bindings
                .get(spec.id)
                .cloned()
                .unwrap_or_default();
            let scope = if spec.scope == Scope::Transcript {
                " (transcript)"
            } else {
                ""
            };
            rows.push((paths.join(", "), format!("{}{scope}", spec.label)));
        }
        rows.push(("/".to_owned(), "commands, on an empty draft".to_owned()));
        rows.push((
            "\\ enter".to_owned(),
            "new line where shift+enter is not reported".to_owned(),
        ));
        self.info("keys", rows)
    }

    fn model_info(&self) -> Overlay {
        let rows = self.handle.read(|s| {
            let view = s.client.view();
            vec![
                (
                    "model".to_owned(),
                    view.model()
                        .unwrap_or_else(|| "not shown by the host".to_owned()),
                ),
                (
                    "thinking".to_owned(),
                    view.thinking_level()
                        .unwrap_or_else(|| "not shown by the host".to_owned()),
                ),
                (
                    "store".to_owned(),
                    s.client.store().unwrap_or("unknown").to_owned(),
                ),
                ("host".to_owned(), self.host_label.clone()),
                (
                    "changing them".to_owned(),
                    "needs session-view v1 configure (slice 2); read-only here".to_owned(),
                ),
            ]
        });
        self.info("model and thinking", rows)
    }

    fn copy_last(&mut self) {
        let Some(text) = model::last_answer(&self.blocks).map(str::to_owned) else {
            self.notice("no answer to copy yet", false);
            return;
        };
        let data = base64::engine::general_purpose::STANDARD.encode(text);
        let osc = format!("\u{1b}]52;c;{data}\u{7}");
        self.pending_output.push_str(&if self.caps.env.tmux {
            caps::passthrough(&osc)
        } else {
            osc
        });
        self.notice("copied the last answer (OSC 52)", false);
    }

    fn scroll_by(&mut self, delta: isize) {
        let max_start = self.last_lines.saturating_sub(self.last_rows);
        let current = self.top.unwrap_or(self.last_start) as isize;
        let next = (current + delta).clamp(0, max_start as isize) as usize;
        self.top = if next >= max_start { None } else { Some(next) };
        if next == 0 {
            self.handle.update(|c| c.load_earlier());
        }
    }

    fn run(&mut self, id: &str) {
        self.dirty = true;
        let half = (self.last_rows / 2).max(1) as isize;
        let page = self.last_rows.max(1) as isize;
        match id {
            "tui.input.submit" => self.submit(InputKind::Prompt),
            "app.message.followUp" => self.submit(InputKind::FollowUp),
            "tui.input.newLine" => self.editor.insert("\n"),
            "app.interrupt" => self.interrupt(),
            "app.clear" => {
                if !self.editor.is_empty() {
                    self.editor.set("");
                } else if self
                    .clear_armed
                    .is_some_and(|t| t.elapsed() < Duration::from_millis(1500))
                {
                    self.quit = true;
                } else {
                    self.clear_armed = Some(Instant::now());
                    self.notice("ctrl+c again detaches; the Session keeps running", false);
                }
            }
            "app.exit" => {
                if self.editor.is_empty() {
                    self.quit = true;
                } else {
                    self.editor
                        .key(KeyEvent::from(crossterm::event::KeyCode::Delete));
                }
            }
            "app.thinking.toggle" => self.show_thinking = !self.show_thinking,
            "app.tools.expand" => self.expand_tools = !self.expand_tools,
            "oqt.palette" => self.overlays.push(Overlay::Palette(CommandPalette::new(
                keys::palette_actions(self.composer_actions),
            ))),
            "oqt.focus.transcript" => {
                self.focus = Focus::Transcript;
                self.scroll_by(-page);
            }
            "oqt.focus.composer" => self.focus = Focus::Composer,
            "oqt.session.stop" => self.abort(),
            "oqt.transcript.earlier" => self.handle.update(|c| c.load_earlier()),
            "oqt.transcript.missing" => self.handle.update(|c| c.fill_gap()),
            "oqt.note.save" => {
                let text = self.editor.take();
                if text.trim().is_empty() {
                    self.notice("type the note first", false);
                } else {
                    self.handle
                        .update(|c| c.write("app.note", Some(json!({"text": text}))));
                }
            }
            "oqt.model.show" => {
                let overlay = self.model_info();
                self.overlays.push(overlay);
            }
            "oqt.copy.last" => self.copy_last(),
            "oqt.help" => {
                let overlay = self.help();
                self.overlays.push(overlay);
            }
            "oqt.quit" => self.quit = true,
            "tui.altScreen.lineDown" => self.scroll_by(1),
            "tui.altScreen.lineUp" => self.scroll_by(-1),
            "tui.altScreen.halfPageDown" => self.scroll_by(half),
            "tui.altScreen.halfPageUp" => self.scroll_by(-half),
            "tui.altScreen.pageDown" => self.scroll_by(page),
            "tui.altScreen.pageUp" => self.scroll_by(-page),
            "tui.altScreen.top" => self.scroll_by(-(self.last_lines as isize)),
            "tui.altScreen.bottom" => self.top = None,
            _ => {}
        }
    }

    fn key(&mut self, event: KeyEvent) {
        if event.kind == KeyEventKind::Release {
            return;
        }
        self.dirty = true;
        let key = keys::normalize(Key::from(event));
        if let Some(top) = self.overlays.last_mut() {
            match top {
                Overlay::Palette(palette) => match palette.handle(key) {
                    PaletteOutcome::Open => {}
                    PaletteOutcome::Closed => {
                        self.overlays.pop();
                    }
                    PaletteOutcome::Run(id) => {
                        self.overlays.pop();
                        self.run(id.as_str());
                    }
                },
                Overlay::Info { .. } => {
                    self.overlays.pop();
                }
            }
            return;
        }
        let (router, starts) = match self.focus {
            Focus::Composer => (&mut self.router, &self.composer_starts),
            Focus::Transcript => (&mut self.transcript_router, &self.transcript_starts),
        };
        let had_prefix = router.has_prefix();
        if had_prefix || starts.contains(&key) {
            match router.feed(key) {
                Route::Action(id) => {
                    self.hint = None;
                    self.run(id.as_str());
                    return;
                }
                Route::Prefix(options) => {
                    self.hint = Some((router.prefix_ref().to_vec(), options));
                    return;
                }
                Route::Miss => {
                    self.hint = None;
                    if had_prefix {
                        return;
                    }
                }
            }
        }
        if self.focus == Focus::Composer {
            if key == Key::char('/') && self.editor.is_empty() {
                self.run("oqt.palette");
                return;
            }
            self.editor.key(event);
        }
    }

    fn event(&mut self, event: Event) {
        match event {
            Event::Key(key) => self.key(key),
            Event::Paste(text) => {
                if self.focus == Focus::Composer && self.overlays.is_empty() {
                    self.editor.insert(&text);
                }
                self.dirty = true;
            }
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.scroll_by(-3);
                    self.dirty = true;
                }
                MouseEventKind::ScrollDown => {
                    self.scroll_by(3);
                    self.dirty = true;
                }
                _ => {}
            },
            _ => self.dirty = true,
        }
    }

    fn sync_state(&mut self) {
        let notices = self.handle.take_notices();
        for notice in notices {
            match notice {
                Notice::Refused { code } => {
                    self.notice(format!("the host refused a request: {code}"), true)
                }
                Notice::Purged => self.notice("access ended; the local cache was deleted", true),
                Notice::Resynced => {}
            }
        }
        let (blocks, working, failure, has_gap, loading) = self.handle.read(|s| {
            let client = &s.client;
            let view = client.view();
            (
                model::project(client),
                view.working(),
                view.run_failure().map(|(reason, _)| reason),
                !client.transcript().gaps().is_empty(),
                client.loading(),
            )
        });
        self.blocks = blocks;
        self.attention = self
            .blocks
            .iter()
            .rev()
            .find_map(|b| match b {
                Block::Part { data } => crate::parts::waiting_title(data),
                _ => None,
            })
            .map(|title| format!("the agent shows \"{title}\": answering needs a paired device, not this client yet"));
        if has_gap && loading.1 == 0 {
            self.handle.update(|c| c.fill_gap());
        }
        match (working, self.run_since) {
            (true, None) => self.run_since = Some(Instant::now()),
            (false, Some(_)) => self.run_since = None,
            _ => {}
        }
        let state = if working {
            State::Working
        } else if let Some(reason) = failure.filter(|_| self.was_working) {
            State::Error { message: reason }
        } else if self.was_working {
            State::Done
        } else {
            State::Idle
        };
        if working {
            self.was_working = true;
        }
        if let Some(bytes) = self.reporter.report(state) {
            self.pending_output.push_str(&bytes);
        }
    }

    fn screen(&self) -> Screen<'_> {
        let (model, thinking, offline, cached, queued, cost, tokens) = self.handle.read(|s| {
            let view = s.client.view();
            let usage = view
                .doc("pi.usage")
                .and_then(|u| u.get("models"))
                .and_then(|m| m.as_object());
            let cost: Option<f64> = usage.map(|m| {
                m.values()
                    .filter_map(|v| v.pointer("/cost/total").and_then(serde_json::Value::as_f64))
                    .sum()
            });
            let tokens: Option<u64> = usage.map(|m| {
                m.values()
                    .filter_map(|v| v.get("totalTokens").and_then(serde_json::Value::as_u64))
                    .sum()
            });
            let offline = match &s.link {
                Link::Online => None,
                Link::Connecting => Some("connecting".to_owned()),
                Link::Offline(reason) => Some(reason.clone()),
            };
            (
                view.model(),
                view.thinking_level(),
                offline,
                s.client.cached(),
                view.inbox().len(),
                cost,
                tokens,
            )
        });
        Screen {
            theme: self.theme,
            glyphs: self.glyphs,
            host: &self.host_label,
            session: &self.session,
            model,
            thinking_level: thinking,
            blocks: &self.blocks,
            working: self.run_since.map(|t| t.elapsed()),
            offline,
            cached,
            queued,
            cost,
            tokens,
            transcript_focus: self.focus == Focus::Transcript,
            top: self.top,
            show_thinking: self.show_thinking,
            expand_tools: self.expand_tools,
            tool_lines: usize::from(self.config.display.tool_lines),
            editor: &self.editor,
            notice: self.notice.as_ref().map(|(t, d, _)| (t.as_str(), *d)),
            attention: self.attention.clone(),
            images: self.caps.images,
            overlay_open: !self.overlays.is_empty(),
        }
    }

    fn image_commands(&mut self, placements: Vec<ui::Placement>) -> String {
        let tmux = self.caps.env.tmux;
        let mut out = String::new();
        match self.caps.images {
            ImageMode::Text => return out,
            ImageMode::Placeholders => {
                for block in &self.blocks {
                    if let Block::Image(image) = block {
                        if self.images.sent.contains_key(&image.key) {
                            continue;
                        }
                        let id = self.images.id(&image.key);
                        let Some((w, h)) = kitty::png_size(&image.bytes) else {
                            continue;
                        };
                        let (cols, rows) = kitty::cells(w, h, 40);
                        out.push_str(&kitty::transmit(id, &image.bytes, tmux));
                        out.push_str(&kitty::virtual_place(id, cols, rows, tmux));
                        self.images.sent.insert(image.key.clone(), id);
                    }
                }
                return out;
            }
            ImageMode::Kitty => {}
        }
        if placements == self.images.last {
            return out;
        }
        out.push_str(&kitty::delete_placements(tmux));
        for placement in &placements {
            let id = self.images.id(&placement.key);
            if !self.images.sent.contains_key(&placement.key) {
                let bytes = self.blocks.iter().find_map(|b| match b {
                    Block::Image(image) if image.key == placement.key => Some(image.bytes.clone()),
                    _ => None,
                });
                if let Some(bytes) = bytes {
                    out.push_str(&kitty::transmit(id, &bytes, tmux));
                    self.images.sent.insert(placement.key.clone(), id);
                }
            }
            out.push_str(&kitty::place(
                id,
                placement.x,
                placement.y,
                placement.cols,
                placement.rows,
            ));
        }
        self.images.last = placements;
        out
    }

    fn draw(&mut self, term: &mut Term) -> Result<()> {
        if let Some((_, _, at)) = &self.notice
            && at.elapsed() > Duration::from_secs(4)
        {
            self.notice = None;
        }
        let sync = self.caps.synchronized_output;
        if sync {
            term.write("\u{1b}[?2026h")?;
        }
        let ids: HashMap<String, u32> = if self.caps.images == ImageMode::Placeholders {
            self.blocks
                .iter()
                .filter_map(|b| {
                    if let Block::Image(i) = b {
                        Some(i.key.clone())
                    } else {
                        None
                    }
                })
                .map(|k| {
                    let id = self.images.id(&k);
                    (k, id)
                })
                .collect()
        } else {
            HashMap::new()
        };
        let lookup = |key: &str| ids.get(key).copied();
        let mut output = ui::Output::default();
        let mut overlays = std::mem::take(&mut self.overlays);
        let drawn = {
            let mut screen = self.screen();
            screen.overlay_open = !overlays.is_empty();
            let theme = self.theme;
            let hint = self.hint.clone();
            term.guard().and_then(|guard| {
                guard
                    .draw(|frame| {
                        output = ui::draw(frame, &screen, &lookup);
                        if let Some((prefix, options)) = &hint {
                            whichkey::draw_hint(
                                frame,
                                ui::hint_area(frame.area()),
                                theme,
                                prefix,
                                options,
                            );
                        }
                        match overlays.last_mut() {
                            Some(Overlay::Palette(palette)) => {
                                ui::palette_frame(frame, theme);
                                palette.draw(frame, theme);
                            }
                            Some(Overlay::Info { title, lines }) => {
                                ui::overlay(frame, theme, title, lines.clone())
                            }
                            None => {}
                        }
                    })
                    .map(|_| ())
                    .context("draw")
            })
        };
        self.overlays = overlays;
        drawn?;
        self.last_lines = output.transcript_lines;
        self.last_rows = output.transcript_rows;
        self.last_start = output.start;
        if output.earlier_visible {
            self.handle.update(|c| c.load_earlier());
        }
        let mut bytes = std::mem::take(&mut self.pending_output);
        bytes.push_str(&self.image_commands(output.placements));
        if sync {
            bytes.push_str("\u{1b}[?2026l");
        }
        term.write(&bytes)?;
        self.dirty = false;
        Ok(())
    }

    fn external_editor(&mut self, term: &mut Term) -> Result<()> {
        let editor = std::env::var("VISUAL")
            .or_else(|_| std::env::var("EDITOR"))
            .unwrap_or_else(|_| "vi".to_owned());
        let dir = std::env::temp_dir().join(format!("oqt-tui-{}", std::process::id()));
        std::fs::create_dir_all(&dir).context("draft dir")?;
        let path = dir.join("draft.md");
        std::fs::write(&path, self.editor.text()).context("write draft")?;
        let status = term.suspend(|| {
            std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("{editor} \"$1\""))
                .arg("oqt-tui")
                .arg(&path)
                .status()
        })?;
        if status.is_ok_and(|s| s.success()) {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            self.editor.set(text.trim_end_matches('\n'));
        }
        let _ = std::fs::remove_dir_all(&dir);
        term.guard()?.clear().context("redraw")?;
        self.dirty = true;
        Ok(())
    }
}

fn timing(path: &Option<PathBuf>, line: String) {
    if let Some(path) = path
        && let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
    {
        let _ = writeln!(file, "{line}");
    }
}

/// Run the interactive client until the user detaches.
pub async fn run(launch: Launch) -> Result<()> {
    let started = Instant::now();
    let config = launch.config;
    let cache = if config.cache.enabled {
        let dir = match &config.cache.dir {
            Some(dir) => dir.clone(),
            None => crate::config::cache_dir()?.join("sessions"),
        };
        let key_file = dir.join("cache.key");
        let source = match config.cache.key {
            CacheKeyStore::Auto => KeySource::Auto(key_file),
            CacheKeyStore::File => KeySource::File(key_file),
        };
        Some(Arc::new(FileCache::open(dir, &source)?))
    } else {
        None
    };
    let (handle, task) = SessionHandle::start(SessionConfig {
        url: launch.host.url.clone(),
        secret: launch.secret,
        session: launch.host.session.clone(),
        options: ClientOptions {
            window_min: config.connection.window_min,
            window_max: None,
            page_size: config.connection.page_size,
        },
        cache,
        max_cached_entries: config.cache.max_entries,
        connect_timeout: Duration::from_millis(config.connection.connect_timeout_ms),
    });
    let env = caps::Environment::read(&|name| std::env::var(name).ok());
    crossterm::terminal::enable_raw_mode().context("raw mode")?;
    let replies = caps::probe(
        env.tmux,
        Duration::from_millis(config.terminal.probe_timeout_ms),
    )
    .unwrap_or_default();
    let caps = caps::decide(replies, env, &config.terminal);
    let leader = keys::parse_key(&config.keys.leader)?;
    keys::check_ids(&config.keys.bindings)?;
    let composer: &'static [Action] = Box::leak(
        keys::actions(Scope::Composer, leader, &config.keys.bindings)?.into_boxed_slice(),
    );
    let transcript: &'static [Action] = Box::leak(
        keys::actions(Scope::Transcript, leader, &config.keys.bindings)?.into_boxed_slice(),
    );
    let reporter = Reporter::new(
        caps.program_status,
        caps.herdr.then(|| caps.env.herdr_pane.clone()).flatten(),
        launch.host.session.clone(),
        launch.resume,
    );
    let mut term = Term::enter(caps.kitty_keyboard)?;
    let mut cleanup = String::new();
    if caps.images != ImageMode::Text {
        cleanup.push_str(&kitty::delete_placements(caps.env.tmux));
    }
    if caps.program_status {
        cleanup.push_str(&crate::status::osc7501(&State::Clear));
    }
    term.cleanup = cleanup;
    let theme = if caps.env.no_color {
        no_color_theme()
    } else {
        Theme::ansi_default()
    };
    let mut app = App {
        handle: handle.clone(),
        host_label: format!("{} ({})", launch.host.name, host_of(&launch.host.url)),
        session: launch.host.session.clone(),
        show_thinking: config.display.thinking == crate::config::Thinking::Shown,
        config,
        theme,
        glyphs: if launch.ascii { ui::ASCII } else { ui::UNICODE },
        caps,
        composer_actions: composer,
        router: KeyRouter::new(composer),
        transcript_router: KeyRouter::new(transcript),
        composer_starts: starts(composer),
        transcript_starts: starts(transcript),
        focus: Focus::Composer,
        overlays: Vec::new(),
        editor: Editor::default(),
        top: None,
        last_start: 0,
        last_lines: 0,
        last_rows: 0,
        expand_tools: false,
        hint: None,
        notice: None,
        clear_armed: None,
        stop_armed: None,
        quit: false,
        dirty: true,
        run_since: None,
        was_working: false,
        reporter,
        images: Images {
            ids: HashMap::new(),
            next: 1,
            sent: HashMap::new(),
            last: Vec::new(),
        },
        pending_output: String::new(),
        blocks: Vec::new(),
        attention: None,
    };
    app.sync_state();
    app.draw(&mut term)?;
    let (entries, from_cache) =
        handle.read(|s| (s.client.transcript().entries().len(), s.client.cached()));
    timing(
        &launch.timing_file,
        format!(
            "first_frame_ms={} entries={entries} from_cache={from_cache}",
            started.elapsed().as_millis()
        ),
    );
    // The connection runs during the probe, so the live base may already be here.
    let mut live_logged = handle.read(|s| s.client.view().ready());
    if live_logged {
        timing(
            &launch.timing_file,
            format!("live_ms={}", started.elapsed().as_millis()),
        );
    }
    let mut events = EventStream::new();
    let mut changes = handle.changed();
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    loop {
        tokio::select! {
            event = events.next() => match event {
                Some(Ok(event)) => {
                    let external = matches!(&event, Event::Key(k) if k.kind != KeyEventKind::Release && keys::normalize(Key::from(*k)) == Key::ctrl_char('g'));
                    if external && app.overlays.is_empty() {
                        app.external_editor(&mut term)?;
                    } else {
                        app.event(event);
                    }
                }
                Some(Err(_)) | None => break,
            },
            _ = changes.changed() => {
                app.sync_state();
                app.dirty = true;
                if !live_logged && handle.read(|s| s.client.view().ready()) {
                    live_logged = true;
                    timing(&launch.timing_file, format!("live_ms={}", started.elapsed().as_millis()));
                }
            },
            _ = tick.tick() => {
                if app.run_since.is_some() || app.notice.is_some() {
                    app.dirty = true;
                }
            },
        }
        if app.quit {
            break;
        }
        if app.dirty {
            app.draw(&mut term)?;
        }
    }
    task.abort();
    drop(term);
    Ok(())
}

fn host_of(url: &str) -> String {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or(url)
        .to_owned()
}

/// Under `NO_COLOR` weight and spacing carry the hierarchy (V8).
fn no_color_theme() -> Theme {
    use ratatui::style::Color;
    let mut theme = Theme::ansi_default();
    for token in [
        Token::Bar,
        Token::Muted,
        Token::Accent,
        Token::Success,
        Token::Danger,
        Token::Warning,
        Token::Info,
    ] {
        theme = theme.with_token(token, Color::Reset);
    }
    theme
}
