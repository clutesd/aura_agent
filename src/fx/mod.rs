pub mod atmosphere;
pub mod celestial;
pub mod easing;
pub mod globe;
pub mod noise;
pub mod orb;
pub mod spectral;
pub mod starfield;
pub mod synapse;
pub mod weather;

/// Classify a WMO weather code into boolean flags.
/// Returns (is_clear, is_cloudy, is_fog, is_rain, is_snow, is_storm).
pub fn classify_weather(wc: u16) -> (bool, bool, bool, bool, bool, bool) {
    let is_clear = wc <= 1;
    let is_cloudy = wc == 2 || wc == 3;
    let is_fog = wc == 45 || wc == 48;
    let is_rain = (51..=67).contains(&wc) || (80..=82).contains(&wc);
    let is_snow = (71..=77).contains(&wc) || (85..=86).contains(&wc);
    let is_storm = wc >= 95;
    (is_clear, is_cloudy, is_fog, is_rain, is_snow, is_storm)
}

/// Quick frame-unique hash from time (for particle recycling without allocations).
pub fn grain_frame_hash(t: f32) -> u32 {
    let bits = (t * 10000.0) as u32;
    bits.wrapping_mul(2654435761)
}

/// Replace Unicode punctuation with ASCII equivalents so Raylib's
/// default font can render every character.
#[allow(dead_code)] // Used in dual-phase streaming pipeline (Phase 2)
pub fn sanitize_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\u{2018}' | '\u{2019}' | '\u{02BC}' | '\u{FF07}' => out.push('\''),
            '\u{201C}' | '\u{201D}' | '\u{FF02}' => out.push('"'),
            '\u{2013}' | '\u{2014}' => out.push_str("--"),
            '\u{2026}' => out.push_str("..."),
            '\u{00A0}' | '\u{2002}' | '\u{2003}' | '\u{2009}' => out.push(' '),
            '\u{00B7}' | '\u{2022}' | '\u{2023}' => out.push('*'),
            '\u{2190}'..='\u{21FF}' => out.push_str("->"),
            '\u{00E9}' => out.push('e'),
            '\u{00E0}' => out.push('a'),
            '\u{FEFF}' => {} // BOM — discard
            c if c.is_ascii() => out.push(c),
            c if c.is_alphanumeric() => {
                // Non-ASCII alphanumeric — keep as-is, Raylib may render
                out.push(c);
            }
            _ => out.push(' '), // unknown symbol — replace with space
        }
    }
    out
}

/// Word-wrap text to fit within `max_chars` per line, breaking at word boundaries.
pub fn wrap_lines(text: &str, max_chars: usize) -> Vec<String> {
    if text.chars().count() <= max_chars {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut current_line = String::new();
    for word in text.split_whitespace() {
        if current_line.is_empty() {
            // First word on the line — take it even if it's too long
            if word.chars().count() > max_chars {
                // Hard-break a very long word
                let mut chars = word.chars();
                while chars.clone().count() > 0 {
                    let chunk: String = chars.by_ref().take(max_chars).collect();
                    if chunk.is_empty() {
                        break;
                    }
                    lines.push(chunk);
                }
            } else {
                current_line = word.to_string();
            }
        } else if current_line.chars().count() + 1 + word.chars().count() <= max_chars {
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            lines.push(current_line);
            current_line = word.to_string();
        }
    }
    if !current_line.is_empty() {
        lines.push(current_line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}
