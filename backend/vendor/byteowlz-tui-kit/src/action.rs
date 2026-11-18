//! Actions, key paths, and the prefix router.
//!
//! This is the core lever against mode explosion. An [`Action`] is *data*: a stable id,
//! a human label, and a **Key Path** (a sequence of [`Key`]s). Actions are registered
//! once and surfaced two ways: the command palette (type-to-discover) and key
//! progressions (chord-to-discover). Adding an action never adds a mode.
//!
//! A [`KeyRouter`] is a prefix state machine: feed it keys in Normal mode and it returns
//! either a completed [`ActionId`], a transient `Prefix` hint (more keys expected), or a
//! `Miss`. This is how `dd`, `gg`, and `s d`-style progressions work, with an on-demand
//! WhichKey hint instead of a permanent bar.

use core::fmt;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// A normalized key for binding and comparison: a [`KeyCode`] plus its [`KeyModifiers`].
///
/// Key *kind* and *state* (which vary across terminals and press/release cycles) are
/// ignored, so two presses of the same logical key always compare equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    /// The logical key.
    pub code: KeyCode,
    /// Active modifiers.
    pub modifiers: KeyModifiers,
}

impl Key {
    /// Any key with no modifiers.
    #[must_use]
    pub const fn plain(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: KeyModifiers::NONE,
        }
    }

    /// A plain printable character, e.g. `Key::char('d')`.
    #[must_use]
    pub const fn char(c: char) -> Self {
        Self::plain(KeyCode::Char(c))
    }

    /// A control-character chord, e.g. `Key::ctrl_char('c')`.
    #[must_use]
    pub const fn ctrl_char(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::CONTROL,
        }
    }

    /// The Enter key.
    #[must_use]
    pub const fn enter() -> Self {
        Self::plain(KeyCode::Enter)
    }

    /// The Escape key.
    #[must_use]
    pub const fn esc() -> Self {
        Self::plain(KeyCode::Esc)
    }

    /// The Tab key.
    #[must_use]
    pub const fn tab() -> Self {
        Self::plain(KeyCode::Tab)
    }

    /// The Backspace key.
    #[must_use]
    pub const fn backspace() -> Self {
        Self::plain(KeyCode::Backspace)
    }

    /// The Space key.
    #[must_use]
    pub const fn space() -> Self {
        Self::plain(KeyCode::Char(' '))
    }

    /// The Up arrow.
    #[must_use]
    pub const fn up() -> Self {
        Self::plain(KeyCode::Up)
    }

    /// The Down arrow.
    #[must_use]
    pub const fn down() -> Self {
        Self::plain(KeyCode::Down)
    }
}

impl From<KeyEvent> for Key {
    fn from(event: KeyEvent) -> Self {
        Self {
            code: event.code,
            modifiers: event.modifiers,
        }
    }
}

/// Stable identifier for an action, e.g. `item.delete`.
///
/// Newtype over `&'static str` so action ids are cheap, comparable, and live as data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ActionId(&'static str);

impl ActionId {
    /// Create an id from a static string.
    #[must_use]
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    /// The underlying id string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for ActionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// One named thing the user can do. Data, not code.
///
/// Build with [`Action::new`] and the `key`/`keys`/`group` combinators:
///
/// ```
/// use byteowlz_tui_kit::action::{Action, ActionId, Key};
/// let actions = vec![
///     Action::new(ActionId::new("item.delete"), "Delete").key(Key::char('d')),
///     Action::new(ActionId::new("sort.date"), "Sort by date")
///         .keys(&[Key::char('s'), Key::char('d')]),
/// ];
/// let _ = actions;
/// ```
#[derive(Debug, Clone)]
pub struct Action {
    /// Stable id.
    id: ActionId,
    /// Human label shown in the palette and help.
    label: &'static str,
    /// The Key Path: `[]` unbound, `[k]` direct, `[k1, k2]` a progression.
    keys: Vec<Key>,
    /// Optional grouping for palette/help sections, e.g. `"item"`.
    group: &'static str,
}

impl Action {
    /// Create an unbound action with a label.
    #[must_use]
    pub const fn new(id: ActionId, label: &'static str) -> Self {
        Self {
            id,
            label,
            keys: Vec::new(),
            group: "",
        }
    }

    /// Append one key to the Key Path (builder).
    #[must_use]
    pub fn key(mut self, key: Key) -> Self {
        self.keys.push(key);
        self
    }

    /// Set the whole Key Path from a slice (builder).
    #[must_use]
    pub fn keys(mut self, keys: &[Key]) -> Self {
        self.keys.extend_from_slice(keys);
        self
    }

    /// Set the grouping (builder).
    #[must_use]
    pub const fn group(mut self, group: &'static str) -> Self {
        self.group = group;
        self
    }

    /// The action's stable id.
    #[must_use]
    pub const fn id(&self) -> ActionId {
        self.id
    }

    /// The human label.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        self.label
    }

    /// The grouping, or `""` if ungrouped.
    #[must_use]
    pub const fn group_str(&self) -> &'static str {
        self.group
    }

    /// The Key Path as a slice of keys.
    #[must_use]
    pub fn key_path(&self) -> &[Key] {
        &self.keys
    }

    /// A compact, human-readable hint for the Key Path, e.g. `"s d"` or `"C-c"`.
    ///
    /// Empty string when unbound. Used by the palette and help to show the binding.
    #[must_use]
    pub fn key_hint(&self) -> String {
        key_path_hint(&self.keys)
    }
}

/// Render a key path as a compact, human-readable hint.
///
/// Each key is joined by a single space; modifier chords use a `Mod-key` form.
#[must_use]
pub fn key_path_hint(path: &[Key]) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(path.len());
    for key in path {
        parts.push(key_hint_single(key));
    }
    parts.join(" ")
}

/// Render one key as a compact hint.
fn key_hint_single(key: &Key) -> String {
    let base = match key.code {
        KeyCode::Enter => "Ret".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::BackTab => "S-Tab".to_string(),
        KeyCode::Backspace => "Bksp".to_string(),
        KeyCode::Up => "Up".to_string(),
        KeyCode::Down => "Down".to_string(),
        KeyCode::Left => "Left".to_string(),
        KeyCode::Right => "Right".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::PageUp => "PgUp".to_string(),
        KeyCode::PageDown => "PgDn".to_string(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::F(n) => format!("F{n}"),
        _ => "???".to_string(),
    };
    apply_modifiers(&base, key.modifiers)
}

/// Prefix the base hint with modifier sigils.
fn apply_modifiers(base: &str, modifiers: KeyModifiers) -> String {
    let mut out = String::new();
    if modifiers.contains(KeyModifiers::CONTROL) {
        out.push_str("C-");
    }
    if modifiers.contains(KeyModifiers::ALT) {
        out.push_str("A-");
    }
    if modifiers.contains(KeyModifiers::SHIFT) && !base.starts_with('S') {
        out.push_str("S-");
    }
    out.push_str(base);
    out
}

/// The result of feeding one key to a [`KeyRouter`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// A complete Key Path matched an action.
    Action(ActionId),
    /// The keys so far are a valid prefix; more keys are expected.
    ///
    /// Carries the next-key options to show as a transient WhichKey hint:
    /// `(next key, action label)`.
    Prefix(Vec<(Key, &'static str)>),
    /// No action matches this sequence; the prefix was cleared.
    Miss,
}

/// A prefix-state machine that resolves a stream of [`Key`]s into [`ActionId`]s.
///
/// Created per render/Normal-mode session over a borrowed action table. Call
/// [`KeyRouter::feed`] for each key. On [`Route::Prefix`], show the hint (see
/// [`crate::whichkey`]); on [`Route::Action`], run the action; on [`Route::Miss`], do
/// nothing. [`KeyRouter::reset`] clears an in-progress prefix (call on Escape).
#[derive(Debug)]
pub struct KeyRouter<'a> {
    actions: &'a [Action],
    prefix: Vec<Key>,
}

impl<'a> KeyRouter<'a> {
    /// Create a router over an action table.
    #[must_use]
    pub const fn new(actions: &'a [Action]) -> Self {
        Self {
            actions,
            prefix: Vec::new(),
        }
    }

    /// Whether a key progression is in progress.
    #[must_use]
    pub const fn has_prefix(&self) -> bool {
        !self.prefix.is_empty()
    }

    /// The action table the router was created over.
    ///
    /// Returns the stored reference with its original lifetime (a copy of the reference,
    /// not a reborrow), so a `'static` table stays `'static`.
    #[must_use]
    pub const fn actions_ref(&self) -> &'a [Action] {
        self.actions
    }

    /// The keys accumulated so far in an in-progress prefix.
    #[must_use]
    pub fn prefix_ref(&self) -> &[Key] {
        &self.prefix
    }

    /// Clear any in-progress prefix (e.g. on Escape).
    pub fn reset(&mut self) {
        self.prefix.clear();
    }

    /// Feed one key and return the routing decision.
    pub fn feed(&mut self, key: Key) -> Route {
        self.prefix.push(key);
        let prefix = self.prefix.clone();

        if let Some(action) = self
            .actions
            .iter()
            .find(|a| a.key_path() == prefix.as_slice())
        {
            self.prefix.clear();
            return Route::Action(action.id());
        }
        let hint = self.collect_prefix_hint(&prefix);
        if hint.is_empty() {
            self.prefix.clear();
            Route::Miss
        } else {
            Route::Prefix(hint)
        }
    }

    /// Collect the next-key options for the given prefix, deduplicated by key.
    fn collect_prefix_hint(&self, prefix: &[Key]) -> Vec<(Key, &'static str)> {
        let mut options: Vec<(Key, &'static str)> = Vec::new();
        for action in self.actions {
            let Some(next) = next_key_after_prefix(action.key_path(), prefix) else {
                continue;
            };
            if !options.iter().any(|(k, _)| *k == next) {
                options.push((next, action.label()));
            }
        }
        options
    }
}

/// Return the key that follows `prefix` in `path`, if `path` starts with `prefix`
/// and is strictly longer.
fn next_key_after_prefix(path: &[Key], prefix: &[Key]) -> Option<Key> {
    if path.len() <= prefix.len() {
        return None;
    }
    if path.starts_with(prefix) {
        Some(path[prefix.len()])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures() -> Vec<Action> {
        vec![
            Action::new(ActionId::new("delete"), "Delete").key(Key::char('d')),
            Action::new(ActionId::new("sort.date"), "by date")
                .keys(&[Key::char('s'), Key::char('d')]),
            Action::new(ActionId::new("sort.imp"), "by importance")
                .keys(&[Key::char('s'), Key::char('i')]),
        ]
    }

    #[test]
    fn direct_key_matches() {
        let actions = fixtures();
        let mut router = KeyRouter::new(&actions);
        assert_eq!(
            router.feed(Key::char('d')),
            Route::Action(ActionId::new("delete"))
        );
        assert!(!router.has_prefix());
    }

    #[test]
    fn multi_key_progression_reports_prefix_then_action() {
        let actions = fixtures();
        let mut router = KeyRouter::new(&actions);
        let first = router.feed(Key::char('s'));
        assert!(
            matches!(first, Route::Prefix(ref h) if h.len() == 2),
            "expected Prefix with 2 options, got {first:?}"
        );
        assert!(router.has_prefix());
        assert_eq!(
            router.feed(Key::char('d')),
            Route::Action(ActionId::new("sort.date"))
        );
    }

    #[test]
    fn unknown_key_after_prefix_is_a_miss() {
        let actions = fixtures();
        let mut router = KeyRouter::new(&actions);
        let _ = router.feed(Key::char('s'));
        assert_eq!(router.feed(Key::char('z')), Route::Miss);
        assert!(!router.has_prefix());
    }

    #[test]
    fn key_hint_formats_chords() {
        assert_eq!(Action::new(ActionId::new("x"), "X").key_hint(), "");
        let a = Action::new(ActionId::new("y"), "Y").keys(&[Key::char('s'), Key::char('d')]);
        assert_eq!(a.key_hint(), "s d");
        let b = Action::new(ActionId::new("z"), "Z").key(Key::ctrl_char('c'));
        assert_eq!(b.key_hint(), "C-c");
    }
}
