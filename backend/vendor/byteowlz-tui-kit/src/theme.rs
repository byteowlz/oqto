//! The ANSI-default theme.
//!
//! [`Token`]s are semantic slots. A [`Theme`] resolves each token to a **named ANSI
//! color** taken from the terminal's 16-color palette — never a raw RGB value by
//! default. The terminal (and its theme manager, e.g. tinty) therefore owns the actual
//! rendering, and every byteowlz tool automatically matches the operator's light/dark
//! theme and palette with zero per-tool configuration.
//!
//! Tokens serve double duty: each can be used as a **foreground** (`.fg()`) **or** as a
//! **background fill** (`.bg()`). Modern byteowlz TUIs use `SurfaceAlt`/`Bar` fills for
//! header/status bars and panel backgrounds, and `Panel` for thin borders — *that* is
//! where the visual structure comes from. `Color::Reset` (transparent) is reserved for
//! the content background only; using it for bars produces the flat, structureless look
//! the skill warns against.

use ratatui::style::{Color, Modifier, Style};

/// A semantic color slot. Code references these instead of raw colors.
///
/// Each variant maps to one role; see [`Theme::color`] for the resolution. A token can be
/// used as a foreground (`.fg()`) or a background fill (`.bg()`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Token {
    /// Content background; resolves to the terminal default (`Reset`) so content floats
    /// on the operator's bg. **Do not use this for bars/panels** — use [`Token::Bar`].
    Surface,
    /// A distinct surface shade for header/status bars and panel fills. Resolves to the
    /// terminal's `Black` (mapped by the theme to a dark surface). This is how regions
    /// get visible structure without raw RGB.
    Bar,
    /// Primary text; resolves to the terminal's default foreground (`Reset`).
    Primary,
    /// Secondary text, metadata, hints, and **thin borders**. Resolves to `DarkGray`.
    Muted,
    /// The single saturated color reserved for focus and primary actions.
    Accent,
    /// Positive state (ok, success).
    Success,
    /// Destructive state (delete, error).
    Danger,
    /// Cautionary state.
    Warning,
    /// Neutral informational state.
    Info,
}

/// A theme: a mapping from [`Token`] to color, applied identically across every
/// screen and every tool.
///
/// Use [`Theme::ansi_default`] unless you have a stated reason to override (a
/// deliberately branded tool). Designing with raw hex is the smell; pinning a token to
/// RGB is an opt-in exception you state explicitly via [`Theme::with_token`].
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    surface: Color,
    bar: Color,
    primary: Color,
    muted: Color,
    accent: Color,
    success: Color,
    danger: Color,
    warning: Color,
    info: Color,
}

impl Theme {
    /// The default ANSI theme. Defers every color to the terminal palette.
    ///
    /// `Surface`/`Primary` use `Reset` (terminal default bg/fg) so content floats; `Bar`
    /// uses `Black` (a real fill the terminal maps to a dark surface) for structured
    /// strips/panels; borders use `Muted` (`DarkGray`); there is exactly one `Accent`.
    #[must_use]
    pub const fn ansi_default() -> Self {
        Self {
            surface: Color::Reset,
            bar: Color::Black,
            primary: Color::Reset,
            muted: Color::DarkGray,
            accent: Color::Blue,
            success: Color::Green,
            danger: Color::Red,
            warning: Color::Yellow,
            info: Color::Cyan,
        }
    }

    /// Override a single token's color.
    ///
    /// Use sparingly and with a stated reason. Pinning `Accent` to an RGB for a
    /// branded tool is the canonical legitimate use.
    #[must_use]
    pub const fn with_token(mut self, token: Token, color: Color) -> Self {
        match token {
            Token::Surface => self.surface = color,
            Token::Bar => self.bar = color,
            Token::Primary => self.primary = color,
            Token::Muted => self.muted = color,
            Token::Accent => self.accent = color,
            Token::Success => self.success = color,
            Token::Danger => self.danger = color,
            Token::Warning => self.warning = color,
            Token::Info => self.info = color,
        }
        self
    }

    /// Resolve a token to its color.
    #[must_use]
    pub const fn color(self, token: Token) -> Color {
        match token {
            Token::Surface => self.surface,
            Token::Bar => self.bar,
            Token::Primary => self.primary,
            Token::Muted => self.muted,
            Token::Accent => self.accent,
            Token::Success => self.success,
            Token::Danger => self.danger,
            Token::Warning => self.warning,
            Token::Info => self.info,
        }
    }

    /// A plain foreground style for the token (no modifiers).
    #[must_use]
    pub const fn fg(self, token: Token) -> Style {
        Style::new().fg(self.color(token))
    }

    /// A bold foreground style for the token — the primary hierarchy mechanism.
    #[must_use]
    pub const fn fg_bold(self, token: Token) -> Style {
        self.fg(token).add_modifier(Modifier::BOLD)
    }

    /// A background-fill style for the token. Use `Bar` for header/status bars and panel
    /// fills — this is what gives regions visible structure.
    #[must_use]
    pub const fn bg(self, token: Token) -> Style {
        Style::new().bg(self.color(token))
    }

    /// A style combining a foreground on a `Bar` background — for text drawn on a filled
    /// strip (header/status bar).
    #[must_use]
    pub const fn on_bar(self, token: Token) -> Style {
        Style::new().fg(self.color(token)).bg(self.bar)
    }

    /// A bold variant of [`Theme::on_bar`].
    #[must_use]
    pub const fn on_bar_bold(self, token: Token) -> Style {
        self.on_bar(token).add_modifier(Modifier::BOLD)
    }

    /// A dimmed style — for secondary/metadata text.
    #[must_use]
    pub const fn dim(self) -> Style {
        Style::new().add_modifier(Modifier::DIM)
    }

    /// An accented bold style — for the focused/active row of a list or menu.
    #[must_use]
    pub const fn focus(self) -> Style {
        self.fg(Token::Accent).add_modifier(Modifier::BOLD)
    }

    /// A thin, dim border style for an inactive panel. Resolves to `Muted`.
    #[must_use]
    pub const fn border(self) -> Style {
        self.fg(Token::Muted)
    }

    /// An accented border style for the active/focused panel.
    #[must_use]
    pub const fn border_focus(self) -> Style {
        self.fg(Token::Accent)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::ansi_default()
    }
}
