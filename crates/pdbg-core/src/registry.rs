use crate::dto::*;
use crate::wire;
use pdbg_shim::raw;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChildContainer {
    Dictionary,
    Array,
}

#[derive(Clone, Debug, Default)]
pub struct NodeTokenRegistry {
    by_token: HashMap<NodeTokenKey, NodeId>,
    by_node: HashMap<NodeId, raw::pdbg_node_id>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct NodeTokenKey {
    document_id: u64,
    path_token: u64,
}

impl NodeTokenRegistry {
    pub fn register(&mut self, raw_node: raw::pdbg_node_id, public_node: NodeId) {
        if raw_node.kind == raw::pdbg_node_kind::PDBG_NODE_PATH_TOKEN {
            self.by_token.insert(
                NodeTokenKey {
                    document_id: raw_node.document_id,
                    path_token: raw_node.path_token,
                },
                public_node.clone(),
            );
        }
        self.by_node.insert(public_node, raw_node);
    }

    pub fn raw_for(&self, public_node: &NodeId) -> Option<raw::pdbg_node_id> {
        self.by_node.get(public_node).copied()
    }

    pub fn resolve_node(&self, raw_node: &raw::pdbg_node_id) -> Option<NodeId> {
        match raw_node.kind {
            raw::pdbg_node_kind::PDBG_NODE_PATH_TOKEN => self
                .by_token
                .get(&NodeTokenKey {
                    document_id: raw_node.document_id,
                    path_token: raw_node.path_token,
                })
                .cloned(),
            _ => direct_node_id(raw_node),
        }
    }

    /// Converts one borrowed C list entry into a public object summary and records
    /// the private path-token mapping for later reverse lookups.
    ///
    /// # Safety
    ///
    /// `entry` must come from a live `pdbg_node_list` owned by the shim. All
    /// pointer fields referenced by the entry must remain valid for the duration
    /// of this call. The method copies every string/diagnostic before returning.
    pub unsafe fn convert_child_entry(
        &mut self,
        parent: &NodeId,
        entry: &raw::pdbg_dict_entry,
        container: ChildContainer,
        range: ChildRange,
        list_index: usize,
    ) -> ObjectSummary {
        let doc = parent.document_id();
        let id = match container {
            ChildContainer::Dictionary => NodeId::DictEntry {
                doc,
                parent: Box::new(parent.clone()),
                key: wire::copy_c_string(entry.key),
            },
            ChildContainer::Array => NodeId::ArrayEntry {
                doc,
                parent: Box::new(parent.clone()),
                index: range.offset + list_index,
            },
        };

        self.register(entry.node, id.clone());
        let diagnostics = wire::diagnostic_list(entry.diagnostics, &|node| self.resolve_node(node));

        ObjectSummary {
            id,
            kind: wire::object_kind(entry.object_kind),
            label: wire::copy_c_string(entry.label),
            preview: wire::copy_c_string(entry.preview),
            object: wire::optional_object_id(entry.object, entry.has_object),
            has_children: entry.has_children != 0,
            has_stream: entry.has_stream != 0,
            child_count: (entry.has_child_count != 0).then_some(entry.child_count),
            byte_size_hint: (entry.has_byte_size_hint != 0).then_some(entry.byte_size_hint),
            diagnostics,
        }
    }
}

fn direct_node_id(raw_node: &raw::pdbg_node_id) -> Option<NodeId> {
    let doc = DocumentId(raw_node.document_id);
    Some(match raw_node.kind {
        raw::pdbg_node_kind::PDBG_NODE_DOCUMENT_ROOT => NodeId::DocumentRoot { doc },
        raw::pdbg_node_kind::PDBG_NODE_TRAILER => NodeId::Trailer { doc },
        raw::pdbg_node_kind::PDBG_NODE_CATALOG => NodeId::Catalog { doc },
        raw::pdbg_node_kind::PDBG_NODE_XREF_ROOT => NodeId::XrefRoot { doc },
        raw::pdbg_node_kind::PDBG_NODE_XREF_OBJECT => NodeId::XrefObject {
            doc,
            object: wire::object_id(raw_node.object),
        },
        raw::pdbg_node_kind::PDBG_NODE_PAGE_ROOT => NodeId::PageRoot { doc },
        raw::pdbg_node_kind::PDBG_NODE_PAGE => NodeId::Page {
            doc,
            index: raw_node.page_index as usize,
        },
        raw::pdbg_node_kind::PDBG_NODE_INDIRECT_REF => NodeId::IndirectRef {
            doc,
            object: wire::object_id(raw_node.object),
        },
        raw::pdbg_node_kind::PDBG_NODE_STREAM => NodeId::Stream {
            doc,
            object: wire::object_id(raw_node.object),
            decoded: raw_node.decoded != 0,
        },
        raw::pdbg_node_kind::PDBG_NODE_RESOURCE_GROUP => NodeId::ResourceGroup {
            doc,
            page_index: raw_node.page_index as usize,
            group: wire::resource_group(raw_node.resource_group),
        },
        raw::pdbg_node_kind::PDBG_NODE_PATH_TOKEN => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::ptr;

    #[test]
    fn dict_child_registers_public_node_and_reverse_lookup() {
        let parent = NodeId::XrefObject {
            doc: DocumentId(1),
            object: ObjectId { num: 10, gen: 0 },
        };
        let key = CString::new("Resources").unwrap();
        let label = CString::new("Resources").unwrap();
        let preview = CString::new("<< /Font ... >>").unwrap();
        let entry = fake_entry(1, 77, key.as_ptr(), label.as_ptr(), preview.as_ptr());
        let mut registry = NodeTokenRegistry::default();

        let summary = unsafe {
            registry.convert_child_entry(
                &parent,
                &entry,
                ChildContainer::Dictionary,
                ChildRange {
                    offset: 0,
                    limit: 10,
                },
                0,
            )
        };

        assert!(matches!(summary.id, NodeId::DictEntry { .. }));
        assert_eq!(registry.raw_for(&summary.id).unwrap().path_token, 77);
        assert_eq!(registry.resolve_node(&entry.node), Some(summary.id));
    }

    #[test]
    fn array_child_uses_range_offset_plus_list_position() {
        let parent = NodeId::XrefObject {
            doc: DocumentId(1),
            object: ObjectId { num: 20, gen: 0 },
        };
        let key = CString::new("").unwrap();
        let label = CString::new("item").unwrap();
        let preview = CString::new("42").unwrap();
        let entry = fake_entry(1, 9, key.as_ptr(), label.as_ptr(), preview.as_ptr());
        let mut registry = NodeTokenRegistry::default();

        let summary = unsafe {
            registry.convert_child_entry(
                &parent,
                &entry,
                ChildContainer::Array,
                ChildRange {
                    offset: 40,
                    limit: 10,
                },
                2,
            )
        };

        assert!(matches!(summary.id, NodeId::ArrayEntry { index: 42, .. }));
    }

    #[test]
    fn unknown_diagnostic_path_token_does_not_leak_public_node() {
        let registry = NodeTokenRegistry::default();
        let message = CString::new("broken child").unwrap();
        let diag = raw::pdbg_diagnostic {
            severity: raw::pdbg_diagnostic_severity::PDBG_DIAG_WARNING,
            code: raw::pdbg_diagnostic_code::PDBG_DIAG_REPAIR_WARNING,
            message: message.as_ptr().cast_mut(),
            node: path_token_node(1, 999),
            has_node: 1,
            page_index: 0,
            has_page_index: 0,
            object: raw::pdbg_object_id { num: 4, gen: 0 },
            has_object: 1,
        };

        let converted = unsafe { wire::diagnostic(&diag, &|node| registry.resolve_node(node)) };
        assert_eq!(converted.code, DiagnosticCode::RepairWarning);
        assert_eq!(converted.node, None);
        assert_eq!(converted.object, Some(ObjectId { num: 4, gen: 0 }));
    }

    fn fake_entry(
        document_id: u64,
        token: u64,
        key: *const i8,
        label: *const i8,
        preview: *const i8,
    ) -> raw::pdbg_dict_entry {
        raw::pdbg_dict_entry {
            key: key.cast_mut(),
            node: path_token_node(document_id, token),
            object_kind: raw::pdbg_object_kind::PDBG_OBJECT_DICT,
            object: raw::pdbg_object_id { num: 1, gen: 0 },
            has_object: 1,
            label: label.cast_mut(),
            preview: preview.cast_mut(),
            has_children: 1,
            has_stream: 0,
            child_count: 2,
            has_child_count: 1,
            byte_size_hint: 100,
            has_byte_size_hint: 1,
            max_diagnostic_severity: raw::pdbg_diagnostic_severity::PDBG_DIAG_WARNING,
            diagnostic_count: 0,
            diagnostics: ptr::null_mut(),
        }
    }

    fn path_token_node(document_id: u64, token: u64) -> raw::pdbg_node_id {
        raw::pdbg_node_id {
            document_id,
            kind: raw::pdbg_node_kind::PDBG_NODE_PATH_TOKEN,
            object: raw::pdbg_object_id { num: 0, gen: 0 },
            has_object: 0,
            page_index: 0,
            path_token: token,
            decoded: 0,
            resource_group: raw::pdbg_resource_group::PDBG_RESOURCE_FONTS,
        }
    }
}
