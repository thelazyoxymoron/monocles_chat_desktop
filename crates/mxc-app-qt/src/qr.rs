//! QR encoding for the verification code, flattened into a string QML can draw.
//!
//! QML gets `"<size>:<0/1 per module, row-major>"` and paints it on a Canvas, so the app needs
//! neither an image codec nor the SVG plugin to show a scannable code.

/// Encode `text`, or return an empty string when it is empty or too long to encode.
pub fn matrix(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    // Medium error correction: still readable when a little of the code is obscured or the
    // screen is dirty, without the density of the higher levels.
    let Ok(code) = qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::M)
    else {
        return String::new();
    };
    let width = code.width();
    let colors = code.to_colors();
    let mut out = String::with_capacity(colors.len() + 8);
    out.push_str(&width.to_string());
    out.push(':');
    for c in colors {
        out.push(if c == qrcode::Color::Dark { '1' } else { '0' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_a_verification_uri() {
        let uri = format!(
            "xmpp:me@example.com?omemo-sid-1234={}",
            "aa".repeat(32) // a 64-char fingerprint
        );
        let m = matrix(&uri);
        let (size, bits) = m.split_once(':').expect("size prefix");
        let size: usize = size.parse().expect("numeric size");
        assert_eq!(bits.len(), size * size, "row-major matrix of size²");
        assert!(bits.chars().all(|c| c == '0' || c == '1'));
        // The top-left finder pattern is a 7x7 dark border — the first row starts dark.
        assert!(bits.starts_with("1111111"));
    }

    #[test]
    fn empty_input_yields_no_code() {
        assert!(matrix("").is_empty());
    }
}
