//! The composer: a small multi-line editor with Pi's default editing keys (PARITY.md 3).
//! The cursor is a byte index on a character boundary; widths use `unicode-width`.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_width::UnicodeWidthChar;

/// What a key did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edit {
    /// The text or cursor changed.
    Changed,
    /// The key is not an editing key.
    Ignored,
}

/// The draft and its cursor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Editor {
    text: String,
    cursor: usize,
    history: Vec<String>,
    browsing: Option<usize>,
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl Editor {
    /// The draft.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Nothing typed.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Replace the draft.
    pub fn set(&mut self, text: &str) {
        self.text = text.to_owned();
        self.cursor = self.text.len();
    }

    /// Take the draft for sending and remember it in the history.
    pub fn take(&mut self) -> String {
        let text = std::mem::take(&mut self.text);
        self.cursor = 0;
        self.browsing = None;
        if !text.trim().is_empty() && self.history.last() != Some(&text) {
            self.history.push(text.clone());
        }
        text
    }

    /// Insert text at the cursor (typing or a bracketed paste).
    pub fn insert(&mut self, text: &str) {
        let clean: String = text
            .chars()
            .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
            .collect();
        self.text.insert_str(self.cursor, &clean);
        self.cursor += clean.len();
    }

    fn prev_boundary(&self, at: usize) -> usize {
        self.text[..at]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i)
    }

    fn next_boundary(&self, at: usize) -> usize {
        self.text[at..]
            .chars()
            .next()
            .map_or(at, |c| at + c.len_utf8())
    }

    fn line_start(&self) -> usize {
        self.text[..self.cursor].rfind('\n').map_or(0, |i| i + 1)
    }

    fn line_end(&self) -> usize {
        self.text[self.cursor..]
            .find('\n')
            .map_or(self.text.len(), |i| self.cursor + i)
    }

    fn word_left(&self) -> usize {
        let mut at = self.cursor;
        let chars: Vec<(usize, char)> = self.text[..at].char_indices().collect();
        let mut index = chars.len();
        while index > 0 && !is_word(chars[index - 1].1) {
            index -= 1;
        }
        while index > 0 && is_word(chars[index - 1].1) {
            index -= 1;
        }
        if let Some((i, _)) = chars.get(index) {
            at = *i;
        } else {
            at = 0;
        }
        at
    }

    fn word_right(&self) -> usize {
        let rest = &self.text[self.cursor..];
        let mut seen_word = false;
        for (i, c) in rest.char_indices() {
            if is_word(c) {
                seen_word = true;
            } else if seen_word {
                return self.cursor + i;
            }
        }
        self.text.len()
    }

    fn column(&self) -> usize {
        self.text[self.line_start()..self.cursor].chars().count()
    }

    fn move_vertical(&mut self, down: bool) -> bool {
        let column = self.column();
        let start = self.line_start();
        let target_start = if down {
            match self.text[self.cursor..].find('\n') {
                Some(i) => self.cursor + i + 1,
                None => return false,
            }
        } else {
            if start == 0 {
                return false;
            }
            self.text[..start - 1].rfind('\n').map_or(0, |i| i + 1)
        };
        let line_end = self.text[target_start..]
            .find('\n')
            .map_or(self.text.len(), |i| target_start + i);
        let mut at = target_start;
        for _ in 0..column {
            if at >= line_end {
                break;
            }
            at = self.next_boundary(at);
        }
        self.cursor = at;
        true
    }

    fn history(&mut self, older: bool) {
        if self.history.is_empty() {
            return;
        }
        let next = match (self.browsing, older) {
            (None, true) => Some(self.history.len() - 1),
            (None, false) => None,
            (Some(i), true) => Some(i.saturating_sub(1)),
            (Some(i), false) if i + 1 < self.history.len() => Some(i + 1),
            (Some(_), false) => None,
        };
        self.browsing = next;
        let text = next
            .and_then(|i| self.history.get(i))
            .cloned()
            .unwrap_or_default();
        self.set(&text);
    }

    /// Apply an editing key.
    pub fn key(&mut self, event: KeyEvent) -> Edit {
        let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
        let alt = event.modifiers.contains(KeyModifiers::ALT);
        match event.code {
            KeyCode::Char(c) if !ctrl && !alt => self.insert(c.encode_utf8(&mut [0; 4])),
            KeyCode::Char('a') if ctrl => self.cursor = self.line_start(),
            KeyCode::Char('e') if ctrl => self.cursor = self.line_end(),
            KeyCode::Char('b') if ctrl => self.cursor = self.prev_boundary(self.cursor),
            KeyCode::Char('f') if ctrl => self.cursor = self.next_boundary(self.cursor),
            KeyCode::Char('b') if alt => self.cursor = self.word_left(),
            KeyCode::Char('f') if alt => self.cursor = self.word_right(),
            KeyCode::Char('u') if ctrl => {
                let start = self.line_start();
                self.text.replace_range(start..self.cursor, "");
                self.cursor = start;
            }
            KeyCode::Char('k') if ctrl => {
                let end = self.line_end();
                self.text.replace_range(self.cursor..end, "");
            }
            KeyCode::Char('w') if ctrl => self.delete_word_backward(),
            KeyCode::Backspace if alt => self.delete_word_backward(),
            KeyCode::Char('d') if alt => {
                let end = self.word_right();
                self.text.replace_range(self.cursor..end, "");
            }
            KeyCode::Backspace => {
                let start = self.prev_boundary(self.cursor);
                self.text.replace_range(start..self.cursor, "");
                self.cursor = start;
            }
            KeyCode::Delete => {
                let end = self.next_boundary(self.cursor);
                self.text.replace_range(self.cursor..end, "");
            }
            KeyCode::Left if ctrl || alt => self.cursor = self.word_left(),
            KeyCode::Right if ctrl || alt => self.cursor = self.word_right(),
            KeyCode::Left => self.cursor = self.prev_boundary(self.cursor),
            KeyCode::Right => self.cursor = self.next_boundary(self.cursor),
            KeyCode::Home => self.cursor = self.line_start(),
            KeyCode::End => self.cursor = self.line_end(),
            KeyCode::Up => {
                if !self.move_vertical(false) {
                    self.history(true);
                }
            }
            KeyCode::Down => {
                if !self.move_vertical(true) {
                    self.history(false);
                }
            }
            KeyCode::Tab => self.insert("    "),
            _ => return Edit::Ignored,
        }
        Edit::Changed
    }

    fn delete_word_backward(&mut self) {
        let start = self.word_left();
        self.text.replace_range(start..self.cursor, "");
        self.cursor = start;
    }

    /// `\` then Enter inserts a newline on terminals that cannot report Shift+Enter (fx).
    pub fn backslash_newline(&mut self) -> bool {
        if self.cursor > 0 && self.text[..self.cursor].ends_with('\\') {
            self.text.replace_range(self.cursor - 1..self.cursor, "\n");
            return true;
        }
        false
    }

    /// Wrapped display lines and the cursor's (column, row) in them.
    pub fn layout(&self, width: usize) -> (Vec<String>, (u16, u16)) {
        let width = width.max(1);
        let mut lines = vec![String::new()];
        let mut col = 0usize;
        let mut cursor = (0u16, 0u16);
        for (index, c) in self.text.char_indices() {
            if index == self.cursor {
                cursor = (
                    u16::try_from(col).unwrap_or(u16::MAX),
                    u16::try_from(lines.len() - 1).unwrap_or(u16::MAX),
                );
            }
            if c == '\n' {
                lines.push(String::new());
                col = 0;
                continue;
            }
            let w = c.width().unwrap_or(0);
            if col + w > width {
                lines.push(String::new());
                col = 0;
            }
            if let Some(line) = lines.last_mut() {
                line.push(if c == '\t' { ' ' } else { c });
            }
            col += w;
        }
        if self.cursor >= self.text.len() {
            if col >= width {
                lines.push(String::new());
                col = 0;
            }
            cursor = (
                u16::try_from(col).unwrap_or(u16::MAX),
                u16::try_from(lines.len() - 1).unwrap_or(u16::MAX),
            );
        }
        (lines, cursor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(editor: &mut Editor, code: KeyCode, modifiers: KeyModifiers) {
        editor.key(KeyEvent::new(code, modifiers));
    }

    #[test]
    fn typing_words_and_kills() {
        let mut e = Editor::default();
        e.insert("hello wörld");
        press(&mut e, KeyCode::Char('w'), KeyModifiers::CONTROL);
        assert_eq!(e.text(), "hello ");
        e.insert("there");
        press(&mut e, KeyCode::Left, KeyModifiers::ALT);
        press(&mut e, KeyCode::Char('k'), KeyModifiers::CONTROL);
        assert_eq!(e.text(), "hello ");
        press(&mut e, KeyCode::Char('u'), KeyModifiers::CONTROL);
        assert!(e.is_empty());
    }

    #[test]
    fn lines_cursor_and_history() {
        let mut e = Editor::default();
        e.insert("ab\\");
        assert!(e.backslash_newline());
        e.insert("cd");
        assert_eq!(e.text(), "ab\ncd");
        assert_eq!(e.layout(10).1, (2, 1));
        press(&mut e, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(e.layout(10).1, (2, 0));
        let sent = e.take();
        assert_eq!(sent, "ab\ncd");
        press(&mut e, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(e.text(), "ab\ncd");
    }

    #[test]
    fn wide_characters_wrap_by_cells() {
        let mut e = Editor::default();
        e.insert("日本語テキスト");
        let (lines, cursor) = e.layout(6);
        assert_eq!(lines, vec!["日本語", "テキス", "ト"]);
        assert_eq!(cursor, (2, 2));
    }
}
