//! Terminal capabilities by reply, not by `TERM` (ADR-0087 decision 5, DESIGN.md 10).
//!
//! One write at start, ended by DA1 so every terminal answers at least once: XTVERSION, the
//! Kitty keyboard flags query, a Kitty graphics query (and one for Unicode placeholders), the
//! OSC 7501 support query and DECRQM for synchronized output. Environment variables refine
//! the result but never replace a reply.

use std::io::Write;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::config::{Images, TerminalConfig, Toggle};

/// What the terminal answered.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Replies {
    /// XTVERSION name.
    pub name: Option<String>,
    /// Kitty keyboard flags.
    pub kitty_keyboard: Option<u32>,
    /// Kitty graphics query answered OK.
    pub kitty_graphics: bool,
    /// Unicode placeholder query answered OK.
    pub placeholders: bool,
    /// OSC 7501 support reply.
    pub program_status: bool,
    /// DECRQM 2026 reported support.
    pub synchronized_output: bool,
    /// DA1 arrived (the probe completed).
    pub da1: bool,
}

/// Environment hints.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Environment {
    /// Inside tmux.
    pub tmux: bool,
    /// Inside herdr (`HERDR_ENV`), with its pane id.
    pub herdr_pane: Option<String>,
    /// Inside Rex.
    pub rex: bool,
    /// Over SSH.
    pub ssh: bool,
    /// `NO_COLOR` set.
    pub no_color: bool,
    /// `TERM`, for the report only.
    pub term: String,
}

impl Environment {
    /// Read the process environment.
    pub fn read(env: &dyn Fn(&str) -> Option<String>) -> Self {
        let set = |name: &str| env(name).is_some_and(|v| !v.is_empty());
        Self {
            tmux: set("TMUX"),
            herdr_pane: if set("HERDR_ENV") {
                env("HERDR_PANE_ID").filter(|v| !v.is_empty())
            } else {
                None
            },
            rex: set("REX_SESSION"),
            ssh: set("SSH_CONNECTION"),
            no_color: set("NO_COLOR"),
            term: env("TERM").unwrap_or_default(),
        }
    }
}

/// How images are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageMode {
    /// Kitty placements, in-band.
    Kitty,
    /// Kitty Unicode placeholders (tmux).
    Placeholders,
    /// Text stand-in.
    Text,
}

/// The decided feature set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caps {
    /// Raw replies.
    pub replies: Replies,
    /// Environment.
    pub env: Environment,
    /// Images.
    pub images: ImageMode,
    /// Emit OSC 7501.
    pub program_status: bool,
    /// Report to herdr through `report-agent`.
    pub herdr: bool,
    /// Wrap frames in synchronized output.
    pub synchronized_output: bool,
    /// Push Kitty keyboard flags.
    pub kitty_keyboard: bool,
}

/// The probe sequence. Inside tmux the graphics queries go through passthrough.
pub fn query(tmux: bool) -> String {
    let graphics = "\u{1b}_Ga=q,i=31,s=1,v=1,t=d,f=24;AAAA\u{1b}\\\u{1b}_Ga=q,i=32,s=1,v=1,t=d,f=24,U=1;AAAA\u{1b}\\";
    // Inside tmux the graphics queries reach the outer terminal through passthrough and are
    // answered after tmux's own DA1, so a second DA1 travels the same way as their sentinel.
    let graphics = if tmux {
        passthrough(&format!("{graphics}\u{1b}[c"))
    } else {
        graphics.to_owned()
    };
    format!("\u{1b}[>q\u{1b}[?u{graphics}\u{1b}]7501;?\u{1b}\\\u{1b}[?2026$p\u{1b}[c")
}

/// How many DA1 replies end the probe: one, or two inside tmux (tmux's and the outer
/// terminal's through passthrough).
pub const fn sentinels(tmux: bool) -> usize {
    if tmux { 2 } else { 1 }
}

/// DA1 replies in the input so far.
pub fn da1_count(bytes: &[u8]) -> usize {
    let text = String::from_utf8_lossy(bytes);
    let mut count = 0;
    let mut rest: &str = &text;
    while let Some(start) = rest.find("\u{1b}[?") {
        let after = &rest[start + 3..];
        let params = after
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == ';')
            .count();
        if after[params..].starts_with('c') {
            count += 1;
        }
        rest = after;
    }
    count
}

/// Wrap a sequence for tmux passthrough (`allow-passthrough on`).
pub fn passthrough(sequence: &str) -> String {
    format!(
        "\u{1b}Ptmux;{}\u{1b}\\",
        sequence.replace('\u{1b}', "\u{1b}\u{1b}")
    )
}

/// Parse whatever arrived; partial input is fine.
pub fn parse(bytes: &[u8]) -> Replies {
    let text = String::from_utf8_lossy(bytes);
    let mut replies = Replies::default();
    if let Some(start) = text.find("\u{1b}P>|") {
        let rest = &text[start + 4..];
        let end = rest.find("\u{1b}\\").unwrap_or(rest.len());
        let name: String = rest[..end]
            .chars()
            .filter(|c| !c.is_control())
            .take(64)
            .collect();
        replies.name = Some(name);
    }
    replies.kitty_keyboard = find_csi(&text, "\u{1b}[?", 'u').and_then(|p| p.parse().ok());
    replies.kitty_graphics = text.contains("\u{1b}_Gi=31;OK");
    replies.placeholders = text.contains("\u{1b}_Gi=32;OK");
    replies.program_status = text.contains("\u{1b}]7501;?");
    replies.synchronized_output = text
        .find("\u{1b}[?2026;")
        .and_then(|i| text[i + 8..].chars().next())
        .is_some_and(|c| c == '1' || c == '2');
    replies.da1 = find_csi(&text, "\u{1b}[?", 'c').is_some();
    replies
}

/// The parameters of the first `prefix ... final` CSI whose parameters are digits and `;`.
fn find_csi(text: &str, prefix: &str, last: char) -> Option<String> {
    let mut rest = text;
    while let Some(start) = rest.find(prefix) {
        let after = &rest[start + prefix.len()..];
        let params: String = after
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == ';')
            .collect();
        if after[params.len()..].starts_with(last) {
            return Some(params);
        }
        rest = after;
    }
    None
}

/// Decide features from replies, environment and configuration.
pub fn decide(replies: Replies, env: Environment, config: &TerminalConfig) -> Caps {
    let pick = |toggle: Toggle, replied: bool| match toggle {
        Toggle::Auto => replied,
        Toggle::On => true,
        Toggle::Off => false,
    };
    let images = match config.images {
        Images::Off => ImageMode::Text,
        Images::Kitty => ImageMode::Kitty,
        Images::Placeholders => ImageMode::Placeholders,
        Images::Auto if env.tmux && replies.kitty_graphics && replies.placeholders => {
            ImageMode::Placeholders
        }
        Images::Auto if !env.tmux && replies.kitty_graphics => ImageMode::Kitty,
        Images::Auto => ImageMode::Text,
    };
    Caps {
        images,
        // tmux does not forward OSC 7501; herdr 0.9 reads report-agent instead.
        program_status: pick(config.status, replies.program_status && !env.tmux),
        herdr: pick(config.herdr, env.herdr_pane.is_some()) && env.herdr_pane.is_some(),
        synchronized_output: pick(config.synchronized_output, replies.synchronized_output),
        kitty_keyboard: pick(config.kitty_keyboard, replies.kitty_keyboard.is_some()),
        replies,
        env,
    }
}

/// Write the probe and collect replies until DA1 or the timeout. Raw mode must be on.
pub fn probe(tmux: bool, timeout: Duration) -> Result<Replies> {
    use rustix::event::{PollFd, PollFlags, poll};
    let mut out = std::io::stdout();
    out.write_all(query(tmux).as_bytes())
        .context("write probe")?;
    out.flush().context("flush probe")?;
    let stdin = rustix::stdio::stdin();
    let deadline = Instant::now() + timeout;
    let mut bytes = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        let mut fds = [PollFd::new(&stdin, PollFlags::IN)];
        let wait = rustix::time::Timespec::try_from(left).ok();
        if poll(&mut fds, wait.as_ref()).context("poll stdin")? == 0 {
            break;
        }
        let n = rustix::io::read(stdin, &mut buf).context("read replies")?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        if da1_count(&bytes) >= sentinels(tmux) {
            break;
        }
    }
    Ok(parse(&bytes))
}

/// Lines for `oqt-tui doctor terminal`.
pub fn report(caps: &Caps) -> Vec<String> {
    let yes = |b: bool| if b { "yes" } else { "no" };
    let r = &caps.replies;
    vec![
        format!(
            "terminal name (XTVERSION): {}",
            r.name.as_deref().unwrap_or("no reply")
        ),
        format!("probe completed (DA1): {}", yes(r.da1)),
        format!(
            "kitty keyboard: {}",
            r.kitty_keyboard
                .map_or("no reply".to_owned(), |f| format!("flags {f}"))
        ),
        format!("kitty graphics: {}", yes(r.kitty_graphics)),
        format!("kitty unicode placeholders: {}", yes(r.placeholders)),
        format!("OSC 7501 program status: {}", yes(r.program_status)),
        format!("synchronized output (2026): {}", yes(r.synchronized_output)),
        format!(
            "environment: TERM={} tmux={} herdr={} rex={} ssh={} NO_COLOR={}",
            if caps.env.term.is_empty() {
                "unset"
            } else {
                &caps.env.term
            },
            yes(caps.env.tmux),
            caps.env.herdr_pane.as_deref().map_or("no", |_| "yes"),
            yes(caps.env.rex),
            yes(caps.env.ssh),
            yes(caps.env.no_color)
        ),
        format!(
            "decided: images={:?} status={} herdr={} sync={} kitty_keyboard={}",
            caps.images,
            yes(caps.program_status),
            yes(caps.herdr),
            yes(caps.synchronized_output),
            yes(caps.kitty_keyboard)
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_kitty_like_answer() {
        let reply = b"\x1bP>|kitty(0.45.0)\x1b\\\x1b[?15u\x1b_Gi=31;OK\x1b\\\x1b_Gi=32;OK\x1b\\\x1b]7501;?\x1b\\\x1b[?2026;2$y\x1b[?62;c";
        let r = parse(reply);
        assert_eq!(r.name.as_deref(), Some("kitty(0.45.0)"));
        assert_eq!(r.kitty_keyboard, Some(15));
        assert!(
            r.kitty_graphics
                && r.placeholders
                && r.program_status
                && r.synchronized_output
                && r.da1
        );
    }

    #[test]
    fn plain_terminal_answers_only_da1() {
        let r = parse(b"\x1b[?1;2c");
        assert_eq!(
            r,
            Replies {
                da1: true,
                ..Replies::default()
            }
        );
        let caps = decide(r, Environment::default(), &TerminalConfig::default());
        assert_eq!(caps.images, ImageMode::Text);
        assert!(!caps.program_status && !caps.herdr);
    }

    #[test]
    fn graphics_error_is_not_support() {
        assert!(!parse(b"\x1b_Gi=31;ENOTSUPPORTED\x1b\\\x1b[?62c").kitty_graphics);
    }

    #[test]
    fn tmux_uses_placeholders_and_no_osc_7501() {
        let r = Replies {
            kitty_graphics: true,
            placeholders: true,
            program_status: true,
            da1: true,
            ..Replies::default()
        };
        let env = Environment {
            tmux: true,
            ..Environment::default()
        };
        let caps = decide(r, env, &TerminalConfig::default());
        assert_eq!(caps.images, ImageMode::Placeholders);
        assert!(!caps.program_status);
        assert!(query(true).contains("\u{1b}Ptmux;\u{1b}\u{1b}_Ga=q"));
        assert!(query(true).contains("\u{1b}\u{1b}[c\u{1b}\\"));
        assert_eq!(da1_count(b"\x1b[?62;c\x1b_Gi=31;OK\x1b\\\x1b[?62;22c"), 2);
    }

    #[test]
    fn herdr_needs_the_pane_id() {
        let env = Environment::read(&|name| match name {
            "HERDR_ENV" => Some("1".into()),
            "HERDR_PANE_ID" => Some("p_1".into()),
            _ => None,
        });
        let caps = decide(Replies::default(), env, &TerminalConfig::default());
        assert!(caps.herdr);
    }
}
