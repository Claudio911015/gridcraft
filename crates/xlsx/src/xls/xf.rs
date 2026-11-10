//! XF record ([MS-XLS] 2.4.353) → `gridcraft_model::Style`.
//!
//! An XF is 20 bytes: `ifnt` and `ifmt` (4 bytes), then 16 bytes of packed fields. The
//! CellXF (2.5.20) and StyleXF (2.5.249) share this layout; only cell XFs are read here.

use gridcraft_model::{Alignment, BorderLine, BorderStyle, Borders, Fill, Font, HAlign, NumFmt, PatternType, Style, VAlign};

use super::{palette::Palette, records::Cursor};
use crate::IoError;

/// An XF record as stored in the file, before conversion to the model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Xf {
    pub ifnt: u16,
    pub ifmt: u16,
    pub alc: u8,
    pub wrap: bool,
    pub alc_v: u8,
    pub trot: u8,
    pub indent: u8,
    pub shrink: bool,
    /// left, right, top, bottom
    pub dg: [u8; 4],
    pub icv: [u16; 4],
    pub grbit_diag: u8,
    pub dg_diag: u8,
    pub icv_diag: u16,
    pub fls: u8,
    pub icv_fore: u16,
    pub icv_back: u16,
}

impl Default for Xf {
    fn default() -> Self {
        Xf {
            ifnt: 0,
            ifmt: 0,
            alc: 0,
            wrap: false,
            alc_v: 2,
            trot: 0,
            indent: 0,
            shrink: false,
            dg: [0; 4],
            icv: [0; 4],
            grbit_diag: 0,
            dg_diag: 0,
            icv_diag: 0,
            fls: 0,
            icv_fore: 0x40,
            icv_back: 0x41,
        }
    }
}

/// Extracts `width` bits starting at bit `shift` of a packed field.
fn bits(v: u32, shift: u32, width: u32) -> u32 {
    (v >> shift) & ((1 << width) - 1)
}

/// Reads an XF body. A body that ends right after `ifmt` (or inside the packed fields) is
/// accepted with the missing fields at their defaults; a body too short for `ifnt`/`ifmt` is an error.
pub(super) fn read_xf(c: &mut Cursor<'_, '_>) -> Result<Xf, IoError> {
    let ifnt = c.u16()?;
    let ifmt = c.u16()?;
    if c.remaining() < 16 {
        return Ok(Xf { ifnt, ifmt, ..Xf::default() });
    }
    // Offset 4: protection and style flags. Not imported yet.
    c.skip(2)?;
    // Offset 6: alc (bits 0-2), fWrap (bit 3), alcV (bits 4-6).
    let align = c.u8()?;
    // Offset 7: textRotation.
    let trot = c.u8()?;
    // Offset 8: cIndent (bits 0-3), fShrinkToFit (bit 4).
    let indent = c.u8()?;
    // Offset 9: fAtr* flags. Not imported.
    c.skip(1)?;
    // Offset 10: borders, part 1.
    let b10 = c.u32()?;
    // Offset 14: borders part 2, diagonal, fill pattern.
    let b14 = c.u32()?;
    // Offset 18: fill colours.
    let b18 = c.u16()?;

    // Each value is masked to its field width, so the narrowing casts cannot truncate.
    Ok(Xf {
        ifnt,
        ifmt,
        alc: align & 0x07,
        wrap: align & 0x08 != 0,
        alc_v: (align >> 4) & 0x07,
        trot,
        indent: indent & 0x0F,
        shrink: indent & 0x10 != 0,
        // left, right, top, bottom
        dg: [bits(b10, 0, 4) as u8, bits(b10, 4, 4) as u8, bits(b10, 8, 4) as u8, bits(b10, 12, 4) as u8],
        icv: [bits(b10, 16, 7) as u16, bits(b10, 23, 7) as u16, bits(b14, 0, 7) as u16, bits(b14, 7, 7) as u16],
        grbit_diag: bits(b10, 30, 2) as u8,
        dg_diag: bits(b14, 21, 4) as u8,
        icv_diag: bits(b14, 14, 7) as u16,
        fls: bits(b14, 26, 6) as u8,
        icv_fore: bits(u32::from(b18), 0, 7) as u16,
        icv_back: bits(u32::from(b18), 7, 7) as u16,
    })
}

/// Border style for a BorderLineStyle value (2.5.12); unknown values mean no border.
fn border_style(dg: u8) -> BorderStyle {
    match dg {
        1 => BorderStyle::Thin,
        2 => BorderStyle::Medium,
        3 => BorderStyle::Dashed,
        4 => BorderStyle::Dotted,
        5 => BorderStyle::Thick,
        6 => BorderStyle::Double,
        7 => BorderStyle::Hair,
        8 => BorderStyle::MediumDashed,
        9 => BorderStyle::DashDot,
        10 => BorderStyle::MediumDashDot,
        11 => BorderStyle::DashDotDot,
        12 => BorderStyle::MediumDashDotDot,
        13 => BorderStyle::SlantDashDot,
        _ => BorderStyle::None,
    }
}

/// Fill pattern for an fls value (2.5.126 FillPattern); unknown values mean no fill.
fn fill_pattern(fls: u8) -> PatternType {
    match fls {
        1 => PatternType::Solid,
        2 => PatternType::Gray50,
        3 => PatternType::Gray75,
        4 => PatternType::Gray25,
        5 => PatternType::DarkHorizontal,
        6 => PatternType::DarkVertical,
        7 => PatternType::DarkDown,
        8 => PatternType::DarkUp,
        9 => PatternType::DarkGrid,
        10 => PatternType::DarkTrellis,
        11 => PatternType::LightHorizontal,
        12 => PatternType::LightVertical,
        13 => PatternType::LightDown,
        14 => PatternType::LightUp,
        15 => PatternType::LightGrid,
        16 => PatternType::LightTrellis,
        17 => PatternType::Gray125,
        18 => PatternType::Gray0625,
        _ => PatternType::None,
    }
}

impl Xf {
    pub(super) fn to_style(&self, font: Font, num_fmt: NumFmt, palette: &Palette) -> Style {
        let h = match self.alc {
            1 => HAlign::Left,
            2 => HAlign::Center,
            3 => HAlign::Right,
            4 => HAlign::Fill,
            5 => HAlign::Justify,
            6 => HAlign::CenterAcross,
            7 => HAlign::Distributed,
            _ => HAlign::General,
        };
        let v = match self.alc_v {
            0 => VAlign::Top,
            1 => VAlign::Center,
            3 => VAlign::Justify,
            4 => VAlign::Distributed,
            _ => VAlign::Bottom,
        };
        let rotation = match self.trot {
            0..=90 => i16::from(self.trot),
            91..=180 => 90 - i16::from(self.trot),
            255 => 255,
            _ => 0,
        };
        let align = Alignment { h, v, wrap: self.wrap, shrink: self.shrink, indent: self.indent, rotation };

        let line = |dg: u8, icv: u16| -> BorderLine {
            let style = border_style(dg);
            if style == BorderStyle::None { BorderLine::default() } else { BorderLine { style, color: palette.color(icv) } }
        };
        let diag = |bit: u8| -> BorderLine { if self.grbit_diag & bit != 0 { line(self.dg_diag, self.icv_diag) } else { BorderLine::default() } };
        let borders = Borders {
            left: line(self.dg[0], self.icv[0]),
            right: line(self.dg[1], self.icv[1]),
            top: line(self.dg[2], self.icv[2]),
            bottom: line(self.dg[3], self.icv[3]),
            diag_down: diag(1),
            diag_up: diag(2),
        };

        let pattern = fill_pattern(self.fls);
        let fill = if pattern == PatternType::None {
            Fill::default()
        } else {
            Fill { pattern, fg: palette.color(self.icv_fore), bg: palette.color(self.icv_back) }
        };

        Style { font, fill, borders, align, num_fmt, protection: Default::default() }
    }
}

#[cfg(test)]
mod tests {
    use gridcraft_model::{Color, Fill, Font, NumFmt, Style};

    use super::super::palette::Palette;
    use super::super::records::Cursor;
    use super::{Xf, read_xf};

    /// A 20-byte XF body. Packed fields are passed as literal u32/u16 values.
    fn xf_bytes(ifnt: u16, ifmt: u16, b6: u8, trot: u8, b8: u8, b10: u32, b14: u32, b18: u16) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend(ifnt.to_le_bytes());
        v.extend(ifmt.to_le_bytes());
        v.extend([0u8, 0]); // flags at offset 4
        v.push(b6);
        v.push(trot);
        v.push(b8);
        v.push(0); // fAtr at offset 9
        v.extend(b10.to_le_bytes());
        v.extend(b14.to_le_bytes());
        v.extend(b18.to_le_bytes());
        v
    }

    /// Zeroed XF with alcV = 2 (bottom), as Excel writes it.
    fn plain() -> Vec<u8> {
        xf_bytes(0, 0, 2 << 4, 0, 0, 0, 0, 0)
    }

    fn parse(body: &[u8]) -> Result<Xf, crate::IoError> {
        let parts = [body];
        let mut c = Cursor::new(&parts);
        read_xf(&mut c)
    }

    fn style_of(body: &[u8]) -> Style {
        parse(body).unwrap().to_style(Font::default(), NumFmt::default(), &Palette::default())
    }

    #[test]
    fn zeroed_xf_is_the_default_style() {
        assert_eq!(style_of(&plain()), Style::default());
    }

    #[test]
    fn alignment_fields_and_rotation() {
        // alc=2 (centre), fWrap, alcV=1 (centre), trot=135, cIndent=3
        let s = style_of(&xf_bytes(0, 0, 2 | 0x08 | (1 << 4), 135, 3, 0, 0, 0));
        assert_eq!(s.align.h, gridcraft_model::HAlign::Center);
        assert_eq!(s.align.v, gridcraft_model::VAlign::Center);
        assert!(s.align.wrap);
        assert!(!s.align.shrink);
        assert_eq!(s.align.indent, 3);
        assert_eq!(s.align.rotation, -45);

        // Shrink-to-fit is bit 4 of byte 8.
        let s = style_of(&xf_bytes(0, 0, 2 << 4, 0, 3 | 0x10, 0, 0, 0));
        assert!(s.align.shrink);
        assert_eq!(s.align.indent, 3);

        // 255 is "stacked text", kept as-is; 200 is out of range and falls back to 0.
        assert_eq!(style_of(&xf_bytes(0, 0, 2 << 4, 255, 0, 0, 0, 0)).align.rotation, 255);
        assert_eq!(style_of(&xf_bytes(0, 0, 2 << 4, 200, 0, 0, 0, 0)).align.rotation, 0);
    }

    #[test]
    fn solid_and_pattern_fills() {
        // fls=1 (solid), icvFore=13 (yellow), icvBack=0x41 (system background → Auto)
        let s = style_of(&xf_bytes(0, 0, 2 << 4, 0, 0, 0, 1 << 26, 13 | (0x41 << 7)));
        assert_eq!(s.fill.pattern, gridcraft_model::PatternType::Solid);
        assert_eq!(s.fill.fg, Color::Rgb(0xFFFF00));
        assert_eq!(s.fill.bg, Color::Auto);

        // fls=17 is Gray125.
        let s = style_of(&xf_bytes(0, 0, 2 << 4, 0, 0, 0, 17 << 26, 13 | (0x41 << 7)));
        assert_eq!(s.fill.pattern, gridcraft_model::PatternType::Gray125);
    }

    #[test]
    fn borders_each_side_and_diagonals_distinct() {
        // left: Thin (1) black (icv 8); right: Dashed (3) green (11); top: Medium (2) white (9);
        // bottom: Double (6) red (10). Diagonal: Hair (7) blue (12), both directions (grbitDiag=3).
        // icvTop=9 sits in the second field, dgTop=2 in the first.
        let b10 = 1 | (3 << 4) | (2 << 8) | (6 << 12) | (8 << 16) | (11 << 23) | (3 << 30);
        let b14 = 9 | (10 << 7) | (12 << 14) | (7 << 21);
        let s = style_of(&xf_bytes(0, 0, 2 << 4, 0, 0, b10, b14, 0));
        let b = s.borders;
        assert_eq!(b.left.style, gridcraft_model::BorderStyle::Thin);
        assert_eq!(b.left.color, Color::Rgb(0x000000));
        assert_eq!(b.right.style, gridcraft_model::BorderStyle::Dashed);
        assert_eq!(b.right.color, Color::Rgb(0x00FF00));
        assert_eq!(b.top.style, gridcraft_model::BorderStyle::Medium);
        assert_eq!(b.top.color, Color::Rgb(0xFFFFFF));
        assert_eq!(b.bottom.style, gridcraft_model::BorderStyle::Double);
        assert_eq!(b.bottom.color, Color::Rgb(0xFF0000));
        assert_eq!(b.diag_down.style, gridcraft_model::BorderStyle::Hair);
        assert_eq!(b.diag_down.color, Color::Rgb(0x0000FF));
        assert_eq!(b.diag_up.style, gridcraft_model::BorderStyle::Hair);
        assert_eq!(b.diag_up.color, Color::Rgb(0x0000FF));

        // grbitDiag=1 only: diag_down is set, diag_up stays empty.
        let s = style_of(&xf_bytes(0, 0, 2 << 4, 0, 0, (b10 & !(3 << 30)) | (1 << 30), b14, 0));
        assert_eq!(s.borders.diag_down.style, gridcraft_model::BorderStyle::Hair);
        assert_eq!(s.borders.diag_up, gridcraft_model::BorderLine::default());
    }

    #[test]
    fn ifnt_and_ifmt_are_read_from_the_first_four_bytes() {
        let x = parse(&xf_bytes(5, 164, 2 << 4, 0, 0, 0, 0, 0)).unwrap();
        assert_eq!((x.ifnt, x.ifmt), (5, 164));
        assert_eq!(x.alc_v, 2);
    }

    #[test]
    fn hostile_bodies_never_panic() {
        // Only ifnt and ifmt: accepted, the rest is default.
        assert_eq!(parse(&[3, 0, 4, 0]).unwrap(), Xf { ifnt: 3, ifmt: 4, ..Xf::default() });
        // Not even ifmt: an error.
        assert!(parse(&[1, 0]).is_err());

        // Every byte 0xFF: alc=7 (distributed), alcV=7 (unknown → bottom), trot=255,
        // cIndent=15 with shrink, every border and fill code out of range → no border, no fill.
        let s = style_of(&xf_bytes(0xFFFF, 0xFFFF, 0xFF, 0xFF, 0xFF, u32::MAX, u32::MAX, u16::MAX));
        assert_eq!(s.align.h, gridcraft_model::HAlign::Distributed);
        assert_eq!(s.align.v, gridcraft_model::VAlign::Bottom);
        assert!(s.align.wrap && s.align.shrink);
        assert_eq!(s.align.indent, 15);
        assert_eq!(s.align.rotation, 255);
        assert_eq!(s.borders, Default::default());
        assert_eq!(s.fill, Fill::default());
    }
}
