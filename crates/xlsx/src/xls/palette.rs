//! PALETTE record ([MS-XLS] 2.4.188) and colour indexes (icv, [MS-XLS] 2.5.161).

use gridcraft_model::Color;

use super::records::Cursor;
use crate::tables::INDEXED;

/// Number of indexes a PALETTE record may replace (8..=63).
const REPLACEABLE_FIRST: usize = 8;
const REPLACEABLE_COUNT: usize = 56;

/// The workbook's 64 indexed colours: 0-7 fixed, 8-63 replaceable by a PALETTE record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Palette([u32; 64]);

impl Default for Palette {
    fn default() -> Self {
        Palette(INDEXED)
    }
}

impl Palette {
    /// Reads a PALETTE record body. Never fails: reads min(ccv, 56, entries present) LongRGB
    /// entries (r, g, b, reserved) into slots 8.. and keeps the defaults for the rest.
    pub(super) fn read(c: &mut Cursor<'_, '_>) -> Palette {
        let mut palette = Palette::default();
        let Ok(ccv) = c.u16() else {
            return palette;
        };
        // Each LongRGB is 4 bytes; the file's count is untrusted, so bound it by what is present.
        let count = usize::from(ccv).min(REPLACEABLE_COUNT).min(c.remaining() / 4);
        for entry in palette.0.iter_mut().skip(REPLACEABLE_FIRST).take(count) {
            let (Ok(r), Ok(g), Ok(b), Ok(_reserved)) = (c.u8(), c.u8(), c.u8(), c.u8()) else {
                break;
            };
            *entry = (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
        }
        palette
    }

    /// Colour for an icv: 0..=63 → Rgb from the table; anything else (0x40 system text,
    /// 0x41 system background, 0x51, 0x7FFF automatic, out of range) → Color::Auto.
    pub(super) fn color(&self, icv: u16) -> Color {
        match self.0.get(usize::from(icv)) {
            Some(&rgb) => Color::Rgb(rgb),
            None => Color::Auto,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette_from(bytes: &[u8]) -> Palette {
        let parts = [bytes];
        let mut c = Cursor::new(&parts);
        Palette::read(&mut c)
    }

    #[test]
    fn default_palette_matches_biff8_table() {
        let p = Palette::default();
        assert_eq!(p.color(2), Color::Rgb(0xFF0000));
        assert_eq!(p.color(22), Color::Rgb(0xC0C0C0));
    }

    #[test]
    fn record_replaces_only_slots_from_8() {
        // ccv = 1, one LongRGB (12, 34, 56) plus reserved byte.
        let p = palette_from(&[1, 0, 0x12, 0x34, 0x56, 0]);
        assert_eq!(p.color(8), Color::Rgb(0x123456));
        assert_eq!(p.color(9), Color::Rgb(0xFFFFFF));
        assert_eq!(p.color(0), Color::Rgb(0x000000));
    }

    #[test]
    fn special_and_out_of_range_indexes_are_auto() {
        let p = Palette::default();
        assert_eq!(p.color(0x40), Color::Auto);
        assert_eq!(p.color(0x41), Color::Auto);
        assert_eq!(p.color(0x7FFF), Color::Auto);
        assert_eq!(p.color(64), Color::Auto);
    }

    #[test]
    fn hostile_counts_do_not_panic_and_are_bounded() {
        // ccv = 1000 but only one 4-byte entry present: reads that one, no panic.
        let p = palette_from(&[0xE8, 0x03, 0x01, 0x02, 0x03, 0x04]);
        assert_eq!(p.color(8), Color::Rgb(0x010203));
        assert_eq!(p.color(9), Color::Rgb(0xFFFFFF));

        // ccv present but no bytes at all: defaults kept.
        let empty = palette_from(&[5, 0]);
        assert_eq!(empty, Palette::default());

        // Empty record body.
        assert_eq!(palette_from(&[]), Palette::default());

        // 300 entries declared with enough bytes: at most 56 slots (8..=63) are replaced.
        let mut bytes = vec![0x2C, 0x01];
        for i in 0..300u32 {
            bytes.extend_from_slice(&[(i & 0xFF) as u8, 0, 0, 0]);
        }
        let many = palette_from(&bytes);
        assert_eq!(many.color(63), Color::Rgb(55 << 16));
        assert_eq!(many.color(7), Palette::default().color(7));
    }
}
