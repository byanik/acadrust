//! P1: an entity colour that references a colour book (ENC flags 0x4000) can carry its names
//! inline, as 8-bit text: the colour name when flag bit 0x0100 is set, the book name when
//! 0x0200 is. Five public encoding-test drawings (R2004, R2007 twice, R2013 twice) were written
//! that way (both bits); their LINE records lost their geometry because the two names were read
//! as the line style scale and the flag bits that follow it, and every later field slid out of
//! place. Bit 0x0100 alone is also the ACI fallback index 256 (ByLayer) and 257 (None) of an
//! ordinary book colour, which the fork's own writer emits: only 0x0200 starts the names.
//!
//! These tests rebuild the record layout from the writer's primitives (no drawing is embedded):
//! the reader must consume the names exactly, in every format, and leave ordinary colours alone.
mod hardening_common;

use acadrust::io::dwg::dwg_reference_type::DwgReferenceType;
use acadrust::io::dwg::dwg_stream_readers::bit_reader::DwgBitReader;
use acadrust::io::dwg::dwg_stream_readers::merged_reader::DwgMergedReader;
use acadrust::io::dwg::dwg_stream_readers::object_reader::DwgObjectReader;
use acadrust::io::dwg::dwg_stream_readers::object_reader::entities::read_line;
use acadrust::io::dwg::dwg_stream_writers::bit_writer::DwgBitWriter;
use acadrust::io::dwg::dwg_version::DwgVersion;
use acadrust::types::{Color, DxfVersion, Vector3};
use hardening_common::{largest_alloc_during, no_panic, serial};
use std::collections::HashMap;

/// Formats whose entity colour is the R2004+ ENC.
const VERSIONS: [DxfVersion; 5] = [
    DxfVersion::AC1018,
    DxfVersion::AC1021,
    DxfVersion::AC1024,
    DxfVersion::AC1027,
    DxfVersion::AC1032,
];

/// Colour flags of the five drawings: complex + book colour + both name bits, ACI index 0.
const BOOK_WITH_NAMES: i16 = 0xC300_u16 as i16;

fn dwg(version: DxfVersion) -> DwgVersion {
    DwgVersion::from_dxf_version(version).unwrap()
}

fn writer(version: DxfVersion) -> DwgBitWriter {
    DwgBitWriter::new(dwg(version), version)
}

/// Inline 8-bit text as the drawings store it: BS length (the NUL counts), then the bytes.
fn write_inline_text(w: &mut DwgBitWriter, bytes: &[u8]) {
    w.write_bit_short(bytes.len() as i16 + 1);
    w.write_bytes(bytes);
    w.write_byte(0);
}

fn main_reader(w: DwgBitWriter, version: DxfVersion) -> DwgBitReader {
    DwgBitReader::new(w.into_bytes(), dwg(version), version)
}

#[test]
fn book_colour_names_are_consumed_in_every_format() {
    let _guard = serial();
    for version in VERSIONS {
        for (name, book) in [
            (&b"Entry"[..], &b"Book"[..]),
            (&b"Linked"[..], &b"Book"[..]),
            // Shift-JIS bytes of two kanji: the length counts bytes, not characters.
            (&[0x93, 0xFA, 0x96, 0x7B][..], &[0x83, 0x65, 0x83, 0x58, 0x83, 0x67][..]),
            (&b""[..], &b""[..]),
        ] {
            let mut w = writer(version);
            w.write_bit_short(BOOK_WITH_NAMES);
            write_inline_text(&mut w, name);
            write_inline_text(&mut w, book);
            w.write_bit_short(12345); // the next field must start right after the names
            let mut r = main_reader(w, version);
            let (_, _, is_book) = r.read_en_color();
            assert!(is_book, "{version:?}: a 0x4000 colour is a book colour");
            assert_eq!(r.read_bit_short(), 12345, "{version:?}: names consumed exactly");
            assert!(r.check_error().is_ok(), "{version:?}");
        }
    }
}

#[test]
fn bit_0x0200_alone_carries_only_the_book_name() {
    let _guard = serial();
    for version in VERSIONS {
        let mut w = writer(version);
        w.write_bit_short(0xC200_u16 as i16);
        write_inline_text(&mut w, b"Book");
        w.write_bit_short(12345);
        let mut r = main_reader(w, version);
        let (_, _, is_book) = r.read_en_color();
        assert!(is_book);
        assert_eq!(r.read_bit_short(), 12345, "{version:?}: one text, then the next field");
        assert!(r.check_error().is_ok());
    }
}

/// The fallback index of a book colour is 0 to 257 (ByLayer 256, None 257), written in the low
/// bits next to 0xC000: such colours carry no text. (Upstream's own tests write them.)
#[test]
fn book_colours_with_a_fallback_index_of_256_or_257_read_no_text() {
    let _guard = serial();
    for version in VERSIONS {
        for flags in [0xC100_u16, 0xC101, 0xC0FF, 0xC000] {
            let mut w = writer(version);
            w.write_bit_short(flags as i16);
            w.write_bit_short(12345);
            let mut r = main_reader(w, version);
            let (_, _, is_book) = r.read_en_color();
            assert!(is_book, "{flags:#06x}");
            assert_eq!(r.read_bit_short(), 12345, "{version:?} {flags:#06x}");
            assert!(r.check_error().is_ok());
        }
    }
}

#[test]
fn colours_without_the_name_bits_read_no_text() {
    let _guard = serial();
    for version in VERSIONS {
        // (flags, trailing values the colour owns before the marker)
        // Book colours as the other public drawings carry them: 0xC000 | ACI index.
        for flags in [0xC02A_u16, 0xC07E, 0xC0A6, 0xC007, 0xC0FE, 0xC0F3] {
            let mut w = writer(version);
            w.write_bit_short(flags as i16);
            w.write_bit_short(12345);
            let mut r = main_reader(w, version);
            let (_, _, is_book) = r.read_en_color();
            assert!(is_book);
            assert_eq!(r.read_bit_short(), 12345, "{version:?} {flags:#06x}");
            assert!(r.check_error().is_ok());
        }
        // The first line of the same drawings: true colour + transparency, ACI fallback 256
        // (bit 0x0100 set). Bit 0x0100 outside a book colour is just part of the index.
        let mut w = writer(version);
        w.write_bit_short(0xA100_u16 as i16);
        w.write_bit_long(0xC200_1234_u32 as i32); // rgb
        w.write_bit_long(0x0300_0033); // transparency, type 3: the value is the last byte
        w.write_bit_short(12345);
        let mut r = main_reader(w, version);
        let (color, transparency, is_book) = r.read_en_color();
        assert!(!is_book);
        assert_eq!(color, Color::from_rgb(0x00, 0x12, 0x34));
        assert_ne!(transparency, acadrust::types::Transparency::BY_LAYER);
        assert_eq!(r.read_bit_short(), 12345, "{version:?}: true colour untouched");
        // Plain ByLayer (256) and an ACI colour.
        for flags in [0x0100_i16, 5] {
            let mut w = writer(version);
            w.write_bit_short(flags);
            w.write_bit_short(12345);
            let mut r = main_reader(w, version);
            let _ = r.read_en_color();
            assert_eq!(r.read_bit_short(), 12345, "{version:?} {flags}");
        }
    }
}

#[test]
fn a_name_longer_than_the_record_fails_the_read_without_allocating_it() {
    let _guard = serial();
    for version in VERSIONS {
        let mut w = writer(version);
        w.write_bit_short(BOOK_WITH_NAMES);
        w.write_bit_short(i16::MAX); // 32,767 bytes announced, a few follow
        w.write_bytes(b"short");
        let ((), largest) = largest_alloc_during(|| {
            no_panic("oversized inline colour name", || {
                let mut r = main_reader(w, version);
                let _ = r.read_en_color();
                assert!(r.check_error().is_err(), "{version:?}: truncated name is an error");
            });
        });
        assert!(largest < 16 * 1024, "{version:?}: largest allocation {largest}");
    }
}

/// One LINE record body of the drawings, split into the main stream and the handle stream.
/// `color` writes the entity colour; `book_handle` is the colour-book handle it needs.
fn line_record(
    version: DxfVersion,
    color: impl Fn(&mut DwgBitWriter),
    book_handle: Option<u64>,
) -> (DwgMergedReader, DwgObjectReader) {
    let mut w = writer(version);
    if version == DxfVersion::AC1018 {
        w.write_raw_long(0); // size placeholder of R2000..R2004 records
    }
    w.write_handle(DwgReferenceType::Undefined, 0x31);
    w.write_bit_short(0); // no extended data
    w.write_bit(false); // no graphics
    w.write_2bits(2); // model space
    w.write_bit_long(0); // no reactors
    w.write_bit(false); // an extension dictionary exists (its handle is null, as in the drawings)
    if version >= DxfVersion::AC1027 {
        w.write_bit(false); // no data-store data
    }
    color(&mut w);
    w.write_bit_double(1.0); // line type scale
    w.write_2bits(0); // line type by layer
    if version >= DxfVersion::AC1021 {
        w.write_2bits(0); // material by layer
        w.write_byte(0); // shadow flags
    }
    w.write_2bits(0); // plot style by layer
    if version >= DxfVersion::AC1024 {
        for _ in 0..3 {
            w.write_bit(false); // no visual styles
        }
    }
    w.write_bit_short(0); // visible
    w.write_byte(29); // line weight by layer
    // LINE (5, 6) - (7, 8)
    w.write_bit(true); // z are zero
    w.write_raw_double(5.0);
    w.write_bit_double_with_default(7.0, 5.0); // value, then the reference the reader knows
    w.write_raw_double(6.0);
    w.write_bit_double_with_default(8.0, 6.0);
    w.write_bit_thickness(0.0);
    w.write_bit_extrusion(Vector3::new(0.0, 0.0, 1.0));
    let main_bits = w.position_in_bits();

    let mut h = writer(version);
    h.write_handle(DwgReferenceType::HardOwnership, 0); // null extension dictionary
    if let Some(book) = book_handle {
        h.write_handle(DwgReferenceType::HardPointer, book);
    }
    h.write_handle(DwgReferenceType::HardPointer, 0x12); // layer

    let mut main = DwgMergedReader::from_readers(
        main_reader(w, version),
        None,
        Some(main_reader(h, version)),
        version,
    );
    // The handle stream starts where the main data ends, as in a split record.
    main.set_main_data_end(main_bits);
    let object_reader = DwgObjectReader::new(Vec::new(), version, HashMap::new()).unwrap();
    (main, object_reader)
}

#[test]
fn a_line_with_a_book_colour_decodes_through_the_entity_preamble() {
    let _guard = serial();
    for version in VERSIONS {
        let (mut reader, objects) = line_record(
            version,
            |w| {
                w.write_bit_short(BOOK_WITH_NAMES);
                write_inline_text(w, b"Entry");
                write_inline_text(w, b"Book");
            },
            Some(0x35),
        );
        let common = objects.read_common_entity_data(&mut reader, 19);
        assert_eq!(common.common.handle, 0x31, "{version:?}");
        assert_eq!(common.color_book_handle, Some(0x35), "{version:?}");
        assert_eq!(common.layer_handle, 0x12, "{version:?}");
        assert_eq!(common.line_weight, 29, "{version:?}");
        assert_eq!(common.linetype_scale, 1.0, "{version:?}");
        let line = read_line(&mut reader, dwg(version));
        assert_eq!(line.start, Vector3::new(5.0, 6.0, 0.0), "{version:?}");
        assert_eq!(line.end, Vector3::new(7.0, 8.0, 0.0), "{version:?}");
        assert!(reader.check_error().is_ok(), "{version:?}: {:?}", reader.check_error());
        assert_eq!(reader.main_remaining_bits(), 0, "{version:?}: every main bit consumed");
    }
}

#[test]
fn a_line_with_an_ordinary_book_colour_still_decodes() {
    let _guard = serial();
    for version in VERSIONS {
        let (mut reader, objects) = line_record(
            version,
            |w| w.write_bit_short(0xC02A_u16 as i16),
            Some(0x35),
        );
        let common = objects.read_common_entity_data(&mut reader, 19);
        assert_eq!(common.color_book_handle, Some(0x35), "{version:?}");
        let line = read_line(&mut reader, dwg(version));
        assert_eq!(line.start, Vector3::new(5.0, 6.0, 0.0), "{version:?}");
        assert_eq!(line.end, Vector3::new(7.0, 8.0, 0.0), "{version:?}");
        assert!(reader.check_error().is_ok());
        assert_eq!(reader.main_remaining_bits(), 0);
    }
}
