//! Kitty graphics, in-band only (DESIGN.md 10): PNG data travels inside the escape sequences
//! in chunks of at most 4096 base64 bytes (`t=d`), never as file or shared-memory paths, so it
//! works over SSH, through Rex and herdr. Inside tmux, images use Unicode placeholders, which
//! survive tmux redraws.

use base64::Engine;
use ratatui::style::Color;

use crate::caps::passthrough;

const CHUNK: usize = 4096;

/// Row and column diacritics of the Kitty Unicode placeholder protocol
/// (`rowcolumn-diacritics.txt`), first 64 entries.
const DIACRITICS: [u32; 64] = [
    0x0305, 0x030D, 0x030E, 0x0310, 0x0312, 0x033D, 0x033E, 0x033F, 0x0346, 0x034A, 0x034B, 0x034C,
    0x0350, 0x0351, 0x0352, 0x0357, 0x035B, 0x0363, 0x0364, 0x0365, 0x0366, 0x0367, 0x0368, 0x0369,
    0x036A, 0x036B, 0x036C, 0x036D, 0x036E, 0x036F, 0x0483, 0x0484, 0x0485, 0x0486, 0x0487, 0x0592,
    0x0593, 0x0594, 0x0595, 0x0597, 0x0598, 0x0599, 0x059C, 0x059D, 0x059E, 0x059F, 0x05A0, 0x05A1,
    0x05A8, 0x05A9, 0x05AB, 0x05AC, 0x05AF, 0x05C4, 0x0610, 0x0611, 0x0612, 0x0613, 0x0614, 0x0615,
    0x0616, 0x0617, 0x0657, 0x0658,
];

/// Largest image size in cells (placeholders can address 64 rows and columns).
pub const MAX_CELLS: u16 = 64;

/// Width and height of a PNG from its IHDR, without decoding it.
pub fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    Some((width, height))
}

/// Cells an image of `width` x `height` pixels takes at most `max_cols` wide (cells are about
/// twice as tall as wide).
pub fn cells(width: u32, height: u32, max_cols: u16) -> (u16, u16) {
    let cols = u32::from(max_cols.clamp(1, MAX_CELLS)).min(width.div_ceil(10).max(1));
    let rows = (u64::from(cols) * u64::from(height) / u64::from(width.max(1)) / 2)
        .clamp(1, u64::from(MAX_CELLS));
    (
        u16::try_from(cols).unwrap_or(MAX_CELLS),
        u16::try_from(rows).unwrap_or(MAX_CELLS),
    )
}

fn wrap(sequence: String, tmux: bool) -> String {
    if tmux {
        passthrough(&sequence)
    } else {
        sequence
    }
}

/// Transmit PNG bytes as image `id` (kept by the terminal until deleted), in chunks.
pub fn transmit(id: u32, png: &[u8], tmux: bool) -> String {
    let data = base64::engine::general_purpose::STANDARD.encode(png);
    let chunks: Vec<&[u8]> = data.as_bytes().chunks(CHUNK).collect();
    let mut out = String::new();
    for (index, chunk) in chunks.iter().enumerate() {
        let more = u8::from(index + 1 < chunks.len());
        let text = String::from_utf8_lossy(chunk);
        let control = if index == 0 {
            format!("a=t,f=100,t=d,i={id},q=2,m={more}")
        } else {
            format!("m={more}")
        };
        out.push_str(&wrap(format!("\u{1b}_G{control};{text}\u{1b}\\"), tmux));
    }
    out
}

/// Place image `id` at a zero-based cell position, `cols` x `rows`, without moving the cursor.
pub fn place(id: u32, col: u16, row: u16, cols: u16, rows: u16) -> String {
    format!(
        "\u{1b}7\u{1b}[{};{}H\u{1b}_Ga=p,i={id},p=1,c={cols},r={rows},C=1,q=2\u{1b}\\\u{1b}8",
        row + 1,
        col + 1
    )
}

/// A virtual placement for Unicode placeholders.
pub fn virtual_place(id: u32, cols: u16, rows: u16, tmux: bool) -> String {
    wrap(
        format!("\u{1b}_Ga=p,U=1,i={id},p=1,c={cols},r={rows},q=2\u{1b}\\"),
        tmux,
    )
}

/// Remove every placement, keeping image data (an overlay hides images this way).
pub fn delete_placements(tmux: bool) -> String {
    wrap("\u{1b}_Ga=d,d=a,q=2\u{1b}\\".to_owned(), tmux)
}

/// Free image `id` (when the client forgets it).
pub fn free(id: u32, tmux: bool) -> String {
    wrap(format!("\u{1b}_Ga=d,d=I,i={id},q=2\u{1b}\\"), tmux)
}

/// The text of one placeholder cell and its foreground colour, which names the image.
pub fn placeholder_cell(id: u32, row: u16, col: u16) -> (String, Color) {
    let mark = |n: u16| {
        DIACRITICS
            .get(usize::from(n))
            .and_then(|c| char::from_u32(*c))
            .unwrap_or('\u{0305}')
    };
    let mut text = String::from('\u{10EEEE}');
    text.push(mark(row));
    text.push(mark(col));
    let color = if id < 256 {
        Color::Indexed(u8::try_from(id).unwrap_or(0))
    } else {
        let [_, r, g, b] = id.to_be_bytes();
        Color::Rgb(r, g, b)
    };
    (text, color)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG_1X1: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x02, 0x80, 0x00, 0x00, 0x01, 0x40, 0x08, 0x02, 0x00, 0x00, 0x00,
    ];

    #[test]
    fn reads_png_size() {
        assert_eq!(png_size(PNG_1X1), Some((640, 320)));
        assert_eq!(png_size(b"GIF89a"), None);
    }

    #[test]
    fn large_images_go_in_chunks_without_paths() {
        let png = vec![7u8; 9000];
        let out = transmit(5, &png, false);
        assert_eq!(out.matches("\u{1b}_G").count(), 3);
        assert!(out.starts_with("\u{1b}_Ga=t,f=100,t=d,i=5,q=2,m=1;"));
        assert!(out.contains("\u{1b}_Gm=0;"));
        assert!(!out.contains("t=f") && !out.contains("t=s"));
    }

    #[test]
    fn tmux_wraps_and_doubles_escapes() {
        let out = delete_placements(true);
        assert!(out.starts_with("\u{1b}Ptmux;\u{1b}\u{1b}_G"));
    }

    #[test]
    fn placeholder_encodes_row_column_and_id() {
        let (text, color) = placeholder_cell(7, 0, 2);
        assert_eq!(text, "\u{10EEEE}\u{0305}\u{030E}");
        assert_eq!(color, Color::Indexed(7));
        assert_eq!(cells(640, 320, 40), (40, 10));
    }
}
