//! Terminal lifecycle, RAII.
//!
//! [`TerminalGuard`] enters raw mode + the alternate screen on construction and restores
//! the terminal when dropped. Apps hold it for the whole session; dropping it is the
//! single cleanup path, so there is no way to leave the terminal broken — even on panic.

use std::io::{self, Stdout};

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::{Frame, Terminal};

/// The backend used by byteowlz TUIs.
pub type Backend = CrosstermBackend<Stdout>;

/// RAII guard for the terminal: enter on creation, restore on drop.
///
/// Construct with [`TerminalGuard::enter`], then drive rendering through
/// [`TerminalGuard::draw`] / [`TerminalGuard::terminal_mut`]. On `Drop`, raw mode is
/// disabled and the alternate screen is left, so the user's shell is restored even if the
/// app panics.
#[derive(Debug)]
pub struct TerminalGuard {
    terminal: Terminal<Backend>,
}

impl TerminalGuard {
    /// Enter raw mode + the alternate screen.
    ///
    /// # Errors
    /// Propagates crossterm I/O errors from enabling raw mode or entering the alternate
    /// screen.
    pub fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }

    /// Draw one frame. Thin wrapper over [`Terminal::draw`].
    ///
    /// # Errors
    /// Propagates the backend's render error.
    pub fn draw<F>(&mut self, f: F) -> io::Result<ratatui::CompletedFrame<'_>>
    where
        F: FnOnce(&mut Frame<'_>),
    {
        self.terminal.draw(f)
    }

    /// Clear the terminal buffer.
    ///
    /// # Errors
    /// Propagates the backend I/O error.
    pub fn clear(&mut self) -> io::Result<()> {
        self.terminal.clear()
    }

    /// Borrow the inner terminal for advanced use.
    #[must_use]
    pub const fn terminal_mut(&mut self) -> &mut Terminal<Backend> {
        &mut self.terminal
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Best-effort restore. The guard's whole purpose is to never leave the terminal
        // broken; ignoring an error during teardown is the correct, bounded behavior.
        let backend = self.terminal.backend_mut();
        let _ = execute!(backend, LeaveAlternateScreen, DisableMouseCapture);
        let _ = self.terminal.show_cursor();
        let _ = disable_raw_mode();
    }
}
