//! Render a commit block, optionally with the author's avatar drawn in the
//! left gutter via the kitty graphics protocol.

use std::collections::HashSet;
use std::io::Write;

use crate::avatar::Avatar;

/// Print one commit. `lines` are the already-formatted text rows (header plus
/// file tree). When `avatar` is `Some` and `rows > 0`, the avatar is drawn at
/// the line start and the text is shifted right to clear it; otherwise the text
/// is printed plainly.
///
/// `transmitted` tracks which avatar ids have already been sent this run so a
/// repeated author is redrawn with a cheap placement instead of resending the
/// pixels.
pub fn render_commit(
    lines: &[String],
    avatar: Option<&Avatar>,
    transmitted: &mut HashSet<u32>,
    rows: u32,
) {
    match avatar {
        Some(av) if rows > 0 => {
            // Cells are about twice as tall as wide, so double the column count
            // to keep the avatar square. One extra column separates it from text.
            let cols = rows * 2;
            let gutter = cols + 1;
            let placement = kitty::Placement {
                cols: Some(cols),
                rows: Some(rows),
                move_cursor: false,
            };

            let mut out = String::new();
            // Anchor at column 0, then draw without moving the cursor (C=1).
            out.push('\r');
            if transmitted.insert(av.id) {
                out.push_str(&kitty::transmit(
                    &kitty::Image::Png(&av.png),
                    Some(av.id),
                    &placement,
                ));
            } else {
                out.push_str(&kitty::place(av.id, &placement));
            }

            // Print at least `rows` lines so the text advances past the image.
            let count = lines.len().max(rows as usize);
            for i in 0..count {
                let content = lines.get(i).map(String::as_str).unwrap_or("");
                // Return to column 0, step right past the avatar, then the text.
                out.push_str(&format!("\r\x1b[{gutter}C{content}\n"));
            }
            out.push('\n');

            print!("{out}");
            let _ = std::io::stdout().flush();
        }
        _ => {
            for line in lines {
                println!("{line}");
            }
            println!();
        }
    }
}
