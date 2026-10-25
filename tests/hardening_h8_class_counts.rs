//! H8: the counts inside a DXF class object (DATATABLE columns x rows, nested
//! ACDBVIEWREP sketches and references, associative dependency lists) are
//! bounded by the values the object actually carries, so a few kilobytes can
//! no longer ask for billions of default items.
mod hardening_common;

use acadrust::objects::{AssociativeData, ClassObject, ClassObjectData, ObjectType};
use acadrust::{CadDocument, DxfReader, DxfReaderConfiguration};
use hardening_common::{largest_alloc_during, no_panic, serial};
use std::time::{Duration, Instant};

/// Reads a DXF built by the test in failsafe mode on a worker thread; fails
/// after 60 seconds instead of hanging and on any request of 16 MiB or more.
fn read(name: &'static str, text: String) -> CadDocument {
    let worker = std::thread::spawn(move || {
        largest_alloc_during(|| {
            no_panic(name, || {
                DxfReader::from_reader(std::io::Cursor::new(text.into_bytes()))
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

fn objects_section(body: &str) -> String {
    format!("0\nSECTION\n2\nOBJECTS\n{body}0\nENDSEC\n0\nEOF\n")
}

fn class_data(document: &CadDocument) -> Vec<&ClassObjectData> {
    document
        .objects
        .values()
        .filter_map(|object| match object {
            ObjectType::ClassObject(ClassObject { data, .. }) => Some(data),
            _ => None,
        })
        .collect()
}

#[test]
fn datatable_rows_are_bounded_by_the_values_present() {
    let _guard = serial();
    // 1000 columns of 100000 rows each (10^8 cells, several GB before the
    // fix), described by 1000 column types and nothing else.
    let body = format!(
        "0\nDATATABLE\n5\n2A\n100\nAcDbDataTable\n70\n0\n90\n1000\n91\n100000\n1\nT\n{}",
        "92\n1\n".repeat(1000)
    );
    let document = read("datatable", objects_section(&body));
    let tables: Vec<_> = class_data(&document)
        .into_iter()
        .filter_map(|data| match data {
            ClassObjectData::DataTable(table) => Some(table),
            _ => None,
        })
        .collect();
    assert_eq!(tables.len(), 1);
    let cells: usize = tables[0].columns.iter().map(|column| column.rows.len()).sum();
    assert!(tables[0].columns.len() <= 1000);
    assert!(cells <= 1010, "{cells} cells from 1004 values");
}

#[test]
fn a_well_formed_datatable_keeps_every_cell() {
    let _guard = serial();
    let body = "0\nDATATABLE\n5\n2A\n100\nAcDbDataTable\n70\n0\n90\n2\n91\n3\n1\nT\n\
                92\n1\n2\nA\n93\n1\n93\n2\n93\n3\n92\n2\n2\nB\n40\n0.5\n40\n1.5\n40\n2.5\n";
    let document = read("datatable ok", objects_section(body));
    let table = class_data(&document)
        .into_iter()
        .find_map(|data| match data {
            ClassObjectData::DataTable(table) => Some(table),
            _ => None,
        })
        .unwrap();
    let rows: Vec<usize> = table.columns.iter().map(|column| column.rows.len()).collect();
    assert_eq!(rows, vec![3, 3]);
    assert_eq!(table.columns[0].rows[2].integer, 3);
    assert_eq!(table.columns[1].rows[1].real, 1.5);
}

#[test]
fn nested_view_rep_counts_are_bounded_by_the_values_present() {
    let _guard = serial();
    // Every 90 says 100000: sketches x references would be 10^10 items.
    let body = format!(
        "0\nACDBVIEWREP\n5\n2B\n100\nAcDbViewRep\n{}",
        "90\n100000\n".repeat(64)
    );
    let document = read("viewrep", objects_section(&body));
    let items: usize = class_data(&document)
        .into_iter()
        .filter_map(|data| match data {
            ClassObjectData::ViewRep(rep) => Some(
                rep.sketches.len()
                    + rep.sketches.iter().map(|sketch| sketch.references.len()).sum::<usize>(),
            ),
            _ => None,
        })
        .sum();
    assert!(items <= 64, "{items} items from 64 values");
}

#[test]
fn associative_counts_are_bounded_by_the_entries_left() {
    let _guard = serial();
    let body = "0\nASSOCNETWORK\n5\n2C\n100\nAcDbAssocAction\n90\n2\n90\n0\n330\n0\n360\n0\n\
                90\n0\n90\n0\n90\n100000\n100\nAcDbAssocNetwork\n90\n0\n90\n0\n90\n100000\n";
    let document = read("assocnetwork", objects_section(body));
    let network = document
        .objects
        .values()
        .find_map(|object| match object {
            ObjectType::Associative(object) => match &object.data {
                AssociativeData::Network(network) => Some(network),
                _ => None,
            },
            _ => None,
        })
        .expect("ASSOCNETWORK is read");
    assert!(network.action.dependencies.len() <= 4);
    assert!(network.actions.len() <= 4);
    assert!(network.actions.capacity() <= 4);
}
