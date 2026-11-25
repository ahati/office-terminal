//! PDF Destination resolution for outline items and hyperlinks.

use hayro_syntax::object::{Array, Dict, MaybeRef, Name, Number, Object, ObjectIdentifier, String as PdfString};
use hayro_syntax::xref::XRef;
use std::collections::HashMap;

/// Resolve the 0-based page index targeted by an outline or link dictionary.
pub fn resolve_destination(
    item: &Dict<'_>,
    page_map: &HashMap<ObjectIdentifier, usize>,
    root: &Dict<'_>,
    xref: &XRef,
) -> Option<usize> {
    // 1. Direct /Dest
    if let Some(res) = resolve_dest_value(item, page_map, root, xref, b"Dest") {
        return Some(res);
    }

    // 2. Action /A << /S /GoTo /D ... >>
    if let Some(action) = item.get::<Dict<'_>>(b"A") {
        let is_goto = action
            .get::<Name<'_>>(b"S")
            .map(|s| s.as_ref() == b"GoTo")
            .unwrap_or(false);
        if is_goto {
            if let Some(res) = resolve_dest_value(&action, page_map, root, xref, b"D") {
                return Some(res);
            }
        }
    }

    None
}

fn resolve_dest_value(
    dict: &Dict<'_>,
    page_map: &HashMap<ObjectIdentifier, usize>,
    root: &Dict<'_>,
    xref: &XRef,
    key: &[u8],
) -> Option<usize> {
    if let Some(arr) = dict.get::<Array<'_>>(key) {
        return resolve_array_dest(&arr, page_map);
    }
    if let Some(num) = dict.get::<Number>(key) {
        return Some(num.as_i64().max(0) as usize);
    }
    if let Some(name) = dict.get::<Name<'_>>(key) {
        return resolve_named_dest(name.as_ref(), page_map, root, xref);
    }
    if let Some(str_val) = dict.get::<PdfString<'_>>(key) {
        return resolve_named_dest(str_val.as_bytes(), page_map, root, xref);
    }
    if let Some(dest_dict) = dict.get::<Dict<'_>>(key) {
        if let Some(arr) = dest_dict.get::<Array<'_>>(b"D") {
            return resolve_array_dest(&arr, page_map);
        }
    }
    None
}

fn resolve_array_dest(arr: &Array<'_>, page_map: &HashMap<ObjectIdentifier, usize>) -> Option<usize> {
    let mut iter = arr.raw_iter();
    let first = iter.next()?;
    match first {
        MaybeRef::Ref(obj_ref) => page_map.get(&obj_ref.into()).copied(),
        MaybeRef::NotRef(obj) => match obj {
            Object::Dict(dict) => dict.obj_id().and_then(|id| page_map.get(&id).copied()),
            Object::Number(num) => Some(num.as_i64().max(0) as usize),
            _ => None,
        },
    }
}

fn resolve_name_tree_node(
    node: &Dict<'_>,
    name: &[u8],
    page_map: &HashMap<ObjectIdentifier, usize>,
    xref: &XRef,
    depth: usize,
) -> Option<usize> {
    if depth > 10 {
        return None;
    }

    // Prune subtree using /Limits [least greatest] when present (ISO 32000-1 §7.7.4)
    if let Some(limits) = node.get::<Array<'_>>(b"Limits") {
        let mut lim_iter = limits.raw_iter();
        if let (Some(min_elem), Some(max_elem)) = (lim_iter.next(), lim_iter.next()) {
            let min_bytes = match &min_elem {
                MaybeRef::NotRef(Object::String(s)) => Some(s.as_bytes()),
                MaybeRef::NotRef(Object::Name(n)) => Some(n.as_ref()),
                _ => None,
            };
            let max_bytes = match &max_elem {
                MaybeRef::NotRef(Object::String(s)) => Some(s.as_bytes()),
                MaybeRef::NotRef(Object::Name(n)) => Some(n.as_ref()),
                _ => None,
            };
            if let (Some(min_b), Some(max_b)) = (min_bytes, max_bytes) {
                if name < min_b || name > max_b {
                    return None;
                }
            }
        }
    }

    // Leaf node: /Names array
    if let Some(names_arr) = node.get::<Array<'_>>(b"Names") {
        let mut iter = names_arr.raw_iter();
        while let Some(k) = iter.next() {
            let is_match = match k {
                MaybeRef::NotRef(Object::String(s)) => s.as_bytes() == name,
                MaybeRef::NotRef(Object::Name(n)) => n.as_ref() == name,
                _ => false,
            };
            if let Some(val) = iter.next() {
                if is_match {
                    match val {
                        MaybeRef::Ref(obj_ref) => {
                            let id: ObjectIdentifier = obj_ref.into();
                            if let Some(dest_arr) = xref.get::<Array<'_>>(id) {
                                return resolve_array_dest(&dest_arr, page_map);
                            }
                            // FrameMaker-style producers store named destinations
                            // as indirect `<< /D [page /XYZ ...] >>` dictionaries.
                            if let Some(dest_dict) = xref.get::<Dict<'_>>(id) {
                                if let Some(arr) = dest_dict.get::<Array<'_>>(b"D") {
                                    return resolve_array_dest(&arr, page_map);
                                }
                            }
                        }
                        MaybeRef::NotRef(Object::Array(arr)) => {
                            return resolve_array_dest(&arr, page_map);
                        }
                        MaybeRef::NotRef(Object::Dict(dict)) => {
                            if let Some(arr) = dict.get::<Array<'_>>(b"D") {
                                return resolve_array_dest(&arr, page_map);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Intermediate node: /Kids array
    if let Some(kids_arr) = node.get::<Array<'_>>(b"Kids") {
        for kid in kids_arr.raw_iter() {
            let kid_dict = match kid {
                MaybeRef::Ref(r) => xref.get::<Dict<'_>>(r.into()),
                MaybeRef::NotRef(Object::Dict(d)) => Some(d),
                _ => None,
            };
            if let Some(kd) = kid_dict {
                if let Some(page) = resolve_name_tree_node(&kd, name, page_map, xref, depth + 1) {
                    return Some(page);
                }
            }
        }
    }

    None
}

fn resolve_named_dest(
    name: &[u8],
    page_map: &HashMap<ObjectIdentifier, usize>,
    root: &Dict<'_>,
    xref: &XRef,
) -> Option<usize> {
    // 1. Check root /Dests dict
    if let Some(dests) = root.get::<Dict<'_>>(b"Dests") {
        if let Some(arr) = dests.get::<Array<'_>>(name) {
            return resolve_array_dest(&arr, page_map);
        }
        if let Some(dict) = dests.get::<Dict<'_>>(name) {
            if let Some(arr) = dict.get::<Array<'_>>(b"D") {
                return resolve_array_dest(&arr, page_map);
            }
        }
    }

    // 2. Check root /Names /Dests name tree
    if let Some(names) = root.get::<Dict<'_>>(b"Names") {
        if let Some(dests) = names.get::<Dict<'_>>(b"Dests") {
            if let Some(page) = resolve_name_tree_node(&dests, name, page_map, xref, 0) {
                return Some(page);
            }
        }
    }

    None
}
