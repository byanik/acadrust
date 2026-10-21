//! Fork patch L1: the DXF reader keeps the MLINESTYLE flags (group 70): fill, joints and the six
//! cap bits, with the DXF bit values the fork's own DXF writer uses.

use std::io::Cursor;

use acadrust::objects::{MLineStyle, MLineStyleFlags, ObjectType};
use acadrust::types::DxfVersion;
use acadrust::{CadDocument, DxfReader, DxfWriter};

fn read_dxf(bytes: &[u8]) -> CadDocument {
    DxfReader::from_reader(Cursor::new(bytes.to_vec()))
        .expect("reader")
        .read()
        .expect("read")
}

fn style_named<'a>(doc: &'a CadDocument, name: &str) -> &'a MLineStyle {
    doc.objects
        .values()
        .find_map(|o| match o {
            ObjectType::MLineStyle(s) if s.name == name => Some(s),
            _ => None,
        })
        .unwrap_or_else(|| panic!("MLINESTYLE {name} read"))
}

#[test]
fn l1_every_mlinestyle_flag_survives_a_dxf_round_trip() {
    for bits in [1, 2, 16, 32, 64, 256, 512, 1024, 0x0773] {
        let mut doc = CadDocument::with_version(DxfVersion::AC1032);
        let mut style = MLineStyle::new("CF_FLAGS");
        style.handle = doc.allocate_handle();
        style.add_element_at_offset(0.5);
        style.add_element_at_offset(-0.5);
        style.flags = MLineStyleFlags::from_bits(bits);
        doc.objects
            .insert(style.handle, ObjectType::MLineStyle(style));
        let bytes = DxfWriter::new(&doc).write_to_vec().expect("write dxf");
        let back = read_dxf(&bytes);
        assert_eq!(
            style_named(&back, "CF_FLAGS").flags.to_bits(),
            bits,
            "flags {bits:#x}"
        );
    }
}

fn group(code: i32, value: &str) -> String {
    format!("{code}\n{value}\n")
}

#[test]
fn l1_a_hand_written_mlinestyle_keeps_its_flags_and_elements() {
    // fill on, joints, start round, end inner arcs (1 + 2 + 64 + 512)
    let dxf = [
        group(0, "SECTION"),
        group(2, "OBJECTS"),
        group(0, "MLINESTYLE"),
        group(5, "2A"),
        group(100, "AcDbMlineStyle"),
        group(2, "CF_HAND"),
        group(70, "579"),
        group(3, ""),
        group(62, "3"),
        group(51, "90.0"),
        group(52, "90.0"),
        group(71, "2"),
        group(49, "1.0"),
        group(62, "1"),
        group(6, "BYLAYER"),
        group(49, "-1.0"),
        group(62, "256"),
        group(6, "BYBLOCK"),
        group(0, "ENDSEC"),
        group(0, "EOF"),
    ]
    .concat();
    let doc = read_dxf(dxf.as_bytes());
    let s = style_named(&doc, "CF_HAND");
    assert!(s.flags.fill_on && s.flags.display_joints, "{:?}", s.flags);
    assert!(
        s.flags.start_round_cap && s.flags.end_inner_arcs_cap,
        "{:?}",
        s.flags
    );
    assert!(
        !s.flags.start_square_cap && !s.flags.end_round_cap && !s.flags.start_inner_arcs_cap,
        "{:?}",
        s.flags
    );
    assert_eq!(s.elements.len(), 2);
    assert_eq!(s.elements[0].offset, 1.0);
    assert_eq!(s.elements[1].linetype, "BYBLOCK");
}
