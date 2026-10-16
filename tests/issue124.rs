use std::io::Cursor;

use acadrust::io::dwg::dwg_stream_readers::header_reader::read_header;
use acadrust::io::dwg::dwg_stream_writers::header_writer::write_header;
use acadrust::io::dwg::file_headers::section_definition::start_sentinels;
use acadrust::{CadDocument, DwgReadOptions, DwgReader, DwgWriter, DxfVersion};

#[test]
fn invalid_header_lengths_return_errors_for_all_section_layouts() {
    for (version, maintenance) in [
        (DxfVersion::AC1012, 0),
        (DxfVersion::AC1014, 0),
        (DxfVersion::AC1015, 0),
        (DxfVersion::AC1018, 0),
        (DxfVersion::AC1021, 0),
        (DxfVersion::AC1024, 0),
        (DxfVersion::AC1024, 4),
        (DxfVersion::AC1027, 4),
        (DxfVersion::AC1032, 0),
    ] {
        let document = CadDocument::with_version(version);
        let bytes = write_header(version, &document.header, maintenance);
        assert!(read_header(&bytes, version, maintenance).is_ok());
        for size in [0, 1, -1, i32::MIN, i32::MAX, bytes.len() as i32] {
            let mut corrupt = bytes.clone();
            corrupt[16..20].copy_from_slice(&size.to_le_bytes());
            assert!(
                read_header(&corrupt, version, maintenance).is_err(),
                "{version:?} size {size}"
            );
        }
    }
}

#[test]
fn public_reader_propagates_header_errors_and_failsafe_reports_them() {
    let document = CadDocument::with_version(DxfVersion::AC1015);
    let mut bytes = DwgWriter::write_to_vec(&document).unwrap();
    let header = bytes
        .windows(16)
        .position(|b| b == start_sentinels::HEADER)
        .unwrap();
    bytes[header + 16..header + 20].copy_from_slice(&i32::MAX.to_le_bytes());
    assert!(DwgReader::from_stream(Cursor::new(bytes.clone()))
        .read()
        .is_err());
    let mut reader =
        DwgReader::from_stream_with_options(Cursor::new(bytes), DwgReadOptions::failsafe());
    let outcome = reader.read_with_stats().unwrap();
    assert!(outcome
        .stats
        .diagnostics
        .iter()
        .any(|d| d.code == "header-decode-failed"));
}
