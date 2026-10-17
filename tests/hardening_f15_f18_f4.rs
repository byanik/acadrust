//! Fork patches F15 (visit with stats), F18 (canonical block membership) and
//! F4 (raw viewport status bits). Inputs come from the fork's own writer.

use std::io::Cursor;

use acadrust::entities::{Circle, EntityType, Line};
use acadrust::types::DxfVersion;
use acadrust::{CadDocument, DwgReader, DwgWriter};

fn flat_sample(version: DxfVersion) -> Vec<u8> {
    let mut doc = CadDocument::with_version(version);
    for i in 0..10 {
        let x = f64::from(i);
        doc.add_entity(EntityType::Line(Line::from_coords(x, 0.0, 0.0, x, 5.0, 0.0))).expect("line");
    }
    doc.add_entity(EntityType::Circle(Circle::from_coords(3.0, 3.0, 0.0, 1.0))).expect("circle");
    DwgWriter::write_to_vec(&doc).expect("write dwg")
}

#[test]
fn f15_visit_with_stats_matches_full_read_stats() {
    for version in [DxfVersion::AC1018, DxfVersion::AC1032] {
        let bytes = flat_sample(version);
        let full = DwgReader::from_stream(Cursor::new(bytes.clone())).read_with_stats().expect("full read");
        let mut seen = 0usize;
        let visit = DwgReader::from_stream(Cursor::new(bytes))
            .read_visiting_with_stats(|_, e| {
                if !matches!(e, EntityType::Block(_) | EntityType::BlockEnd(_)) {
                    seen += 1;
                }
                Some(e)
            })
            .expect("visit read");
        assert!(full.stats.source_records > 0, "{version:?}: the sample has records");
        assert_eq!(visit.stats.source_records, full.stats.source_records, "{version:?}");
        assert_eq!(visit.stats.decoded_source_records, full.stats.decoded_source_records, "{version:?}");
        assert_eq!(visit.stats.skipped_source_records, full.stats.skipped_source_records, "{version:?}");
        assert_eq!(seen, full.document.entities().count(), "visitor sees every entity once");
    }
}
