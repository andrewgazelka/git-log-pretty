//! Encoder for the [kitty terminal graphics protocol].
//!
//! This crate turns image bytes into the `APC _G ... ST` escape sequences that
//! kitty, [ghostty], and wezterm understand, and nothing else: it does not open
//! a terminal, decode images, or talk to the network. Callers own those
//! concerns and decide where the returned string is written.
//!
//! ```no_run
//! let png: &[u8] = b"...";
//! let seq = kitty::transmit(
//!     &kitty::Image::Png(png),
//!     None,
//!     &kitty::Placement { rows: Some(2), cols: Some(4), move_cursor: false },
//! );
//! print!("{seq}");
//! ```
//!
//! [kitty terminal graphics protocol]: https://sw.kovidgoyal.net/kitty/graphics-protocol/
//! [ghostty]: https://ghostty.org

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

/// Start of an Application Programming Command carrying a graphics command.
const APC_START: &str = "\x1b_G";
/// String Terminator (`ESC \`) that closes each command.
const ST: &str = "\x1b\\";
/// The protocol requires the base64 payload be split into chunks no larger than
/// this many bytes, each sent as its own command.
const MAX_CHUNK: usize = 4096;

/// Image data to transmit to the terminal.
#[derive(Debug, Clone, Copy)]
pub enum Image<'a> {
    /// PNG file bytes. The terminal decodes them, so no dimensions are needed
    /// (protocol format `f=100`).
    Png(&'a [u8]),
    /// Raw 8-bit RGBA pixels, exactly `width * height * 4` bytes, row-major
    /// (protocol format `f=32`).
    Rgba {
        width: u32,
        height: u32,
        pixels: &'a [u8],
    },
}

/// How the image occupies the terminal cell grid when displayed.
#[derive(Debug, Clone, Copy)]
pub struct Placement {
    /// Columns to scale the image across (`c=`). `None` lets the terminal pick.
    pub cols: Option<u32>,
    /// Rows to scale the image across (`r=`). `None` lets the terminal pick.
    pub rows: Option<u32>,
    /// Whether the cursor advances past the image after display.
    ///
    /// When `false`, the command sets `C=1` (do not move cursor) so the caller
    /// can position the cursor and lay out text around the image itself.
    pub move_cursor: bool,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            cols: None,
            rows: None,
            move_cursor: true,
        }
    }
}

/// Best-effort detection of whether the current terminal speaks the protocol.
///
/// This reads environment variables only; it never queries the terminal. A
/// `true` result is a strong hint, not a guarantee, so callers should still
/// offer an opt-out.
pub fn is_supported() -> bool {
    if std::env::var_os("KITTY_WINDOW_ID").is_some() {
        return true;
    }
    let advertises = |v: &str| {
        let v = v.to_ascii_lowercase();
        v.contains("kitty") || v.contains("ghostty") || v.contains("wezterm")
    };
    std::env::var("TERM").is_ok_and(|t| advertises(&t))
        || std::env::var("TERM_PROGRAM").is_ok_and(|t| advertises(&t))
}

/// Transmit `image` and display it at the cursor.
///
/// When `id` is `Some`, the terminal stores the image under that id so the same
/// pixels can be redrawn later with [`place`] instead of resending them.
pub fn transmit(image: &Image, id: Option<u32>, placement: &Placement) -> String {
    let (format, payload, dims): (u16, &[u8], Option<(u32, u32)>) = match *image {
        Image::Png(bytes) => (100, bytes, None),
        Image::Rgba {
            width,
            height,
            pixels,
        } => (32, pixels, Some((width, height))),
    };

    let mut control = format!("a=T,f={format},q=2");
    if let Some((w, h)) = dims {
        control.push_str(&format!(",s={w},v={h}"));
    }
    if let Some(id) = id {
        control.push_str(&format!(",i={id}"));
    }
    push_placement(&mut control, placement);

    encode_chunks(&control, payload)
}

/// Display an image previously sent by [`transmit`] with the same `id`, at the
/// cursor. Sends no pixels, so it is cheap to repeat.
pub fn place(id: u32, placement: &Placement) -> String {
    let mut control = format!("a=p,i={id},q=2");
    push_placement(&mut control, placement);
    format!("{APC_START}{control}{ST}")
}

fn push_placement(control: &mut String, placement: &Placement) {
    if let Some(c) = placement.cols {
        control.push_str(&format!(",c={c}"));
    }
    if let Some(r) = placement.rows {
        control.push_str(&format!(",r={r}"));
    }
    if !placement.move_cursor {
        // C=1 tells the terminal to leave the cursor where it was.
        control.push_str(",C=1");
    }
}

/// Base64-encode `payload` and frame it as one or more graphics commands,
/// splitting into `m=1` continuation chunks when it exceeds [`MAX_CHUNK`].
fn encode_chunks(control: &str, payload: &[u8]) -> String {
    let encoded = STANDARD.encode(payload);
    let bytes = encoded.as_bytes();

    if bytes.len() <= MAX_CHUNK {
        return format!("{APC_START}{control},m=0;{encoded}{ST}");
    }

    let chunks: Vec<&[u8]> = bytes.chunks(MAX_CHUNK).collect();
    let last = chunks.len() - 1;
    let mut out = String::with_capacity(encoded.len() + chunks.len() * (APC_START.len() + 16));
    for (i, chunk) in chunks.iter().enumerate() {
        out.push_str(APC_START);
        if i == 0 {
            // First chunk carries the real control keys; the rest carry only `m`.
            out.push_str(control);
            out.push_str(",m=1;");
        } else if i == last {
            out.push_str("m=0;");
        } else {
            out.push_str("m=1;");
        }
        // `chunk` is a slice of valid base64 (ASCII), so this never fails.
        out.push_str(std::str::from_utf8(chunk).expect("base64 is ascii"));
        out.push_str(ST);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single_command(s: &str) -> (&str, &str) {
        let body = s
            .strip_prefix(APC_START)
            .and_then(|s| s.strip_suffix(ST))
            .expect("framed as one APC command");
        body.split_once(';').expect("control;payload")
    }

    #[test]
    fn png_single_chunk_has_control_and_payload() {
        let seq = transmit(&Image::Png(b"hello"), None, &Placement::default());
        let (control, payload) = single_command(&seq);
        assert!(control.contains("a=T"));
        assert!(control.contains("f=100"));
        assert!(control.contains("q=2"));
        assert!(control.contains("m=0"));
        assert_eq!(payload, STANDARD.encode(b"hello"));
    }

    #[test]
    fn rgba_carries_dimensions() {
        let px = [0u8; 16]; // 2x2 RGBA
        let seq = transmit(
            &Image::Rgba {
                width: 2,
                height: 2,
                pixels: &px,
            },
            Some(7),
            &Placement {
                cols: Some(4),
                rows: Some(2),
                move_cursor: false,
            },
        );
        let (control, _) = single_command(&seq);
        assert!(control.contains("f=32"));
        assert!(control.contains("s=2"));
        assert!(control.contains("v=2"));
        assert!(control.contains("i=7"));
        assert!(control.contains("c=4"));
        assert!(control.contains("r=2"));
        assert!(control.contains("C=1"));
    }

    #[test]
    fn large_payload_is_chunked() {
        // 9 KiB of raw bytes -> ~12 KiB base64 -> 3 chunks of <=4096.
        let big = vec![0xABu8; 9 * 1024];
        let seq = transmit(&Image::Png(&big), None, &Placement::default());
        let commands: Vec<&str> = seq.split(ST).filter(|s| !s.is_empty()).collect();
        assert!(commands.len() >= 3, "expected multiple chunks");
        // First chunk has the control keys and m=1.
        assert!(commands[0].contains("a=T"));
        assert!(commands[0].contains("m=1"));
        // Last chunk closes with m=0 and carries no other control keys.
        let last = commands.last().unwrap();
        assert!(last.contains("m=0"));
        assert!(!last.contains("a=T"));
        // No base64 chunk exceeds the protocol limit.
        for cmd in &commands {
            let payload = cmd.rsplit_once(';').map(|(_, p)| p).unwrap_or("");
            assert!(payload.len() <= MAX_CHUNK);
        }
    }

    #[test]
    fn place_references_id_without_payload() {
        let seq = place(7, &Placement {
            cols: Some(4),
            rows: Some(2),
            move_cursor: false,
        });
        assert!(seq.starts_with(APC_START));
        assert!(seq.ends_with(ST));
        assert!(seq.contains("a=p"));
        assert!(seq.contains("i=7"));
        assert!(!seq.contains(';'), "placement of an existing image sends no payload");
    }
}
