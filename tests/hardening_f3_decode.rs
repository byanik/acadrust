//! F3: record layouts that upstream 0.6.4 misreads (the dependency name
//! payload of `AcDbImpBlockRefPathObjectId`, persistent sub-entity ids, and
//! R2007+ VX table records) are consumed exactly, to the bit.
mod hardening_common;

use acadrust::io::dwg::dwg_reference_type::DwgReferenceType;
use acadrust::io::dwg::dwg_stream_readers::bit_reader::DwgBitReader;
use acadrust::io::dwg::dwg_stream_readers::merged_reader::DwgMergedReader;
use acadrust::io::dwg::dwg_stream_readers::object_reader::associative::read_associative_data;
use acadrust::io::dwg::dwg_stream_readers::object_reader::tables::read_vx_table_record;
use acadrust::io::dwg::dwg_stream_writers::bit_writer::DwgBitWriter;
use acadrust::io::dwg::dwg_version::DwgVersion;
use acadrust::objects::AssociativeData;
use acadrust::types::DxfVersion;
use hardening_common::serial;

fn writer(version: DxfVersion) -> DwgBitWriter {
    DwgBitWriter::new(DwgVersion::from_dxf_version(version).unwrap(), version)
}

/// Single-stream reader over everything `writer` wrote; its end is the last
/// written bit, so `main_remaining_bits() == 0` means "consumed exactly".
fn reader(writer: DwgBitWriter, version: DxfVersion) -> DwgMergedReader {
    let bits = writer.position_in_bits();
    let main = DwgBitReader::new(
        writer.into_bytes(),
        DwgVersion::from_dxf_version(version).unwrap(),
        version,
    );
    let mut reader = DwgMergedReader::from_readers(main, None, None, version);
    reader.set_main_data_end(bits);
    reader
}

/// ASSOCDEPENDENCY fields, in stream order.
fn write_dependency(w: &mut DwgBitWriter, name: Option<&str>, path: &[u64]) {
    w.write_bit_short(2); // class version
    w.write_bit_long(0); // status
    w.write_bit(true); // read dependency
    w.write_bit(false); // write dependency
    w.write_bit(true); // attached to object
    w.write_bit(false); // delegating to owning action
    w.write_bit_long(7); // order
    w.write_handle(DwgReferenceType::SoftPointer, 0x51); // dependent on
    w.write_bit(name.is_some());
    if let Some(name) = name {
        w.write_variable_text(name);
        if name == "AcDbImpBlockRefPathObjectId" {
            w.write_bit_short(0); // payload version
            w.write_bit_long(path.len() as i32);
            for element in path {
                w.write_raw_short(1); // element tag, 0x0001 in every traced record
                w.write_handle(DwgReferenceType::SoftPointer, *element);
            }
        }
    }
    w.write_handle(DwgReferenceType::SoftPointer, 0x61); // read dependency
    w.write_handle(DwgReferenceType::HardPointer, 0x62); // node
    w.write_handle(DwgReferenceType::HardOwnership, 0x63); // dependency body
    w.write_bit_long(9); // dependency body id
}

#[test]
fn imp_block_ref_path_payload_is_consumed() {
    let _guard = serial();
    let version = DxfVersion::AC1027;
    let mut w = writer(version);
    write_dependency(&mut w, Some("AcDbImpBlockRefPathObjectId"), &[0x71, 0x72, 0x73]);
    let mut r = reader(w, version);
    let data = read_associative_data(
        &mut r,
        "ACDBASSOCDEPENDENCY",
        DwgVersion::from_dxf_version(version).unwrap(),
        version,
    );
    let Some(AssociativeData::Dependency(dependency)) = data else {
        panic!("not a dependency: {data:?}");
    };
    assert_eq!(dependency.name.as_deref(), Some("AcDbImpBlockRefPathObjectId"));
    assert_eq!(dependency.read_dependency.value(), 0x61);
    assert_eq!(dependency.node.value(), 0x62);
    assert_eq!(dependency.dependency_body.value(), 0x63);
    assert_eq!(dependency.dependency_body_id, 9);
    assert!(r.check_error().is_ok());
    assert_eq!(r.main_remaining_bits(), 0);
}

fn geom_dependency(version: DxfVersion, subent: impl Fn(&mut DwgBitWriter)) -> DwgMergedReader {
    let mut w = writer(version);
    write_dependency(&mut w, None, &[]);
    w.write_bit_short(1); // geom dependency class version
    w.write_bit(true); // enabled
    subent(&mut w);
    w.write_bit(true); // dependent on compound object
    reader(w, version)
}

#[test]
fn r2013_subent_id_code_2_is_bs_bl_bl_b() {
    let _guard = serial();
    let version = DxfVersion::AC1032;
    let mut r = geom_dependency(version, |w| {
        w.write_bit(true); // leading flag
        w.write_bit_long(2); // class code
        w.write_bit_short(1);
        w.write_bit_long(40);
        w.write_bit_long(41);
        w.write_bit(true);
    });
    let data = read_associative_data(
        &mut r,
        "ACDBASSOCGEOMDEPENDENCY",
        DwgVersion::from_dxf_version(version).unwrap(),
        version,
    );
    let Some(AssociativeData::GeomDependency(geom)) = data else {
        panic!("not a geom dependency: {data:?}");
    };
    let subent = &geom.persistent_subent;
    assert!(subent.leading_flag);
    assert_eq!(subent.class_code, 2);
    assert_eq!(subent.values, vec![1, 40, 41, 1]);
    assert!(subent.dependent_on_compound_object);
    assert!(r.check_error().is_ok());
    assert_eq!(r.main_remaining_bits(), 0);
}

#[test]
fn r2007_subent_id_is_named_by_text() {
    let _guard = serial();
    let version = DxfVersion::AC1021;
    let mut r = geom_dependency(version, |w| {
        w.write_variable_text("AcDbAssocEdgePersSubentId");
        w.write_bit_short(1);
        w.write_bit_long(12);
        w.write_bit_long(13);
    });
    let data = read_associative_data(
        &mut r,
        "ACDBASSOCGEOMDEPENDENCY",
        DwgVersion::from_dxf_version(version).unwrap(),
        version,
    );
    let Some(AssociativeData::GeomDependency(geom)) = data else {
        panic!("not a geom dependency: {data:?}");
    };
    let subent = &geom.persistent_subent;
    assert_eq!(subent.class_name, "AcDbAssocEdgePersSubentId");
    assert_eq!(subent.class_code, 3);
    assert_eq!(subent.values, vec![1, 12, 13]);
    assert!(subent.dependent_on_compound_object);
    assert!(r.check_error().is_ok());
    assert_eq!(r.main_remaining_bits(), 0);
}

fn read_geom(
    r: &mut DwgMergedReader,
    version: DxfVersion,
) -> acadrust::objects::AssocGeomDependency {
    let data = read_associative_data(
        r,
        "ACDBASSOCGEOMDEPENDENCY",
        DwgVersion::from_dxf_version(version).unwrap(),
        version,
    );
    let Some(AssociativeData::GeomDependency(geom)) = data else {
        panic!("not a geom dependency: {data:?}");
    };
    geom
}

#[test]
fn r2013_subent_id_code_1_has_no_fields() {
    let _guard = serial();
    let version = DxfVersion::AC1027;
    let mut r = geom_dependency(version, |w| {
        w.write_bit(false); // leading flag
        w.write_bit_long(1); // class code
    });
    let geom = read_geom(&mut r, version);
    let subent = &geom.persistent_subent;
    assert!(!subent.leading_flag);
    assert_eq!(subent.class_code, 1);
    assert_eq!(subent.class_name, "AcDbAssocSingleEdgePersSubentId");
    assert!(subent.values.is_empty());
    assert!(subent.dependent_on_compound_object);
    assert!(r.check_error().is_ok());
    assert_eq!(r.main_remaining_bits(), 0);
}

#[test]
fn r2013_subent_id_code_3_is_bs_bl_bl() {
    let _guard = serial();
    let version = DxfVersion::AC1032;
    let mut r = geom_dependency(version, |w| {
        w.write_bit(true); // leading flag
        w.write_bit_long(3); // class code
        w.write_bit_short(1);
        w.write_bit_long(21);
        w.write_bit_long(22);
    });
    let geom = read_geom(&mut r, version);
    let subent = &geom.persistent_subent;
    assert!(subent.leading_flag);
    assert_eq!(subent.class_code, 3);
    assert_eq!(subent.values, vec![1, 21, 22]);
    assert!(subent.dependent_on_compound_object);
    assert!(r.check_error().is_ok());
    assert_eq!(r.main_remaining_bits(), 0);
}

#[test]
fn unknown_subent_codes_keep_the_greedy_loop() {
    let _guard = serial();
    // R2013+: a class code the fork has no layout for.
    let version = DxfVersion::AC1032;
    let mut r = geom_dependency(version, |w| {
        w.write_bit(false);
        w.write_bit_long(9);
        w.write_bit_long(5);
        w.write_bit_long(6);
    });
    let geom = read_geom(&mut r, version);
    let subent = &geom.persistent_subent;
    assert_eq!(subent.class_code, 9);
    assert_eq!(subent.values, vec![5, 6]);
    assert!(subent.dependent_on_compound_object);
    assert!(r.check_error().is_ok());
    assert_eq!(r.main_remaining_bits(), 0);

    // R2007-R2010: a class name the fork has no layout for keeps its name.
    let version = DxfVersion::AC1024;
    let mut r = geom_dependency(version, |w| {
        w.write_variable_text("AcDbAssocSomeOtherPersSubentId");
        w.write_bit_long(7);
        w.write_bit_long(8);
    });
    let geom = read_geom(&mut r, version);
    let subent = &geom.persistent_subent;
    assert_eq!(subent.class_name, "AcDbAssocSomeOtherPersSubentId");
    assert_eq!(subent.class_code, -1);
    assert_eq!(subent.values, vec![7, 8]);
    assert!(subent.dependent_on_compound_object);
    assert!(r.check_error().is_ok());
    assert_eq!(r.main_remaining_bits(), 0);
}

#[test]
fn other_dependency_names_carry_no_payload() {
    let _guard = serial();
    let version = DxfVersion::AC1027;
    let mut w = writer(version);
    write_dependency(&mut w, Some("AcDbSomeOtherName"), &[]);
    let mut r = reader(w, version);
    let data = read_associative_data(
        &mut r,
        "ACDBASSOCDEPENDENCY",
        DwgVersion::from_dxf_version(version).unwrap(),
        version,
    );
    let Some(AssociativeData::Dependency(dependency)) = data else {
        panic!("not a dependency: {data:?}");
    };
    assert_eq!(dependency.name.as_deref(), Some("AcDbSomeOtherName"));
    assert_eq!(dependency.read_dependency.value(), 0x61);
    assert_eq!(dependency.dependency_body_id, 9);
    assert!(r.check_error().is_ok());
    assert_eq!(r.main_remaining_bits(), 0);
}

#[test]
fn an_oversize_path_length_invalidates_the_record_without_looping() {
    let _guard = serial();
    let version = DxfVersion::AC1027;
    let mut w = writer(version);
    w.write_bit_short(2);
    w.write_bit_long(0);
    for flag in [true, false, true, false] {
        w.write_bit(flag);
    }
    w.write_bit_long(7);
    w.write_handle(DwgReferenceType::SoftPointer, 0x51);
    w.write_bit(true);
    w.write_variable_text("AcDbImpBlockRefPathObjectId");
    w.write_bit_short(0);
    w.write_bit_long(i32::MAX); // path length: far more than the record holds
    let mut r = reader(w, version);
    let _ = read_associative_data(
        &mut r,
        "ACDBASSOCDEPENDENCY",
        DwgVersion::from_dxf_version(version).unwrap(),
        version,
    );
    assert!(r.check_error().is_err(), "an impossible count must be reported");
}

fn vx_record(version: DxfVersion, r2007_flags: bool) -> DwgMergedReader {
    let mut w = writer(version);
    w.write_variable_text("*VX1");
    if r2007_flags {
        w.write_bit_short(256); // xrefindex+1 with the resolved bit
    } else {
        w.write_bit(false); // 64-flag
        w.write_bit_short(256); // xrefindex+1
        w.write_bit(true); // xref dependent
    }
    w.write_bit(true); // is on
    w.write_handle(DwgReferenceType::HardPointer, 0x81); // xref block
    w.write_handle(DwgReferenceType::SoftPointer, 0x82); // viewport
    w.write_handle(DwgReferenceType::HardPointer, 0x83); // previous entry
    reader(w, version)
}

#[test]
fn r2007_vx_record_uses_the_bs_only_xref_flags() {
    let _guard = serial();
    let version = DxfVersion::AC1032;
    let mut r = vx_record(version, true);
    let vx = read_vx_table_record(&mut r, DwgVersion::from_dxf_version(version).unwrap());
    assert_eq!(vx.name, "*VX1");
    assert!(vx.is_xref_resolved);
    assert!(vx.is_on);
    assert_eq!((vx.xref_handle, vx.viewport, vx.previous_entry), (0x81, 0x82, 0x83));
    assert!(r.check_error().is_ok());
    assert_eq!(r.main_remaining_bits(), 0);
}

#[test]
fn r2000_vx_record_keeps_the_three_field_flags() {
    let _guard = serial();
    let version = DxfVersion::AC1015;
    let mut r = vx_record(version, false);
    let vx = read_vx_table_record(&mut r, DwgVersion::from_dxf_version(version).unwrap());
    assert!(!vx.is_xref_reference);
    assert!(vx.is_xref_resolved);
    assert!(vx.is_xref_dependent);
    assert!(vx.is_on);
    assert_eq!((vx.xref_handle, vx.viewport, vx.previous_entry), (0x81, 0x82, 0x83));
    assert!(r.check_error().is_ok());
    assert_eq!(r.main_remaining_bits(), 0);
}
