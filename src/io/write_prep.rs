//! Output-copy repairs shared by the DXF and DWG writers: document-model fixes (two-way
//! references, stale table keys, active viewport elevation) with no file-format encoding.
//! Moved out of `dwg::dwg_writer` (this fork) so the DXF writer builds without the DWG
//! writer (`writer-dxf` without `writer-dwg`).

use crate::document::CadDocument;
use crate::types::Handle;

pub(crate) fn prepare_active_viewport_elevation(document: &mut std::borrow::Cow<'_, CadDocument>) {
    let elevation = document.header.elevation;
    let needs_elevation = elevation != 0.0 && document.vports.iter().any(|viewport| {
        viewport.name.eq_ignore_ascii_case("*ACTIVE") && viewport.ucs_elevation == 0.0
    });
    if needs_elevation {
        // CAD hosts activate the VPORT's UCS elevation when loading the file,
        // so a default zero viewport would mask the nonzero header setting.
        for viewport in document.to_mut().vports.iter_mut() {
            if viewport.name.eq_ignore_ascii_case("*ACTIVE") && viewport.ucs_elevation == 0.0 {
                viewport.ucs_elevation = elevation;
            }
        }
    }
}


/// Repair relationships that are stored in both directions in a DWG database.
/// The caller's document stays unchanged; corrections exist only in the output
/// copy.
pub(crate) fn prepare_database_references(document: &mut std::borrow::Cow<'_, CadDocument>) {
    use crate::entities::EntityType;
    use crate::objects::ObjectType;
    use std::collections::{HashMap, HashSet};

    let mut missing_hatch_reactors = Vec::new();
    let mut invalid_hatches = Vec::new();
    let mut table_repairs = Vec::new();
    let mut table_style_repairs = Vec::new();
    let mut mline_repairs = Vec::new();
    let mut underlay_reactors = Vec::new();
    for entity in document.entities() {
        match entity {
            EntityType::Hatch(hatch) => {
                if !hatch.is_associative {
                    continue;
                }
                let boundaries: Vec<Handle> = hatch
                    .paths
                    .iter()
                    .flat_map(|path| path.boundary_handles.iter().copied())
                    .collect();
                if boundaries.is_empty()
                    || boundaries
                        .iter()
                        .any(|handle| document.get_entity(*handle).is_none())
                {
                    invalid_hatches.push(hatch.common.handle);
                    continue;
                }
                for boundary_handle in boundaries {
                    if document
                        .get_entity(boundary_handle)
                        .is_some_and(|boundary| {
                            !boundary.common().reactors.contains(&hatch.common.handle)
                        })
                    {
                        missing_hatch_reactors.push((boundary_handle, hatch.common.handle));
                    }
                }
            }
            EntityType::Table(table) => {
                if table
                    .table_style_handle
                    .is_none_or(|handle| handle.is_null())
                {
                    let style = document
                        .objects
                        .get(&document.header.named_objects_dict_handle)
                        .and_then(|object| match object {
                            ObjectType::Dictionary(root) => root.get("ACAD_TABLESTYLE"),
                            _ => None,
                        })
                        .and_then(|handle| document.objects.get(&handle))
                        .and_then(|object| match object {
                            ObjectType::Dictionary(styles) => styles
                                .get(&document.header.current_table_style_name)
                                .or_else(|| styles.get("Standard")),
                            _ => None,
                        })
                        .filter(|handle| {
                            matches!(
                                document.objects.get(handle),
                                Some(ObjectType::TableStyle(_))
                            )
                        });
                    if let Some(style) = style {
                        table_style_repairs.push((table.common.handle, style));
                    }
                }
                let resolved = table
                    .block_record_handle
                    .filter(|handle| !handle.is_null())
                    .and_then(|handle| {
                        document
                            .block_records
                            .iter()
                            .find(|record| record.handle == handle)
                    })
                    .or_else(|| document.block_records.get(&table.block_name));
                if resolved.is_none_or(|record| {
                    table.block_record_handle != Some(record.handle)
                        || table.block_name != record.name
                        || record.handle.is_null()
                        || record.block_entity_handle.is_null()
                        || record.block_end_handle.is_null()
                }) {
                    table_repairs.push((
                        table.common.handle,
                        resolved
                            .map(|record| record.name.clone())
                            .unwrap_or_else(|| table.block_name.clone()),
                    ));
                }
            }
            EntityType::MLine(mline)
                if mline.style_handle.is_none_or(|handle| handle.is_null()) =>
            {
                let style = document
                    .objects
                    .iter()
                    .find_map(|(handle, object)| match object {
                        ObjectType::MLineStyle(style)
                            if style.name.eq_ignore_ascii_case(&mline.style_name) =>
                        {
                            Some(*handle)
                        }
                        _ => None,
                    })
                    .unwrap_or(document.header.current_multiline_style_handle);
                if !style.is_null() {
                    mline_repairs.push((mline.common.handle, style));
                }
            }
            EntityType::Underlay(underlay) => {
                if let Some(ObjectType::UnderlayDefinition(definition)) =
                    document.objects.get(&underlay.definition_handle)
                {
                    if !definition.reactors.contains(&underlay.common.handle) {
                        underlay_reactors
                            .push((underlay.definition_handle, underlay.common.handle));
                    }
                }
            }
            _ => {}
        }
    }

    let root_handle = document.header.named_objects_dict_handle;
    let layout_dictionary = document
        .objects
        .get(&root_handle)
        .and_then(|object| match object {
            ObjectType::Dictionary(dictionary) => dictionary.get("ACAD_LAYOUT"),
            _ => None,
        })
        .unwrap_or(document.header.acad_layout_dict_handle);
    let layout_names: HashMap<Handle, String> = document
        .objects
        .iter()
        .filter_map(|(handle, object)| match object {
            ObjectType::Layout(layout) => Some((*handle, layout.name.clone())),
            _ => None,
        })
        .collect();
    let layout_dictionary_needs_repair = document
        .objects
        .get(&layout_dictionary)
        .and_then(|object| match object {
            ObjectType::Dictionary(dictionary) => Some(dictionary),
            _ => None,
        })
        .is_some_and(|dictionary| {
            let targets: HashSet<Handle> = dictionary
                .entries
                .iter()
                .filter_map(|(key, handle)| {
                    layout_names
                        .get(handle)
                        .filter(|name| *name == key)
                        .map(|_| *handle)
                })
                .collect();
            dictionary.entries.len() != targets.len() || targets.len() != layout_names.len()
        });

    if missing_hatch_reactors.is_empty()
        && invalid_hatches.is_empty()
        && table_repairs.is_empty()
        && table_style_repairs.is_empty()
        && mline_repairs.is_empty()
        && underlay_reactors.is_empty()
        && !layout_dictionary_needs_repair
    {
        return;
    }

    let output = document.to_mut();
    for (handle, style) in table_style_repairs {
        if let Some(EntityType::Table(table)) = output.get_entity_mut(handle) {
            table.table_style_handle = Some(style);
        }
    }
    for (definition, reactor) in underlay_reactors {
        if let Some(ObjectType::UnderlayDefinition(definition)) =
            output.objects.get_mut(&definition)
        {
            if !definition.reactors.contains(&reactor) {
                definition.reactors.push(reactor);
            }
        }
    }
    for (handle, style) in mline_repairs {
        if let Some(EntityType::MLine(mline)) = output.get_entity_mut(handle) {
            mline.style_handle = Some(style);
        }
    }
    if !table_repairs.is_empty() {
        output.synchronize_handle_allocator();
        for (table_handle, requested_name) in table_repairs {
            let existing = if !requested_name.is_empty() {
                output.block_records.get(&requested_name).cloned()
            } else {
                None
            };
            let (block_record_handle, block_name) = if let Some(mut record) = existing {
                if record.name.starts_with("*T") {
                    record.flags.anonymous = true;
                }
                if record.handle.is_null() {
                    record.handle = output.allocate_handle();
                }
                if record.block_entity_handle.is_null() {
                    record.block_entity_handle = output.allocate_handle();
                }
                if record.block_end_handle.is_null() {
                    record.block_end_handle = output.allocate_handle();
                }
                let result = (record.handle, record.name.clone());
                output.block_records.add_or_replace(record);
                result
            } else {
                let name = if requested_name.is_empty() {
                    let mut index = 1;
                    while output.block_records.get(&format!("*T{index}")).is_some() {
                        index += 1;
                    }
                    format!("*T{index}")
                } else {
                    requested_name
                };
                let mut record = crate::tables::BlockRecord::new(&name);
                record.handle = output.allocate_handle();
                record.block_entity_handle = output.allocate_handle();
                record.block_end_handle = output.allocate_handle();
                record.flags.anonymous = name.starts_with('*');
                let handle = record.handle;
                output
                    .block_records
                    .add(record)
                    .expect("new table block name is unique");
                (handle, name)
            };
            if let Some(EntityType::Table(table)) = output.get_entity_mut(table_handle) {
                table.block_record_handle = Some(block_record_handle);
                table.block_name = block_name;
            }
        }
    }
    for (boundary_handle, hatch_handle) in missing_hatch_reactors {
        if let Some(boundary) = output.get_entity_mut(boundary_handle) {
            if !boundary.common().reactors.contains(&hatch_handle) {
                boundary.common_mut().reactors.push(hatch_handle);
            }
        }
    }
    for hatch_handle in invalid_hatches {
        if let Some(EntityType::Hatch(hatch)) = output.get_entity_mut(hatch_handle) {
            hatch.is_associative = false;
            for path in &mut hatch.paths {
                path.boundary_handles.clear();
            }
        }
    }

    if layout_dictionary_needs_repair {
        if let Some(ObjectType::Dictionary(dictionary)) = output.objects.get_mut(&layout_dictionary)
        {
            dictionary.entries.clear();
            let mut layouts: Vec<_> = layout_names.into_iter().collect();
            layouts.sort_by_key(|(handle, _)| handle.value());
            let mut names = HashSet::new();
            for (handle, name) in layouts {
                if names.insert(name.clone()) {
                    dictionary.entries.push((name, handle));
                }
            }
        }
        for object in output.objects.values_mut() {
            if let ObjectType::Layout(layout) = object {
                layout.owner = layout_dictionary;
            }
        }
    }
}


/// Re-key symbol-table entries that were renamed in place.
///
/// Tables key entries by the name captured at insertion, so a caller that
/// assigns `layer.name` through `layers.iter_mut()` leaves the entry reachable
/// only under its old name. The entity writer resolves an entity's layer by
/// name, so every entity on that layer would be written with a NULL layer hard
/// pointer — a required reference — and produces an invalid drawing (issue
/// #80). Repair the output copy only; the caller's document is untouched.
///
/// Shared with the DXF writer: the stale key desyncs the same name lookups
/// there, leaving entities pointing at a layer name the LAYER table no longer
/// defines.
pub(crate) fn prepare_table_keys(document: &mut std::borrow::Cow<'_, CadDocument>) {
    if !document.has_stale_table_keys() {
        return;
    }
    document.to_mut().resync_table_keys();
}
