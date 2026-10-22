//! DXF units of the annotative context angles (fork patch A1, P4-M4 review Important 4): group
//! 50 of AcDbBlkRefObjectContextData is in degrees, as INSERT's 50 and the TEXT context's
//! (ACadSharp, MIT: `BlockReferenceObjectContextData.Rotation` is an `IsAngle` code, converted
//! degrees <-> radians by its DXF reader and writer). The model holds radians.

use std::io::Cursor;

use acadrust::objects::{ObjectContextData, ObjectContextKind, ObjectType, Scale};
use acadrust::types::{DxfVersion, Vector3};
use acadrust::{CadDocument, DxfReader, DxfWriter};

fn blkref_doc(rotation: f64) -> (CadDocument, acadrust::Handle) {
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    let kind = ObjectContextKind::BlkRef {
        rotation,
        insertion: Vector3::new(1.0, 2.0, 0.0),
        scale: Vector3::new(2.0, 2.0, 2.0),
    };
    doc.register_object_context_class(kind.class_name().expect("modelled"));
    let scale_h = doc.allocate_handle();
    let mut scale = Scale::new("1:50", 1.0, 50.0);
    scale.handle = scale_h;
    doc.objects.insert(scale_h, ObjectType::Scale(scale));
    let leaf = doc.allocate_handle();
    doc.objects.insert(
        leaf,
        ObjectType::ObjectContextData(ObjectContextData {
            handle: leaf,
            owner_handle: acadrust::Handle::NULL,
            reactors: Vec::new(),
            xdictionary_handle: None,
            class_version: 4,
            is_default: true,
            scale: scale_h,
            kind,
        }),
    );
    (doc, leaf)
}

/// The group-50 value written in the AcDbBlkRefObjectContextData subclass.
fn written_50(text: &str) -> f64 {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let at = lines
        .iter()
        .position(|l| *l == "AcDbBlkRefObjectContextData")
        .expect("the subclass");
    let i = (at..lines.len())
        .step_by(1)
        .find(|&i| lines[i] == "50" && (i - at) % 2 == 1)
        .expect("group 50");
    lines[i + 1].parse().expect("a number")
}

fn read_rotation(text: String, leaf: acadrust::Handle) -> f64 {
    let doc = DxfReader::from_reader(Cursor::new(text.into_bytes()))
        .expect("reader")
        .read()
        .expect("read");
    match doc.objects.get(&leaf) {
        Some(ObjectType::ObjectContextData(c)) => match c.kind {
            ObjectContextKind::BlkRef { rotation, .. } => rotation,
            ref k => panic!("{k:?}"),
        },
        other => panic!("{other:?}"),
    }
}

#[test]
fn block_reference_context_rotation_is_degrees_in_dxf() {
    let (doc, leaf) = blkref_doc(std::f64::consts::FRAC_PI_2);
    let text = String::from_utf8(DxfWriter::new(&doc).write_to_vec().expect("write")).unwrap();
    assert!(
        (written_50(&text) - 90.0).abs() < 1e-9,
        "{}",
        written_50(&text)
    );
    assert!((read_rotation(text.clone(), leaf) - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    // a file that says 30 degrees
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let at = lines
        .iter()
        .position(|l| l.trim() == "AcDbBlkRefObjectContextData")
        .unwrap();
    let i = (at + 1..lines.len())
        .step_by(2)
        .find(|&i| lines[i].trim() == "50")
        .unwrap();
    lines[i + 1] = "30.0".to_string();
    let (head, tail) = (lines.join("\n"), "\n");
    let r = read_rotation(format!("{head}{tail}"), leaf);
    assert!((r - 30f64.to_radians()).abs() < 1e-12, "{r}");
}
