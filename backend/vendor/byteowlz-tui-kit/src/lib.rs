//! # byteowlz-tui-kit
//!
//! The shared TUI substrate for byteowlz tools. This crate is the *mechanism*
//! layer; the [`tui-design`][skill] skill is the *judgment* layer that says when
//! to reach for each piece.
//!
//! What it provides:
//!
//! - [`theme`]: a [`Theme`][theme::Theme] mapping semantic [`Token`][theme::Token]s to
//!   **named ANSI colors** (never raw RGB by default), so every byteowlz tool matches
//!   the operator's terminal palette. See `docs/adr/0002-ansi-default-theme.md` in the
//!   skill for the rationale.
//! - [`action`]: an [`Action`][action::Action] registry plus a [`KeyRouter`][action::KeyRouter]
//!   prefix-state machine that resolves direct keys, multi-key **Key Progressions**, and
//!   transient WhichKey hints. Adding an action never adds a mode.
//! - [`palette`]: a fuzzy [`CommandPalette`][palette::CommandPalette] overlay — the
//!   type-to-discover path over the same actions.
//! - [`whichkey`]: the on-demand hint shown during a key progression.
//! - [`widgets`]: modern building blocks — [`Selection`][widgets::Selection] state,
//!   status bar, empty state, centered popups.
//! - [`event`] and [`terminal`]: a tiny crossterm event loop and a RAII
//!   [`TerminalGuard`][terminal::TerminalGuard] that restores the terminal on drop.
//!
//! The kit is deliberately **synchronous and dependency-light** (only `ratatui` +
//! `crossterm`); async apps wrap it themselves.
//!
//! [skill]: https://github.com/byteowlz/skillissues

pub mod action;
pub mod event;
pub mod fuzzy;
pub mod palette;
pub mod terminal;
pub mod theme;
pub mod whichkey;
pub mod widgets;

/// Convenient re-export of the types most apps need.
pub mod prelude {
    pub use crate::action::{Action, ActionId, Key, KeyRouter, Route};
    pub use crate::event::{AppEvent, poll_event};
    pub use crate::palette::{CommandPalette, PaletteOutcome};
    pub use crate::terminal::TerminalGuard;
    pub use crate::theme::{Theme, Token};
    pub use crate::widgets::{Selection, centered_rect, draw_empty_state, draw_status_bar};
}
