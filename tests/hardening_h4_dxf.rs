//! H4: untrusted DXF counts (HATCH 93, 95, 96, 97 and 98, LTYPE 73, DGN
//! line-style 93) never drive a reservation, a panic or an endless loop.
mod hardening_common;

use acadrust::entities::hatch::{BoundaryEdge, Hatch, HatchStyleType, SplineEdge};
use acadrust::entities::EntityType;
use acadrust::objects::{DgnLineStyleData, DgnLsComponentData, ObjectType};
use acadrust::{CadDocument, DxfReader, DxfReaderConfiguration, Vector2};
use hardening_common::{largest_alloc_during, no_panic, serial};
use std::time::{Duration, Instant};

fn fixture(name: &str) -> String {
    format!("{}/tests/hardening/{name}", env!("CARGO_MANIFEST_DIR"))
}

/// Reads `name` in failsafe mode on a worker thread. A count that drives an
/// endless loop fails the test after 60 seconds instead of hanging the run, and
/// a panic in the worker is re-raised unchanged.
fn read_failsafe(name: &'static str) -> CadDocument {
    read_failsafe_with(name, move || DxfReader::from_file(fixture(name)).unwrap())
}

/// Reads `bytes`, a DXF built by the test, the same way as [`read_failsafe`].
fn read_failsafe_bytes(name: &'static str, bytes: Vec<u8>) -> CadDocument {
    read_failsafe_with(name, move || {
        DxfReader::from_reader(std::io::Cursor::new(bytes)).unwrap()
    })
}

fn read_failsafe_with(
    name: &'static str,
    open: impl FnOnce() -> DxfReader + Send + 'static,
) -> CadDocument {
    let worker = std::thread::spawn(move || {
        largest_alloc_during(|| {
            no_panic(name, || {
                open()
                    .with_configuration(DxfReaderConfiguration {
                        failsafe: true,
                        ..Default::default()
                    })
                    .read()
                    .unwrap()
            })
        })
    });
    let deadline = Instant::now() + Duration::from_secs(60);
    while !worker.is_finished() {
        assert!(Instant::now() < deadline, "{name}: no document within 60 seconds");
        std::thread::sleep(Duration::from_millis(5));
    }
    let (document, largest) = match worker.join() {
        Ok(result) => result,
        Err(payload) => std::panic::resume_unwind(payload),
    };
    assert!(largest < 16 << 20, "{name}: a {largest}-byte request");
    document
}

fn polyline_vertex_counts(document: &CadDocument) -> Vec<usize> {
    document
        .entities()
        .filter_map(|entity| match entity {
            EntityType::Hatch(hatch) => Some(hatch),
            _ => None,
        })
        .flat_map(|hatch| hatch.paths.iter())
        .flat_map(|path| path.edges.iter())
        .filter_map(|edge| match edge {
            BoundaryEdge::Polyline(polyline) => Some(polyline.vertices.len()),
            _ => None,
        })
        .collect()
}

#[test]
fn negative_hatch_vertex_count_reads_an_empty_boundary() {
    let _guard = serial();
    let document = read_failsafe("h4_hatch93_neg.dxf");
    assert_eq!(polyline_vertex_counts(&document), vec![0]);
}

#[test]
fn huge_hatch_vertex_count_keeps_only_the_vertices_present() {
    let _guard = serial();
    let document = read_failsafe("h4_hatch93_huge.dxf");
    assert_eq!(polyline_vertex_counts(&document), vec![3]);
}

#[test]
fn negative_linetype_element_count_reads_no_elements() {
    let _guard = serial();
    let document = read_failsafe("h4_ltype73_neg.dxf");
    let linetype = document.line_types.get("H4NEG").expect("linetype H4NEG");
    assert!(linetype.elements.is_empty());
}

#[test]
fn huge_dgn_counts_are_clamped() {
    let _guard = serial();
    let document = read_failsafe("h4_dgn_huge.dxf");
    let mut seen = 0;
    for object in document.objects.values() {
        if let ObjectType::DgnLineStyle(style) = object {
            if let DgnLineStyleData::Component { component, .. } = &style.data {
                let count = match component {
                    DgnLsComponentData::Stroke(pattern) => pattern.strokes.len(),
                    DgnLsComponentData::Point(point) => point.symbols.len(),
                    _ => continue,
                };
                assert!(count <= 100_000, "{count} DGN entries");
                seen += 1;
            }
        }
    }
    assert_eq!(seen, 2, "both DGN components were read");
}

#[test]
fn huge_hatch_source_object_count_stops_at_the_end_of_the_stream() {
    let _guard = serial();
    // Group 97 claims 2^31 - 1 source boundary objects but the stream holds two
    // and ends; without the end-of-stream stop the loop spins through the whole
    // count (minutes of CPU).
    let document = read_failsafe("h4_hatch97_huge.dxf");
    let handles: Vec<usize> = document
        .entities()
        .filter_map(|entity| match entity {
            EntityType::Hatch(hatch) => Some(hatch),
            _ => None,
        })
        .flat_map(|hatch| hatch.paths.iter())
        .map(|path| path.boundary_handles.len())
        .collect();
    assert_eq!(polyline_vertex_counts(&document), vec![3]);
    assert_eq!(handles, vec![2], "the two source objects present are kept");
}

fn hatches(document: &CadDocument) -> Vec<&Hatch> {
    document
        .entities()
        .filter_map(|entity| match entity {
            EntityType::Hatch(hatch) => Some(&**hatch),
            _ => None,
        })
        .collect()
}

fn spline_edges(document: &CadDocument) -> Vec<&SplineEdge> {
    hatches(document)
        .into_iter()
        .flat_map(|hatch| hatch.paths.iter())
        .flat_map(|path| path.edges.iter())
        .filter_map(|edge| match edge {
            BoundaryEdge::Spline(spline) => Some(spline),
            _ => None,
        })
        .collect()
}

fn only_spline(document: &CadDocument) -> &SplineEdge {
    let splines = spline_edges(document);
    assert_eq!(splines.len(), 1, "one spline edge");
    splines[0]
}

#[test]
fn huge_hatch_list_counts_stop_at_the_first_pair_of_another_group() {
    let _guard = serial();
    // Groups 95, 96, 97 (spline fit points and source objects) and 98 all claim
    // 2^31 - 1 entries, and each list is followed by a pair that does not start
    // an entry. Without the stop the control-point and fit-point loops re-read
    // the pushed-back pair for the whole count (minutes of CPU) and the
    // source-object loop swallows every pair up to the end of the stream.
    let document = read_failsafe("h4_hatch_lists_huge.dxf");
    let spline = only_spline(&document);
    assert_eq!(spline.knots, vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
    assert_eq!(spline.control_points.len(), 3);
    assert_eq!(spline.fit_points.len(), 2);
    assert_eq!(spline.start_tangent, Vector2::new(1.0, 0.0));
    assert_eq!(spline.end_tangent, Vector2::new(1.0, 0.0));
    let hatch = hatches(&document)[0];
    assert_eq!(hatch.paths.len(), 1);
    assert_eq!(hatch.paths[0].boundary_handles.len(), 2);
    assert_eq!(hatch.style, HatchStyleType::Outer, "group 75 after the source objects");
    assert_eq!(hatch.seed_points, vec![Vector2::new(1.0, 2.0), Vector2::new(3.0, 4.0)]);
    assert_eq!(document.entities().count(), 2, "the LINE after the HATCH is kept");
}

#[test]
fn huge_spline_knot_count_stops_at_the_end_of_the_stream() {
    let _guard = serial();
    // Without the stop the knot loop turns 2^31 - 1 times reading nothing.
    let document = read_failsafe("h4_hatch95_eof.dxf");
    assert_eq!(only_spline(&document).knots, vec![0.0, 1.0]);
}

#[test]
fn huge_spline_control_point_count_stops_at_the_end_of_the_stream() {
    let _guard = serial();
    // Without the stop every remaining turn stores a default point (up to
    // 48 GiB).
    let document = read_failsafe("h4_hatch96_eof.dxf");
    assert_eq!(only_spline(&document).control_points.len(), 2);
}

#[test]
fn huge_spline_fit_point_count_stops_at_the_end_of_the_stream() {
    let _guard = serial();
    let document = read_failsafe("h4_hatch97fit_eof.dxf");
    assert_eq!(
        only_spline(&document).fit_points,
        vec![Vector2::new(0.0, 0.0), Vector2::new(10.0, 0.0)]
    );
}

#[test]
fn huge_seed_point_count_stops_at_the_end_of_the_stream() {
    let _guard = serial();
    let document = read_failsafe("h4_hatch98_eof.dxf");
    let hatches = hatches(&document);
    assert_eq!(hatches.len(), 1);
    assert_eq!(hatches[0].seed_points, vec![Vector2::new(1.0, 2.0), Vector2::new(3.0, 4.0)]);
    assert_eq!(polyline_vertex_counts(&document), vec![3]);
}

#[test]
fn a_bare_seed_point_count_at_the_end_of_the_stream_stores_no_point() {
    let _guard = serial();
    // The smallest hostile file: 43 bytes. Without the stop it stores
    // 2^31 - 1 default seed points (32 GiB) and the allocation failure aborts.
    let bytes = b"0\nSECTION\n2\nENTITIES\n0\nHATCH\n98\n2147483647\n".to_vec();
    let document = read_failsafe_bytes("bare 98", bytes);
    let hatches = hatches(&document);
    assert_eq!(hatches.len(), 1);
    assert!(hatches[0].seed_points.is_empty());
}

#[test]
fn hatch_list_counts_are_clamped_to_the_list_limit() {
    let _guard = serial();
    // Each list holds 100,005 entries; at most 100,000 (upstream's DXF list
    // clamp) are stored and the rest is left to the HATCH's outer loop.
    const PRESENT: usize = 100_005;
    const KEPT: usize = 100_000;
    let spline = "91\n1\n92\n1\n93\n1\n72\n4\n94\n2\n73\n0\n74\n0\n";
    let polyline = "91\n1\n92\n2\n72\n0\n73\n1\n93\n3\n\
                    10\n0.0\n20\n0.0\n10\n10.0\n20\n0.0\n10\n10.0\n20\n10.0\n";
    let cases: [(&'static str, String, fn(&CadDocument) -> usize); 5] = [
        (
            "95 knots",
            format!("{spline}95\n{PRESENT}\n96\n0\n{}", "40\n0.5\n".repeat(PRESENT)),
            |document| only_spline(document).knots.len(),
        ),
        (
            "96 control points",
            format!("{spline}95\n0\n96\n{PRESENT}\n{}", "10\n1.0\n20\n2.0\n".repeat(PRESENT)),
            |document| only_spline(document).control_points.len(),
        ),
        (
            "97 fit points",
            format!(
                "{spline}95\n0\n96\n0\n97\n{PRESENT}\n{}",
                "11\n1.0\n21\n2.0\n".repeat(PRESENT)
            ),
            |document| only_spline(document).fit_points.len(),
        ),
        (
            "97 source objects",
            format!("{polyline}97\n{PRESENT}\n{}", "330\n2B\n".repeat(PRESENT)),
            |document| hatches(document)[0].paths[0].boundary_handles.len(),
        ),
        (
            "98 seed points",
            format!("98\n{PRESENT}\n{}", "10\n1.0\n20\n2.0\n".repeat(PRESENT)),
            |document| hatches(document)[0].seed_points.len(),
        ),
    ];
    let mut stored = Vec::new();
    for (name, body, count) in cases {
        let text = format!(
            "0\nSECTION\n2\nENTITIES\n0\nHATCH\n5\n1F\n8\n0\n100\nAcDbEntity\n\
             100\nAcDbHatch\n{body}0\nENDSEC\n0\nEOF\n"
        );
        let document = read_failsafe_bytes(name, text.into_bytes());
        assert_eq!(hatches(&document).len(), 1, "{name}");
        stored.push((name, count(&document)));
    }
    let expected: Vec<(&str, usize)> = stored.iter().map(|&(name, _)| (name, KEPT)).collect();
    assert_eq!(stored, expected);
}
