//! H4: untrusted DXF counts (HATCH 93 and 97, LTYPE 73, DGN line-style 93)
//! never drive a reservation, a panic or an endless loop.
mod hardening_common;

use acadrust::entities::hatch::BoundaryEdge;
use acadrust::entities::EntityType;
use acadrust::objects::{DgnLineStyleData, DgnLsComponentData, ObjectType};
use acadrust::{CadDocument, DxfReader, DxfReaderConfiguration};
use hardening_common::{largest_alloc_during, no_panic, serial};
use std::time::{Duration, Instant};

fn fixture(name: &str) -> String {
    format!("{}/tests/hardening/{name}", env!("CARGO_MANIFEST_DIR"))
}

/// Reads `name` in failsafe mode on a worker thread. A count that drives an
/// endless loop fails the test after 60 seconds instead of hanging the run, and
/// a panic in the worker is re-raised unchanged.
fn read_failsafe(name: &'static str) -> CadDocument {
    let worker = std::thread::spawn(move || {
        largest_alloc_during(|| {
            no_panic(name, || {
                DxfReader::from_file(fixture(name))
                    .unwrap()
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
