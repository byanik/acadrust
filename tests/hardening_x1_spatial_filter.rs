//! Fork patch X1: the two 12-double matrices of a SPATIAL_FILTER (the inverse block transform
//! and the clip bound transform) are stored row by row: three rows of four values, the
//! translation in the 4th, 8th and 12th value (DXF group 40 and the DWG BD run alike). The fork
//! read and wrote them column by column, so every translation landed in the wrong cells.

use std::io::Cursor;

use acadrust::objects::{ObjectType, SpatialFilter};
use acadrust::types::{DxfVersion, Matrix4, Vector2, Vector3};
use acadrust::{CadDocument, DwgReader, DwgWriter, DxfReader, DxfWriter};

/// A rotation by 30 degrees about Z with a translation, as an INSERT's inverse transform is.
fn rotated() -> Matrix4 {
    let (s, c) = (0.5_f64, 3.0_f64.sqrt() / 2.0);
    Matrix4 {
        m: [
            [c, s, 0.0, -1200.5],
            [-s, c, 0.0, 875.25],
            [0.0, 0.0, 1.0, -3.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    }
}

/// A clip bound transform: a translation of the clip space's origin to 0.
fn shifted() -> Matrix4 {
    Matrix4 {
        m: [
            [1.0, 0.0, 0.0, -10.0],
            [0.0, 1.0, 0.0, -20.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    }
}

fn row_values(m: &Matrix4) -> Vec<f64> {
    (0..3).flat_map(|r| (0..4).map(move |c| m.m[r][c])).collect()
}

fn doc_with_filter() -> (CadDocument, acadrust::types::Handle) {
    let mut doc = CadDocument::with_version(DxfVersion::AC1032);
    let h = doc.allocate_handle();
    let mut sf = SpatialFilter::new();
    sf.handle = h;
    sf.boundary_points = vec![Vector2::new(-1.0, -2.0), Vector2::new(3.0, 4.0)];
    sf.normal = Vector3::new(0.0, 0.0, 1.0);
    sf.origin = Vector3::new(10.0, 20.0, 0.0);
    sf.inverse_block_transform = rotated();
    sf.clip_bound_transform = shifted();
    doc.objects.insert(h, ObjectType::SpatialFilter(sf));
    (doc, h)
}

fn filter(doc: &CadDocument, h: acadrust::types::Handle) -> &SpatialFilter {
    match doc.objects.get(&h) {
        Some(ObjectType::SpatialFilter(f)) => f,
        other => panic!("SPATIAL_FILTER expected, got {other:?}"),
    }
}

fn close(a: &Matrix4, b: &Matrix4) -> bool {
    (0..4).all(|r| (0..4).all(|c| (a.m[r][c] - b.m[r][c]).abs() < 1e-12))
}

fn group(code: i32, value: &str) -> String {
    format!("{code}\n{value}\n")
}

#[test]
fn x1_dxf_matrices_are_read_row_by_row() {
    // the values as AutoCAD writes them: row 0, row 1, row 2 of each matrix
    let mut dxf = vec![
        group(0, "SECTION"),
        group(2, "OBJECTS"),
        group(0, "SPATIAL_FILTER"),
        group(5, "2A"),
        group(100, "AcDbFilter"),
        group(100, "AcDbSpatialFilter"),
        group(70, "2"),
        group(10, "-1.0"),
        group(20, "-2.0"),
        group(10, "3.0"),
        group(20, "4.0"),
        group(210, "0.0"),
        group(220, "0.0"),
        group(230, "1.0"),
        group(11, "10.0"),
        group(21, "20.0"),
        group(31, "0.0"),
        group(71, "1"),
        group(72, "0"),
        group(73, "0"),
    ];
    for v in row_values(&rotated())
        .into_iter()
        .chain(row_values(&shifted()))
    {
        dxf.push(group(40, &format!("{v:.17}")));
    }
    dxf.push(group(0, "ENDSEC"));
    dxf.push(group(0, "EOF"));
    let doc = DxfReader::from_reader(Cursor::new(dxf.concat().into_bytes()))
        .expect("reader")
        .read()
        .expect("read");
    let f = filter(&doc, acadrust::types::Handle::new(0x2A));
    assert!(
        close(&f.inverse_block_transform, &rotated()),
        "{:?}",
        f.inverse_block_transform
    );
    assert!(
        close(&f.clip_bound_transform, &shifted()),
        "{:?}",
        f.clip_bound_transform
    );
}

#[test]
fn x1_dxf_writer_writes_the_rows_in_order() {
    let (doc, _) = doc_with_filter();
    let bytes = DxfWriter::new(&doc).write_to_vec().expect("write dxf");
    let text = String::from_utf8(bytes).expect("ascii dxf");
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    // the object's subclass marker, after the CLASSES entry of the same name
    let start = lines
        .windows(2)
        .rposition(|w| w[0] == "100" && w[1] == "AcDbSpatialFilter")
        .expect("AcDbSpatialFilter subclass")
        + 1;
    let forty: Vec<f64> = lines[start + 1..]
        .chunks(2)
        .take_while(|p| p[0] != "0")
        .filter(|p| p[0] == "40")
        .map(|p| p[1].parse().expect("a double"))
        .collect();
    let expected: Vec<f64> = row_values(&rotated())
        .into_iter()
        .chain(row_values(&shifted()))
        .collect();
    assert_eq!(forty.len(), 24, "{forty:?}");
    for (got, want) in forty.iter().zip(&expected) {
        assert!((got - want).abs() < 1e-12, "{forty:?} != {expected:?}");
    }
}

#[test]
fn x1_dxf_and_dwg_round_trips_keep_both_matrices() {
    let (doc, h) = doc_with_filter();
    let dxf = DxfWriter::new(&doc).write_to_vec().expect("write dxf");
    let back = DxfReader::from_reader(Cursor::new(dxf))
        .expect("reader")
        .read()
        .expect("read");
    let f = filter(&back, h);
    assert!(close(&f.inverse_block_transform, &rotated()));
    assert!(close(&f.clip_bound_transform, &shifted()));

    let dwg = DwgWriter::write_to_vec(&doc).expect("write dwg");
    let back = DwgReader::from_stream(Cursor::new(dwg))
        .read()
        .expect("read dwg");
    let f = filter(&back, h);
    assert!(
        close(&f.inverse_block_transform, &rotated()),
        "{:?}",
        f.inverse_block_transform
    );
    assert!(close(&f.clip_bound_transform, &shifted()));
}
