//! Content is untrusted (ADR-0087 decision 3): entries, tool output and part text never reach
//! the terminal as raw bytes. Every escape sequence, C0 and C1 control and bidi override is
//! replaced by a visible stand-in, so clipboard writes (OSC 52), fake links (OSC 8), title
//! changes, Kitty graphics (APC) and cursor moves in content do nothing.

/// Safe text for one line or a block: newlines kept, tabs as spaces, everything else
/// visible.
pub fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\n' => out.push('\n'),
            '\t' => out.push_str("    "),
            '\r' => {
                if chars.peek() != Some(&'\n') {
                    out.push('\n');
                }
            }
            '\u{1b}' => out.push('\u{241b}'),
            c if (c as u32) < 0x20 || c == '\u{7f}' => out.push(control_picture(c)),
            '\u{80}'..='\u{9f}' => out.push_str(&format!("<U+{:04X}>", c as u32)),
            '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' => {
                out.push_str(&format!("<U+{:04X}>", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// One line: newlines become visible too.
pub fn clean_line(text: &str) -> String {
    clean(text).replace('\n', "\u{2424}")
}

fn control_picture(c: char) -> char {
    if c == '\u{7f}' {
        return '\u{2421}';
    }
    char::from_u32(0x2400 + c as u32).unwrap_or('?')
}

/// Strip SGR-free ANSI from tool output while keeping the text readable: CSI and OSC
/// sequences are dropped as a whole (their payload is not text), other escapes become
/// visible. Used for tool output, where colour codes are common and harmless to drop.
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('[') => {
                chars.next();
                // CSI: parameters and intermediates, then one final byte 0x40..=0x7e.
                for next in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&next) {
                        break;
                    }
                }
            }
            Some(']' | 'P' | '_' | '^' | 'X') => {
                chars.next();
                // String sequences end with BEL or ST (ESC \).
                while let Some(next) = chars.next() {
                    if next == '\u{7}' {
                        break;
                    }
                    if next == '\u{1b}' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            _ => out.push('\u{1b}'),
        }
    }
    clean(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn safe(text: &str) -> bool {
        !text.chars().any(|c| {
            (c as u32) < 0x20 && c != '\n' || c == '\u{7f}' || ('\u{80}'..='\u{9f}').contains(&c)
        })
    }

    #[test]
    fn injections_render_visibly_and_never_raw() {
        let attacks = [
            "\u{1b}]52;c;aGVsbG8=\u{7}",
            "\u{1b}]8;;https://evil.example\u{1b}\\click\u{1b}]8;;\u{1b}\\",
            "\u{1b}_Ga=T,f=100;AAAA\u{1b}\\",
            "\u{1b}]0;fake title\u{7}",
            "\u{1b}[2J\u{1b}[H",
            "\u{9b}31m",
            "abc\u{202e}fed",
            "\u{1b}]7501;state=done\u{1b}\\",
        ];
        for attack in attacks {
            let cleaned = clean(attack);
            assert!(safe(&cleaned), "{attack:?} -> {cleaned:?}");
            assert!(!cleaned.contains('\u{1b}'));
            let stripped = strip_ansi(attack);
            assert!(safe(&stripped), "{attack:?} -> {stripped:?}");
        }
        assert_eq!(clean("abc\u{202e}fed"), "abc<U+202E>fed");
        assert_eq!(clean("a\u{1b}[31mb"), "a\u{241b}[31mb");
    }

    #[test]
    fn colour_codes_drop_from_tool_output() {
        assert_eq!(strip_ansi("\u{1b}[31mred\u{1b}[0m ok"), "red ok");
        assert_eq!(
            strip_ansi("a\u{1b}]8;;http://x\u{1b}\\link\u{1b}]8;;\u{1b}\\"),
            "alink"
        );
    }
}
