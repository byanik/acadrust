//! H3: no size read from a DWG reaches an allocation without a cap and the
//! caller's budget; one-byte corruptions of AC1018/AC1021/AC1032 files never
//! panic and never request a giant allocation.
mod hardening_common;

use std::io::Cursor;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use acadrust::entities::{Circle, EntityType, Line};
use acadrust::io::dwg::preview::{container_len, overall_size};
use acadrust::io::limits::AllocBudget;
use acadrust::types::Vector3;
use acadrust::{
    CadDocument, DwgReadOptions, DwgReader, DwgWriter, DxfError, DxfVersion, Preview,
    PreviewFormat,
};
use hardening_common::{largest_alloc_during, no_panic, serial, single_byte_mutations};

fn document(version: DxfVersion, lines: usize) -> Vec<u8> {
    let mut doc = CadDocument::with_version(version);
    for i in 0..lines {
        let x = i as f64;
        doc.add_entity(EntityType::Line(Line::from_points(
            Vector3::new(x, 0.0, 0.0),
            Vector3::new(x, 50.0, 0.0),
        )))
        .unwrap();
        if i % 2 == 0 {
            doc.add_entity(EntityType::Circle(Circle::from_center_radius(
                Vector3::new(x, 25.0, 0.0),
                0.5 + x / 100.0,
            )))
            .unwrap();
        }
    }
    DwgWriter::write_to_vec(&doc).unwrap()
}

fn read(bytes: Vec<u8>, options: DwgReadOptions) -> Result<CadDocument, DxfError> {
    DwgReader::from_stream_with_options(Cursor::new(bytes), options).read()
}

#[test]
fn a_one_mebibyte_budget_stops_a_large_read() {
    let _guard = serial();
    let bytes = document(DxfVersion::AC1032, 30_000);
    let unbudgeted = read(bytes.clone(), DwgReadOptions::failsafe()).unwrap();
    assert_eq!(
        unbudgeted
            .entities()
            .filter(|e| matches!(e, EntityType::Line(_)))
            .count(),
        30_000
    );
    let options = DwgReadOptions {
        failsafe: true,
        budget: Some(AllocBudget::new(|bytes| bytes <= 1 << 20)),
        ..DwgReadOptions::default()
    };
    let result = read(bytes, options);
    assert!(
        matches!(result, Err(DxfError::BudgetExceeded { .. })),
        "failsafe must not hide the budget: {:?}",
        result.err()
    );
}

fn sweep(version: DxfVersion, seed: u64) {
    let bytes = document(version, 200);
    let (_, largest) = largest_alloc_during(|| {
        for (position, mutated) in single_byte_mutations(&bytes, 2048, seed) {
            no_panic(&format!("{version:?} mutation at byte {position}"), || {
                let _ = read(mutated, DwgReadOptions::failsafe());
            });
        }
    });
    assert!(
        largest < 256 << 20,
        "{version:?}: largest allocation request {largest} bytes"
    );
}

#[test]
fn ac1018_mutations_never_panic() {
    let _guard = serial();
    sweep(DxfVersion::AC1018, 0x1018);
}

#[test]
fn ac1021_mutations_never_panic() {
    let _guard = serial();
    sweep(DxfVersion::AC1021, 0x1021);
}

#[test]
fn ac1032_mutations_never_panic() {
    let _guard = serial();
    sweep(DxfVersion::AC1032, 0x1032);
}

// ---------------------------------------------------------------------------
// Coverage beyond the sweeps: every version, both modes, and every size that
// the file controls (section totals, AC15 raw sections, the preview).
// ---------------------------------------------------------------------------

const ALL_VERSIONS: [DxfVersion; 4] = [
    DxfVersion::AC1015,
    DxfVersion::AC1018,
    DxfVersion::AC1021,
    DxfVersion::AC1032,
];

fn count_lines(document: &CadDocument) -> usize {
    document
        .entities()
        .filter(|e| matches!(e, EntityType::Line(_)))
        .count()
}

#[test]
fn a_refused_budget_stops_every_version_in_both_modes() {
    let _guard = serial();
    // The first allocation each reader makes is the one the budget refuses.
    let first = [
        (DxfVersion::AC1015, "AC15 section"),
        (DxfVersion::AC1018, "AC18 system page"),
        (DxfVersion::AC1021, "AC21 encoded page"),
        (DxfVersion::AC1032, "AC18 system page"),
    ];
    for (version, expected) in first {
        let bytes = document(version, 50);
        for failsafe in [false, true] {
            let options = DwgReadOptions {
                failsafe,
                budget: Some(AllocBudget::new(|_| false)),
                ..DwgReadOptions::default()
            };
            let result = no_panic("refused budget", || read(bytes.clone(), options));
            assert!(
                matches!(result, Err(DxfError::BudgetExceeded { what, .. }) if what == expected),
                "{version:?} failsafe={failsafe}: expected a refusal of {expected:?}, got {:?}",
                result.map(|d| d.entities().count())
            );
        }
    }
}

#[test]
fn ac18_data_page_buffers_are_budgeted() {
    let _guard = serial();
    let armed = Arc::new(AtomicBool::new(false));
    let asked = Arc::new(AtomicUsize::new(0));
    let (is_armed, counter) = (Arc::clone(&armed), Arc::clone(&asked));
    // Once armed: allow the section total (request 0) and refuse the first page
    // buffer (request 1).
    let budget = AllocBudget::new(move |_| {
        !is_armed.load(Ordering::SeqCst) || counter.fetch_add(1, Ordering::SeqCst) != 1
    });
    let options = DwgReadOptions {
        failsafe: true,
        budget: Some(budget),
        ..DwgReadOptions::default()
    };
    let mut reader =
        DwgReader::from_stream_with_options(Cursor::new(document(DxfVersion::AC1018, 200)), options);
    let info = reader.read_file_header().unwrap();
    armed.store(true, Ordering::SeqCst);
    let result = reader.get_section_buffer("AcDb:AcDbObjects", &info);
    assert!(
        matches!(result, Err(DxfError::BudgetExceeded { what: "AC18 page", .. })),
        "{:?}",
        result.map(|b| b.len())
    );
}

#[test]
fn a_section_sized_request_reaches_the_budget_in_every_version() {
    let _guard = serial();
    const LIMIT: usize = 256 << 10;
    for version in ALL_VERSIONS {
        let bytes = document(version, 30_000);
        // No false rejection: without a budget every line comes back.
        let whole = read(bytes.clone(), DwgReadOptions::failsafe()).unwrap();
        assert_eq!(count_lines(&whole), 30_000, "{version:?}");
        for failsafe in [false, true] {
            let options = DwgReadOptions {
                failsafe,
                budget: Some(AllocBudget::new(|bytes| bytes <= LIMIT)),
                ..DwgReadOptions::default()
            };
            let result = read(bytes.clone(), options);
            match result {
                Err(DxfError::BudgetExceeded { requested, .. }) => assert!(
                    requested > LIMIT as u64,
                    "{version:?} failsafe={failsafe}: refused a {requested}-byte request"
                ),
                other => panic!(
                    "{version:?} failsafe={failsafe}: expected BudgetExceeded, got {:?}",
                    other.map(|d| d.entities().count())
                ),
            }
        }
    }
}

/// Little-endian `i32` at `at`.
fn put_i32(bytes: &mut [u8], at: usize, value: i32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn an_ac15_section_larger_than_the_file_is_a_limit_error_not_an_allocation() {
    let _guard = serial();
    let mut bytes = document(DxfVersion::AC1015, 200);
    // Locator record 0 (AcDb:Header) is at 0x19: number(1) seeker(4) size(4).
    assert_eq!(bytes[0x19], 0, "record 0 is the header locator");
    put_i32(&mut bytes, 0x19 + 5, 0x7FFF_FFF0);
    let (outcome, largest) = largest_alloc_during(|| {
        no_panic("AC15 oversized section", || {
            DwgReader::from_stream_with_options(Cursor::new(bytes), DwgReadOptions::failsafe())
                .read_with_stats()
        })
    });
    let outcome = outcome.unwrap();
    assert!(
        outcome.stats.diagnostics.iter().any(|d| {
            d.code == "header-extract-failed" && d.message.contains("AC15 section")
        }),
        "the cap is reported: {:?}",
        outcome.stats.diagnostics
    );
    assert!(largest < 64 << 20, "a {largest}-byte request was made");
    assert!(outcome.document.entities().count() > 0, "other sections still read");
}

#[test]
fn a_section_total_beyond_the_cap_is_rejected_before_allocation() {
    let _guard = serial();
    for version in [DxfVersion::AC1018, DxfVersion::AC1021, DxfVersion::AC1032] {
        let bytes = document(version, 200);
        let mut reader =
            DwgReader::from_stream_with_options(Cursor::new(bytes), DwgReadOptions::failsafe());
        let mut info = reader.read_file_header().unwrap();
        let name = "AcDb:AcDbObjects";
        let index = info
            .section_descriptors
            .iter()
            .position(|s| s.name == name)
            .expect("objects section");
        let real = info.section_descriptors[index].compressed_size;
        let section = reader.get_section_buffer(name, &info).unwrap();
        assert_eq!(section.len() as u64, real, "{version:?}: honest size passes the cap");

        for claimed in [1u64 << 40, u64::MAX] {
            info.section_descriptors[index].compressed_size = claimed;
            let (result, largest) = largest_alloc_during(|| {
                no_panic("oversized section", || reader.get_section_buffer(name, &info))
            });
            assert!(
                matches!(result, Err(DxfError::LimitExceeded { .. })),
                "{version:?} claimed {claimed}: {:?}",
                result.map(|b| b.len())
            );
            assert!(largest < 64 << 20, "{version:?}: {largest}-byte request");
        }
        // A section that claims data but lists no page cannot be real either.
        info.section_descriptors[index].compressed_size = real;
        info.section_descriptors[index].pages.clear();
        let result = reader.get_section_buffer(name, &info);
        assert!(
            matches!(result, Err(DxfError::LimitExceeded { .. })),
            "{version:?}: {:?}",
            result.map(|b| b.len())
        );
    }
}

fn preview_data() -> Vec<u8> {
    (0..100_003u32).map(|i| (i % 251) as u8).collect()
}

fn document_with_preview(version: DxfVersion) -> Vec<u8> {
    let mut doc = CadDocument::with_version(version);
    doc.add_entity(EntityType::Line(Line::from_points(
        Vector3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 1.0, 0.0),
    )))
    .unwrap();
    doc.preview = Some(Preview {
        format: PreviewFormat::Png,
        data: preview_data(),
    });
    DwgWriter::write_to_vec(&doc).unwrap()
}

/// File offset of the preview container (the seeker at byte 0x0D).
fn preview_base(bytes: &[u8]) -> usize {
    let seeker = i32::from_le_bytes(bytes[0x0D..0x11].try_into().unwrap());
    let base = usize::try_from(seeker).expect("preview seeker");
    assert!(overall_size(&bytes[base..base + 20]).is_some(), "container at {base}");
    base
}

#[test]
fn a_preview_inside_the_file_is_read_back_in_every_version() {
    let _guard = serial();
    for version in ALL_VERSIONS {
        let bytes = document_with_preview(version);
        let doc = read(bytes, DwgReadOptions::failsafe()).unwrap();
        let preview = doc.preview.unwrap_or_else(|| panic!("{version:?}: preview lost"));
        assert_eq!(preview.format, PreviewFormat::Png, "{version:?}");
        assert_eq!(preview.data, preview_data(), "{version:?}");
    }
}

#[test]
fn a_preview_larger_than_the_file_is_ignored_without_allocating_it() {
    let _guard = serial();
    for version in ALL_VERSIONS {
        let mut bytes = document_with_preview(version);
        let base = preview_base(&bytes);
        // 60,000,000 passes the old 64 MiB sanity check but cannot fit the file.
        bytes[base + 16..base + 20].copy_from_slice(&60_000_000u32.to_le_bytes());
        let (result, largest) = largest_alloc_during(|| {
            no_panic("oversized preview", || read(bytes, DwgReadOptions::failsafe()))
        });
        let doc = result.unwrap_or_else(|e| panic!("{version:?}: {e}"));
        assert!(doc.preview.is_none(), "{version:?}");
        assert_eq!(count_lines(&doc), 1, "{version:?}: the drawing is unaffected");
        assert!(largest < 16 << 20, "{version:?}: a {largest}-byte request was made");
    }
}

#[test]
fn a_refused_budget_stops_the_read_at_the_preview_even_in_failsafe() {
    let _guard = serial();
    for version in ALL_VERSIONS {
        let bytes = document_with_preview(version);
        let base = preview_base(&bytes);
        let overall = u32::from_le_bytes(bytes[base + 16..base + 20].try_into().unwrap());
        let total = container_len(overall as usize);
        let options = DwgReadOptions {
            failsafe: true,
            budget: Some(AllocBudget::new(move |bytes| bytes != total)),
            ..DwgReadOptions::default()
        };
        let result = read(bytes, options);
        assert!(
            matches!(result, Err(DxfError::BudgetExceeded { what: "preview", .. })),
            "{version:?}: {:?}",
            result.map(|d| d.preview.is_some())
        );
    }
}

#[test]
fn ac1014_and_ac1015_mutations_never_panic() {
    let _guard = serial();
    sweep(DxfVersion::AC1014, 0x1014);
    sweep(DxfVersion::AC1015, 0x1015);
}
