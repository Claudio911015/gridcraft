//! `.xls` cell formatting: FONT, PALETTE and XF records ([MS-XLS] 2.4.122, 2.4.188, 2.4.353).
//! Fixtures are assembled byte by byte from the spec; the expected values are literals taken
//! from the spec's tables, not computed with the reader under test.

use gridcraft_core::{CellRef, Value};
use gridcraft_model::{Alignment, BorderLine, BorderStyle, Color, Fill, HAlign, PatternType, Style, StyleId, VAlign, Workbook};

use super::xls_tests::compound;
use crate::read_xls;

fn rec(id: u16, payload: &[u8]) -> Vec<u8> {
    let mut v = id.to_le_bytes().to_vec();
    v.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    v.extend_from_slice(payload);
    v
}

fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

fn u16s(v: &[u16]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn bof(dt: u16) -> Vec<u8> {
    rec(0x0809, &cat(&[&u16s(&[0x0600, dt, 0x0DBB, 0x07CC]), &[0; 8]]))
}

fn eof() -> Vec<u8> {
    rec(0x000A, &[])
}

/// ShortXLUnicodeString with compressed characters: cch, flags, bytes.
fn short_str(s: &str) -> Vec<u8> {
    cat(&[&[s.len() as u8, 0], s.as_bytes()])
}

/// FONT record ([MS-XLS] 2.4.122): height in twips, weight, icv, grbit, underline, name.
fn font_rec(name: &str, twips: u16, bls: u16, icv: u16, grbit: u16, uls: u8) -> Vec<u8> {
    // dyHeight, grbit, icv, bls, sss (0 = normal)
    let mut p = u16s(&[twips, grbit, icv, bls, 0]);
    // uls, bFamily, bCharSet, reserved
    p.extend_from_slice(&[uls, 0, 0, 0]);
    p.extend(short_str(name));
    rec(0x0031, &p)
}

/// XF record, 20 bytes ([MS-XLS] 2.4.353). `align` is byte 6 (alc | fWrap<<3 | alcV<<4),
/// `b8` is byte 8 (cIndent | fShrinkToFit<<4), `u32_10` holds the borders (dg*, icv*, grbitDiag),
/// `u32_14` the top/bottom colours, diagonal and fill pattern, `u16_18` the fore/back colours.
#[allow(clippy::too_many_arguments)]
fn xf_rec(ifnt: u16, ifmt: u16, align: u8, trot: u8, b8: u8, u32_10: u32, u32_14: u32, u16_18: u16) -> Vec<u8> {
    let mut p = u16s(&[ifnt, ifmt, 0]);
    p.extend_from_slice(&[align, trot, b8, 0]);
    p.extend_from_slice(&u32_10.to_le_bytes());
    p.extend_from_slice(&u32_14.to_le_bytes());
    p.extend_from_slice(&u16_18.to_le_bytes());
    rec(0x00E0, &p)
}

/// An XF with bottom vertical alignment and nothing else set.
fn xf_plain(ifnt: u16) -> Vec<u8> {
    xf_rec(ifnt, 0, 0x20, 0, 0, 0, 0, 0)
}

/// NUMBER record (0x0203) at (row, col) with XF index `xf`.
fn num(row: u16, col: u16, xf: u16, x: f64) -> Vec<u8> {
    rec(0x0203, &cat(&[&u16s(&[row, col, xf]), &x.to_le_bytes()]))
}

/// A workbook with the given globals records (FONT, PALETTE, XF) and one sheet "Sheet1".
fn book(fonts: &[Vec<u8>], palette: Option<Vec<u8>>, xfs: &[Vec<u8>], sheet_records: &[u8]) -> Vec<u8> {
    let globals = |pos: u32| {
        let mut g = vec![bof(0x0005)];
        g.extend(fonts.iter().cloned());
        g.extend(palette.iter().cloned());
        g.extend(xfs.iter().cloned());
        g.push(rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"])));
        g.push(eof());
        g.concat()
    };
    let len = globals(0).len() as u32;
    let stream = cat(&[&globals(len), &bof(0x0010), sheet_records, &eof()]);
    compound("Workbook", &stream)
}

fn style_of(wb: &Workbook, a1: &str) -> Style {
    let sh = &wb.sheets[0];
    wb.styles.get(sh.style_id(CellRef::parse(a1).unwrap())).clone()
}

/// Six FONT records: 0-3 Arial 10, then the record that BIFF8 reaches as font 5 (Arial 12 bold,
/// colour 10) and the one reached as font 6 (Courier New 9).
fn six_fonts() -> Vec<Vec<u8>> {
    vec![
        font_rec("Arial", 200, 400, 0x7FFF, 0, 0),
        font_rec("Arial", 200, 400, 0x7FFF, 0, 0),
        font_rec("Arial", 200, 400, 0x7FFF, 0, 0),
        font_rec("Arial", 200, 400, 0x7FFF, 0, 0),
        font_rec("Arial", 240, 700, 10, 0, 1),
        font_rec("Courier New", 180, 400, 0x7FFF, 0, 0),
    ]
}

#[test]
fn xls_fonts_reach_cells() {
    // XF1 uses ifnt 5 (the fifth FONT record), XF2 uses ifnt 6: index 4 is skipped by BIFF8.
    let xfs = [xf_plain(0), xf_plain(5), xf_plain(6)];
    let sheet = cat(&[&num(0, 0, 1, 7.0), &num(1, 0, 2, 8.0)]);
    let (wb, _) = read_xls(&book(&six_fonts(), None, &xfs, &sheet)).unwrap();

    let a1 = style_of(&wb, "A1").font;
    assert_eq!((a1.name.as_str(), a1.size, a1.bold, a1.color), ("Arial", 12.0, true, Color::Rgb(0xFF0000)));
    let a2 = style_of(&wb, "A2").font;
    assert_eq!((a2.name.as_str(), a2.size, a2.bold), ("Courier New", 9.0, false));
}

#[test]
fn xls_font_index_below_four_is_not_shifted() {
    let fonts = [
        font_rec("Arial", 200, 400, 0x7FFF, 0, 0),
        font_rec("Arial", 200, 400, 0x7FFF, 0, 0),
        font_rec("Arial", 200, 400, 0x7FFF, 0, 0),
        font_rec("Verdana", 160, 400, 0x7FFF, 0x0002, 0),
        font_rec("Arial", 240, 700, 10, 0, 0),
    ];
    let xfs = [xf_plain(0), xf_plain(3), xf_plain(5)];
    let sheet = cat(&[&num(0, 0, 1, 1.0), &num(1, 0, 2, 2.0)]);
    let (wb, _) = read_xls(&book(&fonts, None, &xfs, &sheet)).unwrap();

    // ifnt 3 is FONT record 3 (Verdana 8, italic): not shifted.
    let a1 = style_of(&wb, "A1").font;
    assert_eq!((a1.name.as_str(), a1.size, a1.italic), ("Verdana", 8.0, true));
    // ifnt 5 is FONT record 4 (Arial 12 bold).
    let a2 = style_of(&wb, "A2").font;
    assert_eq!((a2.name.as_str(), a2.size, a2.bold), ("Arial", 12.0, true));
}

#[test]
fn xls_normal_font_is_the_default_style() {
    let fonts = [font_rec("Arial", 200, 400, 0x7FFF, 0, 0)];
    let xfs = [xf_plain(0)];
    let sheet = num(0, 1, 0, 1.0);
    let (wb, _) = read_xls(&book(&fonts, None, &xfs, &sheet)).unwrap();

    let sh = &wb.sheets[0];
    let b1 = CellRef::parse("B1").unwrap();
    assert_eq!(sh.style_id(b1), StyleId::DEFAULT);
    let normal = wb.styles.get(StyleId::DEFAULT).font.clone();
    assert_eq!((normal.name.as_str(), normal.size), ("Arial", 10.0));
}

#[test]
fn xls_fill_and_borders_on_blank_cells() {
    let fonts = [font_rec("Arial", 200, 400, 0x7FFF, 0, 0)];
    // XF1: left Thin (dg 1, icv 8 at bits 16-22), bottom Double (dg 6 at bits 12-15, icv 10 at
    // bits 7-13 of u32_14), fls 1 = Solid (bits 26-31), icvFore 13 (yellow), icvBack 0x41.
    let xf1 = xf_rec(0, 0, 0x20, 0, 0, 0x0008_6001, 0x0400_0500, 13 | (0x41 << 7));
    let xfs = [xf_plain(0), xf1];
    let sheet = rec(0x0201, &u16s(&[2, 2, 1]));
    let (wb, _) = read_xls(&book(&fonts, None, &xfs, &sheet)).unwrap();

    let sh = &wb.sheets[0];
    let c3 = CellRef::parse("C3").unwrap();
    assert!(sh.cell(c3).is_some());
    assert_eq!(sh.value(c3), Value::Empty);

    let s = style_of(&wb, "C3");
    assert_eq!(s.fill, Fill { pattern: PatternType::Solid, fg: Color::Rgb(0xFFFF00), bg: Color::Auto });
    assert_eq!(s.borders.left, BorderLine { style: BorderStyle::Thin, color: Color::Rgb(0x000000) });
    assert_eq!(s.borders.bottom, BorderLine { style: BorderStyle::Double, color: Color::Rgb(0xFF0000) });
    assert_eq!(s.borders.top, BorderLine::default());
    assert_eq!(s.borders.right, BorderLine::default());
}

#[test]
fn xls_custom_palette_colours() {
    // PALETTE: ccv 1, then slot 8 = (R 0x12, G 0x34, B 0x56, reserved 0).
    let palette = rec(0x0092, &cat(&[&u16s(&[1]), &[0x12, 0x34, 0x56, 0]]));
    let fonts = [font_rec("Arial", 200, 400, 8, 0, 0)];
    let xfs = [xf_plain(0)];
    let sheet = num(0, 0, 0, 1.0);
    let (wb, _) = read_xls(&book(&fonts, Some(palette), &xfs, &sheet)).unwrap();

    assert_eq!(style_of(&wb, "A1").font.color, Color::Rgb(0x123456));
}

#[test]
fn xls_alignment() {
    // XF1: alc 2 (centre), fWrap, alcV 1 (centre), trot 135, cIndent 3.
    let xf1 = xf_rec(0, 0, 0x02 | 0x08 | (1 << 4), 135, 3, 0, 0, 0);
    let fonts = [font_rec("Arial", 200, 400, 0x7FFF, 0, 0)];
    let xfs = [xf_plain(0), xf1];
    // COLINFO: columns 2..=2, width 2048 (8 characters), ixfe 1, flags 0, reserved.
    let sheet = cat(&[&num(0, 0, 1, 1.0), &rec(0x007D, &u16s(&[2, 2, 2048, 1, 0, 0]))]);
    let (wb, _) = read_xls(&book(&fonts, None, &xfs, &sheet)).unwrap();

    let expected = Alignment { h: HAlign::Center, v: VAlign::Center, wrap: true, shrink: false, indent: 3, rotation: -45 };
    let sh = &wb.sheets[0];
    let id = sh.style_id(CellRef::parse("A1").unwrap());
    assert_ne!(id, StyleId::DEFAULT);
    assert_eq!(wb.styles.get(id).align, expected);
    assert_eq!(sh.cols.get(&2).and_then(|l| l.style), Some(id));
    assert_eq!(wb.styles.get(sh.cols[&2].style.unwrap()).align, expected);
}

#[test]
fn xls_hostile_style_records_never_panic() {
    // FONT of 2 bytes, XF of 4 bytes, PALETTE with ccv 1000 and one entry, and an XF of 0xFF bytes
    // whose ifnt (0xFFFF) does not exist.
    let fonts = [rec(0x0031, &[0x14, 0x00])];
    let palette = rec(0x0092, &cat(&[&u16s(&[1000]), &[1, 2, 3, 0]]));
    let xfs = [rec(0x00E0, &[0, 0, 0, 0]), rec(0x00E0, &[0xFF; 20])];
    let sheet = cat(&[&num(0, 0, 0, 7.0), &num(1, 0, 1, 9.0)]);
    let (wb, report) = read_xls(&book(&fonts, Some(palette), &xfs, &sheet)).unwrap();

    let sh = &wb.sheets[0];
    assert_eq!(sh.value(CellRef::parse("A1").unwrap()), Value::Number(7.0));
    assert_eq!(sh.value(CellRef::parse("A2").unwrap()), Value::Number(9.0));
    assert!(report.warnings.iter().any(|w| w.contains("font")), "{:?}", report.warnings);
}
