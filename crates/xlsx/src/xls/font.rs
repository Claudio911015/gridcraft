//! FONT record ([MS-XLS] 2.4.122) → `gridcraft_model::Font`.

use gridcraft_model::{Font, Underline, VertAlign};

use super::{palette::Palette, records::Cursor};
use crate::IoError;

/// Bold starts at this weight (`bls`); anything lighter is treated as regular.
const BOLD_WEIGHT: u16 = 600;
/// `grbit` flags: italic and strikeout.
const GRBIT_ITALIC: u16 = 0x0002;
const GRBIT_STRIKE: u16 = 0x0008;
/// Font size used when the record gives no height (0 twips).
const FALLBACK_SIZE: f32 = 10.0;
/// Bounds of the font sizes Excel accepts, in points.
const MIN_SIZE: f32 = 1.0;
const MAX_SIZE: f32 = 409.0;
/// Name used when the record's name is empty.
const FALLBACK_NAME: &str = "Arial";

/// A FONT record as stored in the file, before conversion to the model.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct XlsFont {
    pub height: u16,
    pub grbit: u16,
    pub icv: u16,
    pub weight: u16,
    pub script: u16,
    pub underline: u8,
    pub name: String,
}

impl Default for XlsFont {
    fn default() -> Self {
        XlsFont { height: 200, grbit: 0, icv: 0x7FFF, weight: 400, script: 0, underline: 0, name: "Arial".to_string() }
    }
}

/// Reads a FONT record body. Any missing field is an error; the caller decides what to do with it.
pub(super) fn read_font(c: &mut Cursor<'_, '_>) -> Result<XlsFont, IoError> {
    let height = c.u16()?;
    let grbit = c.u16()?;
    let icv = c.u16()?;
    let weight = c.u16()?;
    let script = c.u16()?;
    let underline = c.u8()?;
    // bFamily and bCharSet are not needed for display; unused is reserved.
    c.skip(3)?;
    let name = c.short_string()?;
    Ok(XlsFont { height, grbit, icv, weight, script, underline, name })
}

/// FONT record for an XF's ifnt: Excel never writes font index 4, so ifnt ≥ 5 is record ifnt-1.
pub(super) fn font_at(fonts: &[XlsFont], ifnt: u16) -> Option<&XlsFont> {
    let index = match ifnt {
        4 => return None,
        n if n > 4 => usize::from(n - 1),
        n => usize::from(n),
    };
    fonts.get(index)
}

impl XlsFont {
    pub(super) fn to_model(&self, palette: &Palette) -> Font {
        let size = if self.height == 0 { FALLBACK_SIZE } else { (f32::from(self.height) / 20.0).clamp(MIN_SIZE, MAX_SIZE) };
        let name = if self.name.is_empty() { FALLBACK_NAME.to_string() } else { self.name.clone() };
        let underline = match self.underline {
            0x01 => Underline::Single,
            0x02 => Underline::Double,
            0x21 => Underline::SingleAccounting,
            0x22 => Underline::DoubleAccounting,
            _ => Underline::None,
        };
        let vert = match self.script {
            1 => VertAlign::Superscript,
            2 => VertAlign::Subscript,
            _ => VertAlign::Baseline,
        };
        Font {
            name,
            size,
            bold: self.weight >= BOLD_WEIGHT,
            italic: self.grbit & GRBIT_ITALIC != 0,
            underline,
            strike: self.grbit & GRBIT_STRIKE != 0,
            color: palette.color(self.icv),
            vert,
        }
    }
}

#[cfg(test)]
mod tests {
    use gridcraft_model::Color;

    use super::*;

    /// Builds a FONT record body: height, grbit, icv, weight, script, underline, family, charset, unused, name.
    fn font_bytes(height: u16, grbit: u16, icv: u16, weight: u16, script: u16, underline: u8, name: &str) -> Vec<u8> {
        let mut b = Vec::new();
        for v in [height, grbit, icv, weight, script] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.push(underline);
        b.extend_from_slice(&[0, 0, 0]);
        b.push(name.chars().count() as u8);
        b.push(0);
        b.extend(name.bytes());
        b
    }

    fn parse(bytes: &[u8]) -> Result<XlsFont, IoError> {
        let parts = [bytes];
        let mut c = Cursor::new(&parts);
        read_font(&mut c)
    }

    fn model(bytes: &[u8]) -> Font {
        parse(bytes).unwrap().to_model(&Palette::default())
    }

    #[test]
    fn bold_arial_12_with_palette_red_and_single_underline() {
        let f = model(&font_bytes(240, 0, 10, 700, 0, 1, "Arial"));
        assert_eq!(f.name, "Arial");
        assert_eq!(f.size, 12.0);
        assert!(f.bold);
        assert!(!f.italic);
        assert_eq!(f.underline, Underline::Single);
        assert_eq!(f.color, Color::Rgb(0xFF0000));
    }

    #[test]
    fn italic_strike_subscript_and_accounting_underline() {
        let f = model(&font_bytes(200, 0x000A, 0x7FFF, 400, 2, 0x22, "Arial"));
        assert!(f.italic);
        assert!(f.strike);
        assert_eq!(f.vert, VertAlign::Subscript);
        assert_eq!(f.underline, Underline::DoubleAccounting);
        assert_eq!(f.color, Color::Auto);
        assert!(!f.bold);
    }

    #[test]
    fn utf16_name_is_kept() {
        let mut bytes = Vec::new();
        for v in [200u16, 0, 0x7FFF, 400, 0] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes.push(5);
        bytes.push(1);
        for u in "Ärial".encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        let f = parse(&bytes).unwrap();
        assert_eq!(f.name, "Ärial");
    }

    #[test]
    fn font_at_skips_index_four() {
        let fonts: Vec<XlsFont> = (1..=6u16).map(|i| XlsFont { height: i * 100, ..XlsFont::default() }).collect();
        assert_eq!(font_at(&fonts, 3).map(|f| f.height), Some(400));
        assert!(font_at(&fonts, 4).is_none());
        assert_eq!(font_at(&fonts, 5).map(|f| f.height), Some(500));
        assert!(font_at(&fonts, 9999).is_none());
    }

    #[test]
    fn hostile_font_records_do_not_panic() {
        assert!(parse(&[1, 2, 3]).is_err());
        assert_eq!(model(&font_bytes(0, 0, 0x7FFF, 400, 0, 0, "Arial")).size, 10.0);
        assert_eq!(model(&font_bytes(0xFFFF, 0, 0x7FFF, 400, 0, 0, "Arial")).size, 409.0);
        // cch = 200 but only three characters follow.
        let mut bytes = font_bytes(200, 0, 0x7FFF, 400, 0, 0, "");
        let n = bytes.len();
        bytes[n - 2] = 200;
        bytes.extend_from_slice(b"abc");
        assert!(parse(&bytes).is_err());
    }
}
