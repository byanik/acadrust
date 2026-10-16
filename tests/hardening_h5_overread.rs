//! H5: reading past the end of the data is an error, never a read of zero
//! padding; failsafe page substitution is kept but counted.
mod hardening_common;

use std::io::{self, Cursor, Read, Seek, SeekFrom};

use acadrust::entities::{Circle, EntityType, Line};
use acadrust::io::dwg::checksum::apply_mask;
use acadrust::io::dwg::compressor_ac21::compress_ac21;
use acadrust::io::dwg::decompressor_ac21::decompress_ac21;
use acadrust::io::dwg::dwg_stream_readers::bit_reader::DwgBitReader;
use acadrust::io::dwg::dwg_version::DwgVersion;
use acadrust::types::Vector3;
use acadrust::{
    CadDocument, DwgReadOptions, DwgReader, DwgWriter, DxfError, DxfVersion, ReadStage,
};
use hardening_common::{serial, XorShift};

#[test]
fn reading_past_the_declared_length_is_an_error() {
    let _guard = serial();
    // Opcode 0x0C asks for a 20-byte literal, but the declared stream is the
    // opcode plus 19 bytes; the 20th byte is zero padding after the stream.
    let mut source = vec![0x0C];
    source.extend(1..=19u8);
    source.extend([0u8; 64]);
    let mut out = vec![0u8; 64];
    let result = decompress_ac21(&source, 0, 20, &mut out);
    assert!(
        matches!(result, Err(DxfError::Compression(_))),
        "the padding was read as data: {result:?}"
    );
    // The same stream with its 20th byte declared decodes.
    let mut out = vec![0u8; 64];
    decompress_ac21(&source, 0, 21, &mut out).unwrap();
}

#[test]
fn exact_length_streams_decode_without_padding() {
    let _guard = serial();
    let mut rng = XorShift::new(0x5);
    for size in [8usize, 33, 1000, 0x7400] {
        let mut data = rng.bytes(size);
        data.extend_from_within(..size / 2);
        let compressed = compress_ac21(&data);
        let mut out = vec![0u8; data.len()];
        decompress_ac21(&compressed, 0, compressed.len() as u32, &mut out)
            .unwrap_or_else(|e| panic!("size {size}: {e}"));
        assert!(out == data, "size {size}: roundtrip differs");
    }
}

#[test]
fn the_bit_reader_latches_its_first_over_read() {
    let _guard = serial();
    let mut reader = DwgBitReader::new(vec![0xAB], DwgVersion::AC24, DxfVersion::AC1024);
    assert!(reader.check_error().is_ok());
    let _ = reader.read_raw_long();
    assert!(reader.check_error().is_err());
    // After the first error every read yields zero and the error stays.
    assert_eq!(reader.read_bit_long(), 0);
    assert_eq!(reader.read_raw_short(), 0);
    reader.set_position(0);
    assert!(reader.check_error().is_err());
}

fn ac1021_document() -> Vec<u8> {
    let mut doc = CadDocument::with_version(DxfVersion::AC1021);
    for i in 0..200 {
        let x = i as f64;
        doc.add_entity(EntityType::Line(Line::from_points(
            Vector3::new(x, 0.0, 0.0),
            Vector3::new(x, 50.0, 0.0),
        )))
        .unwrap();
        doc.add_entity(EntityType::Circle(Circle::from_center_radius(
            Vector3::new(x, 25.0, 0.0),
            1.0,
        )))
        .unwrap();
    }
    DwgWriter::write_to_vec(&doc).unwrap()
}

fn zero_filled(stats: &acadrust::ReadStats) -> usize {
    stats
        .diagnostics
        .iter()
        .filter(|d| d.code == "page-zero-filled")
        .count()
}

#[test]
fn an_intact_file_has_no_zero_filled_pages() {
    let _guard = serial();
    let bytes = ac1021_document();
    let outcome = DwgReader::from_stream_with_options(Cursor::new(bytes.clone()), DwgReadOptions::failsafe())
        .read_with_stats()
        .unwrap();
    assert_eq!(zero_filled(&outcome.stats), 0);
    let strict = DwgReader::from_stream(Cursor::new(bytes)).read().unwrap();
    assert_eq!(strict.entities().count(), outcome.document.entities().count());
}

/// Overwrite the Reed-Solomon bytes of the first LZ77-compressed page of
/// `AcDb:AcDbObjects` with 0xFF, which no valid AC21 stream decodes from.
fn corrupt_one_objects_page(bytes: &mut [u8]) -> String {
    let info = DwgReader::from_stream(Cursor::new(bytes.to_vec()))
        .read_file_header()
        .unwrap();
    let section = info
        .section_descriptors
        .iter()
        .find(|s| s.name == "AcDb:AcDbObjects")
        .expect("objects section");
    let page = section
        .pages
        .iter()
        .find(|p| p.compressed_size < p.decompressed_size)
        .expect("a compressed objects page");
    let &(offset, _) = info.page_records.get(&(page.page_number as i32)).unwrap();
    let aligned = (page.compressed_size + 7) & !7;
    let read_length = aligned.div_ceil(251) * 255;
    let start = 0x480 + offset as usize;
    let end = (start + read_length as usize).min(bytes.len());
    bytes[start..end].fill(0xFF);
    section.name.clone()
}

#[test]
fn failsafe_zero_fills_a_corrupt_page_and_counts_it_once() {
    let _guard = serial();
    let mut bytes = ac1021_document();
    let section = corrupt_one_objects_page(&mut bytes);

    let outcome = DwgReader::from_stream_with_options(Cursor::new(bytes.clone()), DwgReadOptions::failsafe())
        .read_with_stats()
        .expect("failsafe returns the document");
    assert_eq!(zero_filled(&outcome.stats), 1);

    let mut strict = DwgReader::from_stream(Cursor::new(bytes));
    let info = strict.read_file_header().unwrap();
    let result = strict.get_section_buffer(&section, &info);
    assert!(result.is_err(), "strict mode must report the corrupt page");
}

// ---------------------------------------------------------------------------
// Beyond the brief: the short-read path, the AC18 path and the diagnostic cap.
// ---------------------------------------------------------------------------

/// A stream that returns at most `chunk` bytes per call, reports `Interrupted`
/// on every other call when asked to, and sees an early end of file for reads
/// that start inside `cut.0..cut.1` (the end is at `cut.2`).
struct FlakyStream {
    inner: Cursor<Vec<u8>>,
    chunk: usize,
    interrupt: bool,
    toggle: bool,
    cut: Option<(u64, u64, u64)>,
}

impl FlakyStream {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            inner: Cursor::new(bytes),
            chunk: usize::MAX,
            interrupt: false,
            toggle: false,
            cut: None,
        }
    }
}

impl Read for FlakyStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.interrupt {
            self.toggle = !self.toggle;
            if self.toggle {
                return Err(io::Error::from(io::ErrorKind::Interrupted));
            }
        }
        let position = self.inner.position();
        let mut want = buf.len().min(self.chunk);
        if let Some((start, end, cut)) = self.cut {
            if position >= start && position < end {
                want = want.min(cut.saturating_sub(position) as usize);
            }
        }
        if want == 0 {
            return Ok(0);
        }
        self.inner.read(&mut buf[..want])
    }
}

impl Seek for FlakyStream {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.inner.seek(pos)
    }
}

/// File range `(start, read_length)` of the first compressed page of the AC1021
/// objects section.
fn objects_page_range(bytes: &[u8]) -> (u64, u64) {
    let info = DwgReader::from_stream(Cursor::new(bytes.to_vec()))
        .read_file_header()
        .unwrap();
    let section = info
        .section_descriptors
        .iter()
        .find(|s| s.name == "AcDb:AcDbObjects")
        .expect("objects section");
    let page = section
        .pages
        .iter()
        .find(|p| p.compressed_size < p.decompressed_size)
        .expect("a compressed objects page");
    let &(offset, _) = info.page_records.get(&(page.page_number as i32)).unwrap();
    let aligned = (page.compressed_size + 7) & !7;
    (0x480 + offset as u64, aligned.div_ceil(251) * 255)
}

#[test]
fn a_stream_that_returns_short_reads_is_not_mistaken_for_end_of_file() {
    let _guard = serial();
    let bytes = ac1021_document();
    let expected = DwgReader::from_stream(Cursor::new(bytes.clone()))
        .read()
        .unwrap()
        .entities()
        .count();
    let mut stream = FlakyStream::new(bytes);
    stream.chunk = 7;
    stream.interrupt = true;
    let outcome = DwgReader::from_stream_with_options(stream, DwgReadOptions::failsafe())
        .read_with_stats()
        .unwrap();
    assert_eq!(zero_filled(&outcome.stats), 0);
    assert_eq!(outcome.document.entities().count(), expected);
}

#[test]
fn a_page_cut_short_by_end_of_file_is_zero_filled_and_counted() {
    let _guard = serial();
    let bytes = ac1021_document();
    let expected = DwgReader::from_stream(Cursor::new(bytes.clone()))
        .read()
        .unwrap()
        .entities()
        .count();
    let (start, read_length) = objects_page_range(&bytes);
    let factor = read_length / 255;

    // Only the last parity row of the page is missing: every data byte is
    // there, so the document is intact, but the zero padding is still an event.
    let mut tail = FlakyStream::new(bytes.clone());
    tail.cut = Some((start, start + read_length, start + read_length - factor));
    let outcome = DwgReader::from_stream_with_options(tail, DwgReadOptions::failsafe())
        .read_with_stats()
        .unwrap();
    let events: Vec<_> = outcome
        .stats
        .diagnostics
        .iter()
        .filter(|d| d.code == "page-zero-filled")
        .collect();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].stage, ReadStage::Section);
    assert_eq!(events[0].section.as_deref(), Some("AcDb:AcDbObjects"));
    assert_eq!(outcome.document.entities().count(), expected);

    // Strict mode reports the same page as an error.
    let mut cut_tail = FlakyStream::new(bytes.clone());
    cut_tail.cut = Some((start, start + read_length, start + read_length - factor));
    let mut strict = DwgReader::from_stream(cut_tail);
    let info = strict.read_file_header().unwrap();
    assert!(strict.get_section_buffer("AcDb:AcDbObjects", &info).is_err());

    // Half of the page is missing: failsafe still returns a document.
    let mut half = FlakyStream::new(bytes);
    half.cut = Some((start, start + read_length, start + read_length / 2));
    let outcome = DwgReader::from_stream_with_options(half, DwgReadOptions::failsafe())
        .read_with_stats()
        .expect("failsafe returns the document");
    assert!(zero_filled(&outcome.stats) >= 1);
}

#[test]
fn an_ac18_page_with_an_invalid_size_is_an_error_in_strict_and_counted_in_failsafe() {
    let _guard = serial();
    let mut doc = CadDocument::with_version(DxfVersion::AC1018);
    for i in 0..50 {
        doc.add_entity(EntityType::Line(Line::from_points(
            Vector3::new(i as f64, 0.0, 0.0),
            Vector3::new(i as f64, 5.0, 0.0),
        )))
        .unwrap();
    }
    let mut bytes = DwgWriter::write_to_vec(&doc).unwrap();
    assert_eq!(
        zero_filled(
            &DwgReader::from_stream_with_options(Cursor::new(bytes.clone()), DwgReadOptions::failsafe())
                .read_with_stats()
                .unwrap()
                .stats
        ),
        0
    );

    // Declare a compressed size of zero in the masked header of the first
    // objects page.
    let info = DwgReader::from_stream(Cursor::new(bytes.clone()))
        .read_file_header()
        .unwrap();
    let section = info
        .section_descriptors
        .iter()
        .find(|s| s.name == "AcDb:AcDbObjects")
        .expect("objects section");
    let page = section.pages.first().expect("an objects page");
    let &(offset, _) = info.page_records.get(&(page.page_number as i32)).unwrap();
    let range = offset as usize..offset as usize + 32;
    apply_mask(&mut bytes[range.clone()], offset as u64);
    bytes[range.start + 8..range.start + 12].copy_from_slice(&0i32.to_le_bytes());
    apply_mask(&mut bytes[range], offset as u64);

    let mut strict = DwgReader::from_stream(Cursor::new(bytes.clone()));
    let info = strict.read_file_header().unwrap();
    assert!(strict.get_section_buffer("AcDb:AcDbObjects", &info).is_err());

    let outcome = DwgReader::from_stream_with_options(Cursor::new(bytes), DwgReadOptions::failsafe())
        .read_with_stats()
        .expect("failsafe returns the document");
    let events: Vec<_> = outcome
        .stats
        .diagnostics
        .iter()
        .filter(|d| d.code == "page-zero-filled")
        .collect();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].section.as_deref(), Some("AcDb:AcDbObjects"));
}

#[test]
fn page_events_survive_the_diagnostic_limit() {
    let _guard = serial();
    let mut doc = CadDocument::with_version(DxfVersion::AC1021);
    for i in 0..4000 {
        doc.add_entity(EntityType::Line(Line::from_points(
            Vector3::new(i as f64, 0.0, 0.0),
            Vector3::new(i as f64, 5.0, 0.0),
        )))
        .unwrap();
    }
    let mut bytes = DwgWriter::write_to_vec(&doc).unwrap();
    // Corrupt every compressed page of the objects section.
    let info = DwgReader::from_stream(Cursor::new(bytes.clone()))
        .read_file_header()
        .unwrap();
    let section = info
        .section_descriptors
        .iter()
        .find(|s| s.name == "AcDb:AcDbObjects")
        .expect("objects section");
    let mut pages = 0;
    for page in section.pages.iter().filter(|p| p.compressed_size < p.decompressed_size) {
        let &(offset, _) = info.page_records.get(&(page.page_number as i32)).unwrap();
        let aligned = (page.compressed_size + 7) & !7;
        let start = 0x480 + offset as usize;
        let end = (start + (aligned.div_ceil(251) * 255) as usize).min(bytes.len());
        bytes[start..end].fill(0xFF);
        pages += 1;
    }
    assert!(pages >= 3, "the test needs several pages, got {pages}");

    let outcome = DwgReader::from_stream_with_options(Cursor::new(bytes), DwgReadOptions::failsafe())
        .read_with_stats()
        .expect("failsafe returns the document");
    assert_eq!(zero_filled(&outcome.stats), pages);
    assert!(
        outcome.stats.diagnostics.iter().any(|d| d.code == "diagnostic-limit-reached"),
        "the test no longer fills the diagnostic list"
    );
}
