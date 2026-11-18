//! Program status for the terminal host (DESIGN.md 10, 11): OSC 7501 when the terminal
//! answered its query, herdr `report-agent` inside herdr, nothing otherwise. Status carries
//! the state and at most a dialog heading or the first line of an error; never prompts,
//! arguments or output. It is presentation only and grants nothing.

use std::process::{Command, Stdio};

use base64::Engine;

/// What the program is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Nothing running.
    Idle,
    /// A run is working.
    Working,
    /// Waiting for the user.
    Blocked {
        /// `permission`, `question` or `auth`.
        kind: &'static str,
        /// A heading, never content.
        message: String,
    },
    /// The last run finished.
    Done,
    /// The last run failed.
    Error {
        /// First line of the failure reason.
        message: String,
    },
    /// Remove the status (on exit).
    Clear,
}

impl State {
    const fn name(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Blocked { .. } => "blocked",
            Self::Done => "done",
            Self::Error { .. } => "error",
            Self::Clear => "clear",
        }
    }

    fn message(&self) -> &str {
        match self {
            Self::Blocked { message, .. } | Self::Error { message } => message,
            _ => "",
        }
    }
}

const MAX_MESSAGE_BYTES: usize = 2048;

fn one_line(text: &str) -> String {
    let line: String = text
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let mut out = String::new();
    for c in line.trim().chars() {
        if out.len() + c.len_utf8() > MAX_MESSAGE_BYTES {
            break;
        }
        out.push(c);
    }
    out
}

/// The OSC 7501 report (Rex program status, revision 0.4; same encoding as pi-tui).
pub fn osc7501(state: &State) -> String {
    let mut pairs = vec![format!("state={}", state.name()), "app=oqt".to_owned()];
    if let State::Blocked { kind, .. } = state {
        pairs.push(format!("kind={kind}"));
    }
    let message = one_line(state.message());
    if !message.is_empty() {
        pairs.push(format!(
            "msg={}",
            base64::engine::general_purpose::STANDARD.encode(message)
        ));
    }
    format!("\u{1b}]7501;{}\u{1b}\\", pairs.join(":"))
}

/// The `herdr pane report-agent` arguments; the resume command reopens this view.
pub fn herdr_args(
    pane: &str,
    state: &State,
    seq: u64,
    session: &str,
    resume: &[String],
) -> Vec<String> {
    let herdr_state = match state {
        State::Working => "working",
        State::Blocked { .. } => "blocked",
        State::Idle | State::Done | State::Error { .. } | State::Clear => "idle",
    };
    let mut args = vec![
        "pane".to_owned(),
        "report-agent".to_owned(),
        pane.to_owned(),
        "--source".to_owned(),
        "oqt-tui".to_owned(),
        "--agent".to_owned(),
        "oqt".to_owned(),
        "--state".to_owned(),
        herdr_state.to_owned(),
        "--seq".to_owned(),
        seq.to_string(),
    ];
    let message = one_line(state.message());
    if !message.is_empty() {
        args.extend(["--message".to_owned(), message]);
    }
    args.extend([
        "--agent-session-id".to_owned(),
        session.to_owned(),
        "--".to_owned(),
    ]);
    args.extend(resume.iter().cloned());
    args
}

/// Sends status changes to whatever the terminal host understands.
#[derive(Debug)]
pub struct Reporter {
    osc: bool,
    herdr_pane: Option<String>,
    herdr_bin: String,
    session: String,
    resume: Vec<String>,
    last: Option<State>,
    seq: u64,
}

impl Reporter {
    /// A reporter; `herdr_pane` only when inside herdr and enabled.
    pub fn new(
        osc: bool,
        herdr_pane: Option<String>,
        session: String,
        resume: Vec<String>,
    ) -> Self {
        let herdr_bin = std::env::var("HERDR_BIN_PATH")
            .ok()
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| "herdr".to_owned());
        // herdr wants seq to increase across restarts.
        let seq = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
        Self {
            osc,
            herdr_pane,
            herdr_bin,
            session,
            resume,
            last: None,
            seq,
        }
    }

    /// Report `state` if it changed; returns the bytes to write to the terminal.
    pub fn report(&mut self, state: State) -> Option<String> {
        if self.last.as_ref() == Some(&state) {
            return None;
        }
        self.seq += 1;
        if let Some(pane) = &self.herdr_pane {
            let args = herdr_args(pane, &state, self.seq, &self.session, &self.resume);
            // Fire and forget; a missing herdr never blocks the client.
            let _ = Command::new(&self.herdr_bin)
                .args(&args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map(|mut child| std::thread::spawn(move || child.wait()));
        }
        let bytes = self.osc.then(|| osc7501(&state));
        self.last = Some(state);
        bytes
    }

    /// Whether anything is reported at all.
    pub const fn active(&self) -> bool {
        self.osc || self.herdr_pane.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc_matches_pi_tui_encoding() {
        assert_eq!(
            osc7501(&State::Working),
            "\u{1b}]7501;state=working:app=oqt\u{1b}\\"
        );
        let blocked = osc7501(&State::Blocked {
            kind: "question",
            message: "Pick one\nsecret".into(),
        });
        assert_eq!(
            blocked,
            "\u{1b}]7501;state=blocked:app=oqt:kind=question:msg=UGljayBvbmU=\u{1b}\\"
        );
    }

    #[test]
    fn herdr_report_carries_the_resume_command() {
        let args = herdr_args(
            "p_1",
            &State::Working,
            7,
            "ses_chat",
            &["oqt-tui".into(), "--session".into(), "ses_chat".into()],
        );
        let joined = args.join(" ");
        assert!(joined.starts_with(
            "pane report-agent p_1 --source oqt-tui --agent oqt --state working --seq 7"
        ));
        assert!(joined.ends_with("--agent-session-id ses_chat -- oqt-tui --session ses_chat"));
    }

    #[test]
    fn unchanged_state_is_not_resent() {
        let mut reporter = Reporter::new(true, None, "s".into(), vec![]);
        assert!(reporter.report(State::Idle).is_some());
        assert!(reporter.report(State::Idle).is_none());
        assert!(reporter.report(State::Working).is_some());
    }
}
