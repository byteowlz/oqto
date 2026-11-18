//! Actions as data (tui-design I1, I9): Pi's action ids where Pi has one, `oqt.*` for client
//! actions. Keys come from `config.toml`; the table here only holds the shipped defaults.

use std::collections::BTreeMap;

use anyhow::{Result, bail};
use byteowlz_tui_kit::action::{Action, ActionId, Key};
use crossterm::event::{KeyCode, KeyModifiers};

/// Where an action applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Composer focus (the home state) and the palette.
    Composer,
    /// Transcript focus.
    Transcript,
}

/// One action: id, label, scope and default key paths.
pub struct Spec {
    /// Pi action id or `oqt.*`.
    pub id: &'static str,
    /// What the palette shows.
    pub label: &'static str,
    /// Where its keys apply.
    pub scope: Scope,
    /// Shipped defaults; `leader` stands for the leader key.
    pub defaults: &'static [&'static str],
}

const fn spec(
    id: &'static str,
    label: &'static str,
    scope: Scope,
    defaults: &'static [&'static str],
) -> Spec {
    Spec {
        id,
        label,
        scope,
        defaults,
    }
}

/// Every action of slice 1.
pub const SPECS: &[Spec] = &[
    spec(
        "tui.input.submit",
        "Send (steer while working)",
        Scope::Composer,
        &["enter"],
    ),
    spec(
        "app.message.followUp",
        "Queue as follow-up",
        Scope::Composer,
        &["alt+enter"],
    ),
    spec(
        "tui.input.newLine",
        "New line",
        Scope::Composer,
        &["shift+enter", "ctrl+j"],
    ),
    spec(
        "app.interrupt",
        "Close overlay, leave focus, or stop the run",
        Scope::Composer,
        &["escape"],
    ),
    spec(
        "app.clear",
        "Clear the draft (twice: detach)",
        Scope::Composer,
        &["ctrl+c"],
    ),
    spec(
        "app.exit",
        "Detach when the draft is empty",
        Scope::Composer,
        &["ctrl+d"],
    ),
    spec(
        "app.thinking.toggle",
        "Show or hide thinking",
        Scope::Composer,
        &["ctrl+t"],
    ),
    spec(
        "app.tools.expand",
        "Expand or collapse tool output",
        Scope::Composer,
        &["ctrl+o"],
    ),
    spec(
        "oqt.palette",
        "Command palette",
        Scope::Composer,
        &["leader :", "ctrl+k"],
    ),
    spec(
        "oqt.focus.transcript",
        "Focus the transcript",
        Scope::Composer,
        &["leader up", "pageup"],
    ),
    spec(
        "oqt.session.stop",
        "Stop the run (withdraws queued inputs)",
        Scope::Composer,
        &["leader x"],
    ),
    spec(
        "oqt.transcript.earlier",
        "Load earlier messages",
        Scope::Composer,
        &["leader e"],
    ),
    spec(
        "oqt.transcript.missing",
        "Fill missing messages",
        Scope::Composer,
        &["leader g"],
    ),
    spec(
        "oqt.note.save",
        "Save the draft as a note (the model never sees it)",
        Scope::Composer,
        &["leader w"],
    ),
    spec(
        "oqt.model.show",
        "Model and thinking (read-only in slice 1)",
        Scope::Composer,
        &["leader m"],
    ),
    spec(
        "oqt.copy.last",
        "Copy the last answer (OSC 52)",
        Scope::Composer,
        &["leader y"],
    ),
    spec(
        "oqt.help",
        "Keys and actions",
        Scope::Composer,
        &["leader ?"],
    ),
    spec(
        "oqt.quit",
        "Detach (the Session keeps running)",
        Scope::Composer,
        &["leader q"],
    ),
    spec(
        "tui.altScreen.lineDown",
        "Scroll down",
        Scope::Transcript,
        &["j", "down"],
    ),
    spec(
        "tui.altScreen.lineUp",
        "Scroll up",
        Scope::Transcript,
        &["k", "up"],
    ),
    spec(
        "tui.altScreen.halfPageDown",
        "Half page down",
        Scope::Transcript,
        &["ctrl+d"],
    ),
    spec(
        "tui.altScreen.halfPageUp",
        "Half page up",
        Scope::Transcript,
        &["ctrl+u"],
    ),
    spec(
        "tui.altScreen.pageDown",
        "Page down",
        Scope::Transcript,
        &["pagedown", "space"],
    ),
    spec(
        "tui.altScreen.pageUp",
        "Page up",
        Scope::Transcript,
        &["pageup"],
    ),
    spec(
        "tui.altScreen.top",
        "Top",
        Scope::Transcript,
        &["g g", "home"],
    ),
    spec(
        "tui.altScreen.bottom",
        "Bottom",
        Scope::Transcript,
        &["G", "end"],
    ),
    spec(
        "oqt.focus.composer",
        "Back to the composer",
        Scope::Transcript,
        &["i", "escape"],
    ),
];

/// The shipped bindings, as written to a new `config.toml`.
pub fn default_bindings() -> BTreeMap<String, Vec<String>> {
    SPECS
        .iter()
        .map(|s| {
            (
                s.id.to_owned(),
                s.defaults.iter().map(|k| (*k).to_owned()).collect(),
            )
        })
        .collect()
}

/// Parse one key such as `ctrl+space`, `alt+enter`, `G`, `pageup`.
pub fn parse_key(text: &str) -> Result<Key> {
    let mut modifiers = KeyModifiers::NONE;
    let mut parts: Vec<&str> = text.split('+').collect();
    // `+` itself: "ctrl++" splits into ["ctrl", "", ""].
    if text.ends_with("++") || text == "+" {
        parts.retain(|p| !p.is_empty());
        parts.push("+");
    }
    let Some((name, mods)) = parts.split_last() else {
        bail!("empty key")
    };
    for modifier in mods {
        modifiers |= match modifier.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "c" => KeyModifiers::CONTROL,
            "alt" | "meta" | "option" | "a" => KeyModifiers::ALT,
            "shift" | "s" => KeyModifiers::SHIFT,
            other => bail!("unknown modifier {other} in {text}"),
        };
    }
    let code = match name.to_ascii_lowercase().as_str() {
        "enter" | "return" => KeyCode::Enter,
        "escape" | "esc" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "backspace" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "space" => KeyCode::Char(' '),
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        lower if lower.len() > 1 && lower.starts_with('f') && lower[1..].parse::<u8>().is_ok() => {
            KeyCode::F(lower[1..].parse().unwrap_or(1))
        }
        _ => {
            let mut chars = name.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => KeyCode::Char(c),
                _ => bail!("unknown key {name} in {text}"),
            }
        }
    };
    Ok(normalize(Key { code, modifiers }))
}

/// One shape per key: uppercase letters carry no SHIFT, shift+tab is BackTab.
pub fn normalize(key: Key) -> Key {
    match key.code {
        KeyCode::Char(c) if c.is_ascii_uppercase() || !c.is_ascii_alphabetic() => Key {
            code: key.code,
            modifiers: key.modifiers - KeyModifiers::SHIFT,
        },
        KeyCode::BackTab => Key {
            code: KeyCode::BackTab,
            modifiers: key.modifiers - KeyModifiers::SHIFT,
        },
        _ => key,
    }
}

/// Parse a key path: keys separated by spaces, `leader` for the leader key.
pub fn parse_path(text: &str, leader: Key) -> Result<Vec<Key>> {
    let path: Vec<Key> = text
        .split_whitespace()
        .map(|part| {
            if part == "leader" {
                Ok(leader)
            } else {
                parse_key(part)
            }
        })
        .collect::<Result<_>>()?;
    if path.is_empty() {
        bail!("empty key path");
    }
    Ok(path)
}

/// The kit actions for one scope, with keys from the configuration.
pub fn actions(
    scope: Scope,
    leader: Key,
    bindings: &BTreeMap<String, Vec<String>>,
) -> Result<Vec<Action>> {
    let mut out = Vec::new();
    for spec in SPECS.iter().filter(|s| s.scope == scope) {
        let paths = bindings.get(spec.id).map_or_else(
            || spec.defaults.iter().map(|s| (*s).to_owned()).collect(),
            Clone::clone,
        );
        if paths.is_empty() {
            // Unbound: still reachable from the palette.
            out.push(Action::new(ActionId::new(spec.id), spec.label));
        }
        for path in paths {
            let keys = parse_path(&path, leader)
                .map_err(|e| anyhow::anyhow!("keys.bindings.\"{}\": {e}", spec.id))?;
            out.push(Action::new(ActionId::new(spec.id), spec.label).keys(&keys));
        }
    }
    Ok(out)
}

/// Unknown action ids in the configuration are an error, not silently ignored.
pub fn check_ids(bindings: &BTreeMap<String, Vec<String>>) -> Result<()> {
    for id in bindings.keys() {
        if !SPECS.iter().any(|s| s.id == id) {
            bail!("keys.bindings: unknown action id {id}");
        }
    }
    Ok(())
}

/// Palette entries: one per action id with its first key path.
pub fn palette_actions(composer: &[Action]) -> Vec<Action> {
    let mut seen: Vec<&str> = Vec::new();
    composer
        .iter()
        .filter(|a| {
            let id = a.id().as_str();
            let keep = !seen.contains(&id) && id != "oqt.palette";
            seen.push(id);
            keep
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modifiers_and_names() -> Result<()> {
        assert_eq!(
            parse_key("ctrl+space")?,
            Key {
                code: KeyCode::Char(' '),
                modifiers: KeyModifiers::CONTROL
            }
        );
        assert_eq!(
            parse_key("alt+enter")?,
            Key {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::ALT
            }
        );
        assert_eq!(parse_key("G")?, Key::char('G'));
        assert_eq!(parse_key("shift+tab")?.code, KeyCode::Tab);
        assert!(parse_key("hyper+x").is_err());
        Ok(())
    }

    #[test]
    fn leader_expands_in_paths() -> Result<()> {
        let leader = parse_key("ctrl+space")?;
        assert_eq!(
            parse_path("leader :", leader)?,
            vec![leader, Key::char(':')]
        );
        Ok(())
    }

    #[test]
    fn defaults_build_and_unknown_ids_are_refused() -> Result<()> {
        let leader = parse_key("ctrl+space")?;
        let bindings = default_bindings();
        check_ids(&bindings)?;
        assert!(actions(Scope::Composer, leader, &bindings)?.len() > 10);
        let mut bad = bindings;
        bad.insert("app.nonsense".into(), vec!["x".into()]);
        assert!(check_ids(&bad).is_err());
        Ok(())
    }
}
