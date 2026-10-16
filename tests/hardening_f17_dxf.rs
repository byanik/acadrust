//! F17: a corrupt DXF entity is skipped on its own in failsafe mode, and
//! ENTITIES block membership is built in linear time with unchanged lists.
mod hardening_common;

use std::io::Cursor;
use std::time::{Duration, Instant};

use acadrust::entities::EntityType;
use acadrust::{CadDocument, DxfReader, DxfReaderConfiguration, ReadOutcome, ReadStage};
use hardening_common::serial;

fn group(code: i32, value: &str) -> String {
    format!("{code}\n{value}\n")
}

fn line(handle: &str, owner: &str, x: f64) -> String {
    [
        group(0, "LINE"),
        group(5, handle),
        group(330, owner),
        group(100, "AcDbEntity"),
        group(8, "0"),
        group(100, "AcDbLine"),
        group(10, &format!("{x:.1}")),
        group(20, "0.0"),
        group(30, "0.0"),
        group(11, &format!("{x:.1}")),
        group(21, "5.0"),
        group(31, "0.0"),
    ]
    .concat()
}

/// A LINE whose first coordinate group code is the text `1O` (letter O),
/// which no DXF reader can parse as an integer group code.
fn corrupt_line(handle: &str, owner: &str) -> String {
    [
        group(0, "LINE"),
        group(5, handle),
        group(330, owner),
        group(100, "AcDbEntity"),
        group(8, "0"),
        group(100, "AcDbLine"),
        "1O\n2.0\n".to_string(),
        group(20, "0.0"),
        group(30, "0.0"),
        group(11, "2.0"),
        group(21, "5.0"),
        group(31, "0.0"),
    ]
    .concat()
}

fn block_record(handle: &str, name: &str) -> String {
    [
        group(0, "BLOCK_RECORD"),
        group(5, handle),
        group(100, "AcDbSymbolTableRecord"),
        group(100, "AcDbBlockTableRecord"),
        group(2, name),
    ]
    .concat()
}

fn drawing(block_records: &str, entities: &str) -> String {
    [
        group(0, "SECTION"),
        group(2, "TABLES"),
        group(0, "TABLE"),
        group(2, "BLOCK_RECORD"),
        group(5, "1"),
        block_records.to_string(),
        group(0, "ENDTAB"),
        group(0, "ENDSEC"),
        group(0, "SECTION"),
        group(2, "ENTITIES"),
        entities.to_string(),
        group(0, "ENDSEC"),
        group(0, "EOF"),
    ]
    .concat()
}

fn read(text: &str, failsafe: bool) -> acadrust::Result<ReadOutcome> {
    DxfReader::from_reader(Cursor::new(text.as_bytes().to_vec()))?
        .with_configuration(DxfReaderConfiguration {
            failsafe,
            ..Default::default()
        })
        .read_with_stats()
}

fn line_handles(document: &CadDocument) -> Vec<u64> {
    document
        .entities()
        .filter(|entity| matches!(entity, EntityType::Line(_)))
        .map(|entity| entity.common().handle.value())
        .collect()
}

fn layout_records() -> String {
    [
        block_record("1F", "*Model_Space"),
        block_record("1E", "*Paper_Space"),
    ]
    .concat()
}

#[test]
fn a_corrupt_entity_skips_only_itself() {
    let _guard = serial();
    let entities = [
        line("A1", "1F", 1.0),
        line("A2", "1F", 2.0),
        corrupt_line("A3", "1F"),
        line("A4", "1F", 4.0),
        line("A5", "1F", 5.0),
    ]
    .concat();
    let text = drawing(&layout_records(), &entities);

    let outcome = read(&text, true).unwrap();
    assert_eq!(line_handles(&outcome.document), vec![0xA1, 0xA2, 0xA4, 0xA5]);
    let failed: Vec<_> = outcome
        .stats
        .diagnostics
        .iter()
        .filter(|d| d.code == "entity-read-failed")
        .collect();
    assert_eq!(failed.len(), 1, "{:?}", outcome.stats.diagnostics);
    assert_eq!(failed[0].record_type.as_deref(), Some("LINE"));
    assert_eq!(outcome.stats.skipped_source_records, 1);

    assert!(read(&text, false).is_err(), "strict mode keeps failing");
}

/// A LINE with one unparsable group code per entry of `bad_codes`, each
/// replacing the code line of a coordinate pair.
fn damaged_line(handle: &str, owner: &str, bad_codes: &[&str]) -> String {
    let mut bad = bad_codes.iter();
    let mut coordinate = |code: i32, value: &str| match bad.next() {
        Some(bad_code) => format!("{bad_code}\n{value}\n"),
        None => group(code, value),
    };
    [
        group(0, "LINE"),
        group(5, handle),
        group(330, owner),
        group(100, "AcDbEntity"),
        group(8, "0"),
        group(100, "AcDbLine"),
        coordinate(10, "2.0"),
        coordinate(20, "0.0"),
        coordinate(30, "0.0"),
        group(11, "2.0"),
        group(21, "5.0"),
        group(31, "0.0"),
    ]
    .concat()
}

#[test]
fn a_record_with_several_bad_group_codes_is_skipped_whole() {
    let _guard = serial();
    let entities = [
        line("A1", "1F", 1.0),
        damaged_line("A2", "1F", &["1O", "2O", "3O"]),
        damaged_line("A3", "1F", &["1O"]), // directly after another damaged record
        line("A4", "1F", 4.0),
        damaged_line("A5", "1F", &["2O"]), // last record before ENDSEC
    ]
    .concat();
    let outcome = read(&drawing(&layout_records(), &entities), true).unwrap();

    assert_eq!(line_handles(&outcome.document), vec![0xA1, 0xA4]);
    let failed = outcome
        .stats
        .diagnostics
        .iter()
        .filter(|d| d.code == "entity-read-failed")
        .count();
    assert_eq!(failed, 3, "{:?}", outcome.stats.diagnostics);
    assert_eq!(outcome.stats.skipped_source_records, 3);
    assert!(
        outcome
            .stats
            .diagnostics
            .iter()
            .all(|d| d.code != "section-read-failed"),
        "the ENTITIES section itself must survive: {:?}",
        outcome.stats.diagnostics
    );
}

#[test]
fn a_non_utf8_group_code_line_keeps_the_stream_aligned() {
    let _guard = serial();
    let mut text = Vec::new();
    let before = drawing(
        &layout_records(),
        &[line("A1", "1F", 1.0), line("A2", "1F", 2.0)].concat(),
    );
    // Splice a LINE whose first coordinate code line is not UTF-8 text in
    // front of the closing ENDSEC of ENTITIES.
    let (head, tail) = before.split_at(before.rfind(&group(0, "ENDSEC")).unwrap());
    text.extend_from_slice(head.as_bytes());
    text.extend_from_slice(
        [
            group(0, "LINE"),
            group(5, "A3"),
            group(330, "1F"),
            group(8, "0"),
        ]
        .concat()
        .as_bytes(),
    );
    text.extend_from_slice(b"\xFF\xFE\n2.0\n");
    text.extend_from_slice(
        [group(20, "0.0"), group(11, "2.0"), group(21, "5.0")]
            .concat()
            .as_bytes(),
    );
    text.extend_from_slice(line("A4", "1F", 4.0).as_bytes());
    text.extend_from_slice(tail.as_bytes());

    let outcome = DxfReader::from_reader(Cursor::new(text))
        .unwrap()
        .with_configuration(DxfReaderConfiguration {
            failsafe: true,
            ..Default::default()
        })
        .read_with_stats()
        .unwrap();
    assert_eq!(
        line_handles(&outcome.document),
        vec![0xA1, 0xA2, 0xA4],
        "{:?}",
        outcome.stats.diagnostics
    );
    assert_eq!(outcome.stats.skipped_source_records, 1);
}

#[test]
fn the_diagnostic_locates_the_corrupt_record() {
    let _guard = serial();
    let entities = [line("A1", "1F", 1.0), corrupt_line("A3", "1F")].concat();
    let text = drawing(&layout_records(), &entities);
    let outcome = read(&text, true).unwrap();

    let failed = outcome
        .stats
        .diagnostics
        .iter()
        .find(|d| d.code == "entity-read-failed")
        .expect("entity-read-failed");
    assert_eq!(failed.stage, ReadStage::RecordStream);
    assert_eq!(failed.section.as_deref(), Some("ENTITIES"));
    assert_eq!(failed.record_handle, Some(0xA3));
    // The offset and line name the unparsable group code itself.
    let bad_line = text.lines().position(|l| l == "1O").unwrap() + 1;
    assert_eq!(failed.source_line, Some(bad_line));
    assert_eq!(failed.source_offset, Some(text.find("1O\n").unwrap() as u64));
    assert_eq!(failed.source_offset_basis.as_deref(), Some("file-byte"));
}

#[test]
fn block_membership_lists_keep_file_order() {
    let _guard = serial();
    let mut records = layout_records();
    for i in 0..50 {
        records.push_str(&block_record(&format!("{:X}", 0x100 + i), &format!("B{i}")));
    }
    let entities = [
        line("A1", "1F", 1.0),
        line("A2", "1E", 2.0),
        line("A3", "DEAD", 3.0), // unknown owner: routed to *Model_Space
        line("A4", "131", 4.0),  // block B49
        line("A5", "1F", 5.0),
        line("A6", "1E", 6.0),
    ]
    .concat();
    let outcome = read(&drawing(&records, &entities), true).unwrap();
    let document = &outcome.document;
    let members = |name: &str| -> Vec<u64> {
        document
            .block_records
            .get(name)
            .unwrap()
            .entity_handles
            .iter()
            .map(|h| h.value())
            .collect()
    };
    assert_eq!(members("*Model_Space"), vec![0xA1, 0xA3, 0xA5]);
    assert_eq!(members("*Paper_Space"), vec![0xA2, 0xA6]);
    assert_eq!(members("B49"), vec![0xA4]);
    let orphan = document
        .entities()
        .find(|e| e.common().handle.value() == 0xA3)
        .unwrap();
    assert_eq!(orphan.common().owner_handle.value(), 0x1F);
}

fn timed_read(block_count: usize, entity_count: usize) -> Duration {
    let mut records = layout_records();
    for i in 0..block_count {
        records.push_str(&block_record(&format!("{:X}", 0x10_0000 + i), &format!("B{i}")));
    }
    // Every LINE names an owner that is not a block record, which made the
    // upstream loader scan every block record for every entity.
    let mut entities = String::new();
    for i in 0..entity_count {
        entities.push_str(&line(&format!("{:X}", 0x20_0000 + i), "DEAD", (i % 100) as f64));
    }
    let text = drawing(&records, &entities);
    let started = Instant::now();
    let outcome = read(&text, true).unwrap();
    let elapsed = started.elapsed();
    assert_eq!(line_handles(&outcome.document).len(), entity_count);
    elapsed
}

/// Complexity class of DXF entity loading. Run in release:
/// `cargo test --release -p acadrust --test hardening_f17_dxf -- --ignored`.
#[test]
#[ignore]
fn entity_loading_is_linear_in_blocks_plus_entities() {
    let _guard = serial();
    let few_blocks = timed_read(5, 100_000);
    let many_blocks = timed_read(50_000, 100_000);
    assert!(many_blocks < Duration::from_secs(10), "{many_blocks:?}");
    assert!(
        many_blocks < few_blocks * 3 + Duration::from_secs(1),
        "50,000 blocks: {many_blocks:?}; 5 blocks: {few_blocks:?}"
    );
}
