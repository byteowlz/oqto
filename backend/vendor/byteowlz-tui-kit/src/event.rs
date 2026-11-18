//! A minimal crossterm event loop.
//!
//! The kit stays synchronous: [`poll_event`] blocks for up to `tick`, returning a
//! normalized [`AppEvent`]. Async apps wrap this in a `tokio::task::spawn_blocking`.

use std::time::Duration;

use crossterm::event::{self, Event, KeyEventKind};

use crate::action::Key;

/// A normalized application event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEvent {
    /// A key press (normalized to a [`Key`]); key-release/-repeat are ignored.
    Key(Key),
    /// A terminal resize, in columns × rows.
    Resize(u16, u16),
    /// No input arrived within the tick window.
    Tick,
}

/// Block for up to `tick`, returning the next event or [`AppEvent::Tick`].
///
/// # Errors
/// Returns the underlying crossterm I/O error.
pub fn poll_event(tick: Duration) -> std::io::Result<Option<AppEvent>> {
    if event::poll(tick)? {
        Ok(Some(normalize(&event::read()?)))
    } else {
        Ok(Some(AppEvent::Tick))
    }
}

/// Normalize a raw crossterm event into an [`AppEvent`], ignoring non-key events.
fn normalize(event: &Event) -> AppEvent {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => AppEvent::Key(Key::from(*key)),
        Event::Resize(width, height) => AppEvent::Resize(*width, *height),
        _ => AppEvent::Tick,
    }
}
