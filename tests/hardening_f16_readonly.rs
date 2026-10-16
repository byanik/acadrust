//! F16: built without the `writers` feature, the reader still opens every
//! public fixture, failsafe and strict, without panicking.
mod hardening_common;

use acadrust::{DwgReadOptions, DwgReader, DxfReader, DxfReaderConfiguration};
use hardening_common::{no_panic, serial};

const DWG_FIXTURES: &[&str] = &[
    "tests/anonymous-names.dwg",
    "tests/mpolygon-fixture.dwg",
    "tests/datatable/point_object_id.dwg",
    "tests/fixtures/dynamic-blocks/lookup.dwg",
    "tests/fixtures/dynamic-blocks/point.dwg",
    "tests/fixtures/dynamic-blocks/polar.dwg",
    "tests/fixtures/dynamic-blocks/xy.dwg",
];

const DXF_FIXTURES: &[&str] = &[
    "tests/datatable/point_object_id.dxf",
    "tests/fixtures/viewports/reference-angle-units.dxf",
    "tests/fixtures/viewports/reference-clip-activation.dxf",
    "tests/issue68/r12_mleaderstyle_repro.dxf",
];

fn path(relative: &str) -> String {
    format!("{}/{relative}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn public_dwg_fixtures_read_without_the_writers() {
    let _guard = serial();
    for relative in DWG_FIXTURES {
        let outcome = no_panic(relative, || {
            DwgReader::from_file_with_options(path(relative), DwgReadOptions::failsafe())
                .unwrap()
                .read_with_stats()
                .unwrap()
        });
        assert!(outcome.stats.has_usable_drawing_data(), "{relative}");
        let strict = no_panic(relative, || DwgReader::from_file(path(relative)).unwrap().read());
        assert!(strict.is_ok(), "{relative}: {:?}", strict.err());
    }
}

#[test]
fn public_dxf_fixtures_read_without_the_writers() {
    let _guard = serial();
    for relative in DXF_FIXTURES {
        let outcome = no_panic(relative, || {
            DxfReader::from_file(path(relative))
                .unwrap()
                .with_configuration(DxfReaderConfiguration {
                    failsafe: true,
                    ..Default::default()
                })
                .read_with_stats()
                .unwrap()
        });
        assert!(outcome.stats.has_usable_drawing_data(), "{relative}");
    }
}
