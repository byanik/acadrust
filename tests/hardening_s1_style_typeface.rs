//! Fork patch S1: the STYLE TrueType typeface (its `ACAD` EED) is decoded before the entities, so
//! an entity visitor sees the same text styles as a full read. Inputs come from the fork's own
//! writer.

use std::io::Cursor;

use acadrust::entities::{EntityType, Line, Text};
use acadrust::tables::TextStyle;
use acadrust::types::{DxfVersion, Vector3};
use acadrust::{CadDocument, DwgReader, DwgWriter};

/// Bold (0x0200_0000) and italic (0x0100_0000) over the default pitch and family (34).
const FLAGS: i32 = 0x0300_0022;

fn styled_sample(version: DxfVersion) -> Vec<u8> {
    let mut doc = CadDocument::with_version(version);
    let mut style = TextStyle::with_truetype("TT", "Arial");
    style.handle = doc.allocate_handle();
    style.true_type_font_flags = FLAGS;
    doc.text_styles.add(style).expect("style");
    let mut text = Text::with_value("Arial", Vector3::new(0.0, 0.0, 0.0)).with_height(2.5);
    text.style = "TT".into();
    doc.add_entity(EntityType::Text(text)).expect("text");
    doc.add_entity(EntityType::Line(Line::from_coords(0.0, 0.0, 0.0, 5.0, 5.0, 0.0)))
        .expect("line");
    DwgWriter::write_to_vec(&doc).expect("write dwg")
}

fn typeface(doc: &CadDocument) -> Option<(String, i32)> {
    doc.text_styles
        .get("TT")
        .map(|s| (s.true_type_font.clone(), s.true_type_font_flags))
}

#[test]
fn s1_the_visitor_sees_the_style_typeface_of_a_full_read() {
    for version in [
        DxfVersion::AC1015,
        DxfVersion::AC1018,
        DxfVersion::AC1021,
        DxfVersion::AC1024,
        DxfVersion::AC1027,
        DxfVersion::AC1032,
    ] {
        let bytes = styled_sample(version);
        let full = DwgReader::from_stream(Cursor::new(bytes.clone()))
            .read_with_stats()
            .expect("full read");
        let expected = Some(("Arial".to_string(), FLAGS));
        assert_eq!(typeface(&full.document), expected, "{version:?}: full read");

        let mut seen = Vec::new();
        let visit = DwgReader::from_stream(Cursor::new(bytes))
            .read_visiting_with_stats(|doc, e| {
                if !matches!(e, EntityType::Block(_) | EntityType::BlockEnd(_)) {
                    seen.push(typeface(doc));
                }
                None
            })
            .expect("visit read");
        assert_eq!(seen.len(), 2, "{version:?}: the TEXT and the LINE are visited");
        assert!(
            seen.iter().all(|t| *t == expected),
            "{version:?}: every visited entity sees the typeface: {seen:?}"
        );
        assert_eq!(typeface(&visit.document), expected, "{version:?}: visit read");
    }
}
