use crossterm::style::Color;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub fn hash_to_background_color(text: &str, is_dark_theme: bool) -> Color {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    let hash = hasher.finish();

    // Convert hash to hue (0-360 degrees)
    let hue = (hash % 360) as f32;

    // Adjust saturation and brightness based on theme for better readability
    let (saturation, brightness) = if is_dark_theme {
        // For dark backgrounds: more muted colors to ensure white text is readable
        (0.5, 0.5)
    } else {
        // For light backgrounds: more vibrant colors work better
        (0.7, 0.8)
    };

    let (r, g, b) = hsv_to_rgb(hue, saturation, brightness);

    Color::Rgb { r, g, b }
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r_prime, g_prime, b_prime) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    let r = ((r_prime + m) * 255.0) as u8;
    let g = ((g_prime + m) * 255.0) as u8;
    let b = ((b_prime + m) * 255.0) as u8;

    (r, g, b)
}

pub fn hex_to_rgb(hex: &str) -> Color {
    // Remove # if present and parse hex color to RGB
    let hex = hex.trim_start_matches('#');

    if hex.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ) {
            return Color::Rgb { r, g, b };
        }
    }

    // Fallback to white if parsing fails
    Color::White
}
