use super::*;

#[derive(Clone, Debug)]
pub(crate) enum TreeModel {
    Virtual(VirtualObjectTree),
    Real(RealObjectTree),
}

impl TreeModel {
    pub(crate) fn from_state(state: &Result<AppState, String>, prefer_real: bool) -> Self {
        match state {
            Ok(state) => Self::from_app_state(state, prefer_real),
            Err(_) => Self::Virtual(VirtualObjectTree::new(VIRTUAL_TREE_ROWS)),
        }
    }

    pub(crate) fn from_app_state(state: &AppState, prefer_real: bool) -> Self {
        if prefer_real {
            if let Some(tree) = &state.panels.tree {
                return Self::Real(RealObjectTree::from_child_page(tree));
            }
        }
        Self::Virtual(VirtualObjectTree::new(VIRTUAL_TREE_ROWS))
    }

    pub(crate) fn is_real(&self) -> bool {
        matches!(self, Self::Real(_))
    }

    pub(crate) fn row_count(&self) -> usize {
        match self {
            Self::Virtual(tree) => tree.row_count(),
            Self::Real(tree) => tree.row_count(),
        }
    }

    pub(crate) fn initial_selected_row(&self) -> usize {
        match self {
            Self::Virtual(_) => 0,
            Self::Real(tree) => tree.initial_selected_row(),
        }
    }

    pub(crate) fn row_count_label(&self) -> String {
        match self {
            Self::Virtual(tree) => format!("{} rows", tree.row_count()),
            Self::Real(tree) => tree.row_count_label(),
        }
    }

    pub(crate) fn row_label(&self, row: usize) -> String {
        match self {
            Self::Virtual(tree) => tree.row_label(row),
            Self::Real(tree) => tree.row_label(row),
        }
    }

    pub(crate) fn row_layout_job(&self, row: usize, selected: bool) -> egui::text::LayoutJob {
        match self {
            Self::Virtual(tree) => tree.row_layout_job(row, selected),
            Self::Real(tree) => tree.row_layout_job(row, selected),
        }
    }

    pub(crate) fn real_row_depth(&self, row: usize) -> Option<usize> {
        match self {
            Self::Real(tree) => tree.row_depth(row),
            Self::Virtual(_) => None,
        }
    }

    pub(crate) fn real_row_tree_marker(&self, row: usize) -> Option<&'static str> {
        match self {
            Self::Real(tree) => tree.row_tree_marker(row),
            Self::Virtual(_) => None,
        }
    }

    pub(crate) fn real_row_page_index(&self, row: usize) -> Option<usize> {
        match self {
            Self::Real(tree) => tree.row_page_index(row),
            Self::Virtual(_) => None,
        }
    }

    pub(crate) fn real_row_for_page_index(&self, page_index: usize) -> Option<usize> {
        match self {
            Self::Real(tree) => tree.row_for_page_index(page_index),
            Self::Virtual(_) => None,
        }
    }

    pub(crate) fn real_page_root_row(&self) -> Option<usize> {
        match self {
            Self::Real(tree) => tree.page_root_row(),
            Self::Virtual(_) => None,
        }
    }

    pub(crate) fn real_first_page_content_stream_row(&self, page_row: usize) -> Option<usize> {
        match self {
            Self::Real(tree) => tree.first_page_content_stream_row(page_row),
            Self::Virtual(_) => None,
        }
    }

    pub(crate) fn real_page_content_candidate_rows(&self, page_row: usize) -> Vec<usize> {
        match self {
            Self::Real(tree) => tree.page_content_candidate_rows(page_row),
            Self::Virtual(_) => Vec::new(),
        }
    }

    pub(crate) fn ensure_real_page_child_row(
        &mut self,
        page_root_row: usize,
        summary: ObjectSummary,
    ) -> Option<usize> {
        match self {
            Self::Real(tree) => tree.ensure_page_child_row(page_root_row, summary),
            Self::Virtual(_) => None,
        }
    }

    pub(crate) fn real_row_visual_target(&self, row: usize) -> Option<RealVisualTarget> {
        match self {
            Self::Real(tree) => tree.row_visual_target(row),
            Self::Virtual(_) => None,
        }
    }

    pub(crate) fn reference_targets(&self, row: usize) -> [usize; 3] {
        match self {
            Self::Virtual(tree) => tree.reference_targets(row),
            Self::Real(_) => [row; 3],
        }
    }

    pub(crate) fn ensure_real_object_row(
        &mut self,
        doc: scalpel_core::DocumentId,
        object: ObjectId,
    ) -> usize {
        match self {
            Self::Real(tree) => tree.ensure_object_row(doc, object),
            Self::Virtual(_) => 0,
        }
    }

    pub(crate) fn ensure_real_search_hit_row(
        &mut self,
        doc: scalpel_core::DocumentId,
        hit: &ObjectSearchHit,
    ) -> Option<usize> {
        match self {
            Self::Real(tree) => tree.ensure_search_hit_row(doc, hit),
            Self::Virtual(_) => None,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RealObjectTree {
    pub(crate) rows: Vec<RealTreeRow>,
    pub(crate) root_children: Vec<ObjectSummary>,
    pub(crate) total: Option<usize>,
}

#[derive(Clone, Debug)]
pub(crate) struct RealTreeRow {
    pub(crate) summary: ObjectSummary,
    pub(crate) depth: usize,
    pub(crate) expanded: bool,
}

impl RealObjectTree {
    pub(crate) fn from_child_page(page: &scalpel_core::ChildPage<ObjectSummary>) -> Self {
        let mut rows = Vec::new();
        if let Some(first) = page.items.first() {
            rows.push(RealTreeRow {
                summary: ObjectSummary {
                    id: NodeId::DocumentRoot {
                        doc: first.id.document_id(),
                    },
                    kind: ObjectKind::Unknown,
                    label: "Document".to_string(),
                    preview: "PDF object graph".to_string(),
                    object: None,
                    has_children: true,
                    has_stream: false,
                    child_count: page.total,
                    byte_size_hint: None,
                    diagnostics: Vec::new(),
                },
                depth: 0,
                expanded: true,
            });
        }
        rows.extend(page.items.iter().cloned().map(|summary| RealTreeRow {
            summary,
            depth: 1,
            expanded: false,
        }));

        Self {
            rows,
            root_children: page.items.clone(),
            total: page.total,
        }
    }

    pub(crate) fn row_count(&self) -> usize {
        self.rows.len().max(1)
    }

    pub(crate) fn has_document_root_row(&self) -> bool {
        self.rows
            .first()
            .is_some_and(|row| matches!(row.summary.id, NodeId::DocumentRoot { .. }))
    }

    pub(crate) fn initial_selected_row(&self) -> usize {
        if self.has_document_root_row() && self.rows.len() > 1 {
            1
        } else {
            0
        }
    }

    pub(crate) fn row_count_label(&self) -> String {
        // Count top-level objects on both sides; `total` is the document's root
        // child count, so pairing it with the (deeper) materialized row count
        // would read as "15 loaded / 4 total".
        let loaded = self.root_children.len();
        let object_noun = |count: usize| if count == 1 { "object" } else { "objects" };
        match self.total {
            Some(total) if loaded < total => format!("{loaded} of {total} objects"),
            Some(total) => format!("{total} {}", object_noun(total)),
            None => format!("{loaded} {}", object_noun(loaded)),
        }
    }

    pub(crate) fn summary(&self, row: usize) -> Option<&ObjectSummary> {
        self.rows.get(row).map(|row| &row.summary)
    }

    pub(crate) fn page_root_summary(&self) -> Option<&ObjectSummary> {
        self.rows
            .iter()
            .find(|row| {
                matches!(
                    &row.summary.id,
                    NodeId::DictEntry { key, .. } if key == "Pages"
                ) || matches!(&row.summary.id, NodeId::PageRoot { .. })
            })
            .map(|row| &row.summary)
    }

    pub(crate) fn page_root_row(&self) -> Option<usize> {
        self.rows.iter().position(|row| {
            matches!(
                &row.summary.id,
                NodeId::DictEntry { key, .. } if key == "Pages"
            ) || matches!(&row.summary.id, NodeId::PageRoot { .. })
        })
    }

    pub(crate) fn row_label(&self, row: usize) -> String {
        self.summary(row)
            .map(summary_inline_text)
            .unwrap_or_else(|| "no real rows loaded".to_string())
    }

    pub(crate) fn row_depth(&self, row: usize) -> Option<usize> {
        self.rows.get(row).map(|row| row.depth)
    }

    pub(crate) fn row_tree_marker(&self, row: usize) -> Option<&'static str> {
        let row = self.rows.get(row)?;
        if !row.summary.has_children {
            return Some(" ");
        }
        if row.expanded {
            Some("-")
        } else {
            Some("+")
        }
    }

    pub(crate) fn row_page_index(&self, row: usize) -> Option<usize> {
        self.rows
            .get(row)
            .and_then(|row| page_index_for_node(&row.summary.id))
    }

    pub(crate) fn row_for_page_index(&self, page_index: usize) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| is_page_row_node_for_index(&row.summary.id, page_index))
    }

    pub(crate) fn first_page_content_stream_row(&self, page_row: usize) -> Option<usize> {
        let end = self.subtree_end(page_row);
        (page_row + 1..end).find(|row| {
            self.rows.get(*row).is_some_and(|tree_row| {
                is_page_content_stream_node(&tree_row.summary.id)
                    && summary_has_stream(&tree_row.summary)
            })
        })
    }

    pub(crate) fn page_content_candidate_rows(&self, page_row: usize) -> Vec<usize> {
        let end = self.subtree_end(page_row);
        (page_row + 1..end)
            .filter(|row| {
                self.rows.get(*row).is_some_and(|tree_row| {
                    is_page_content_stream_node(&tree_row.summary.id)
                        && (tree_row.summary.has_children || summary_has_stream(&tree_row.summary))
                })
            })
            .collect()
    }

    pub(crate) fn row_visual_target(&self, row: usize) -> Option<RealVisualTarget> {
        let row = self.rows.get(row)?;
        let page_index = page_index_for_node(&row.summary.id)?;
        let object = row.summary.object.or_else(|| row.summary.id.object_id())?;
        if !row.summary.has_stream && !matches!(row.summary.kind, ObjectKind::Stream) {
            return None;
        }
        Some(RealVisualTarget {
            page_index,
            object,
            allow_page_union: is_page_content_stream_node(&row.summary.id),
        })
    }

    pub(crate) fn row_layout_job(&self, row: usize, selected: bool) -> egui::text::LayoutJob {
        let Some(row) = self.rows.get(row) else {
            let mut job = egui::text::LayoutJob::default();
            job.append("no real rows loaded", 0.0, tree_text_format(theme().muted));
            return job;
        };
        let mut job = egui::text::LayoutJob::default();
        let primary = if selected {
            theme().selected_text
        } else {
            theme().text
        };
        let muted = if selected {
            theme().selected_text
        } else {
            theme().muted
        };
        let accent = if row.summary.diagnostics.is_empty() {
            muted
        } else {
            theme().warn_fg
        };
        job.append(
            tree_kind_badge_text(&row.summary),
            0.0,
            tree_text_format(accent),
        );
        job.append(" ", 0.0, tree_text_format(muted));
        job.append(&row.summary.label, 0.0, tree_text_format(primary));
        if let Some(count) = row.summary.child_count {
            job.append(&format!(" ({count})"), 0.0, tree_text_format(muted));
        }
        if let Some(object) = row.summary.object {
            let object_color = if selected {
                theme().selected_text
            } else {
                theme().accent
            };
            job.append(
                &format!(" [{} {} R]", object.num, object.gen),
                0.0,
                tree_text_format(object_color),
            );
        }
        if row.summary.has_stream {
            let stream_color = if selected {
                theme().selected_text
            } else {
                theme().operator
            };
            job.append("  stream", 0.0, tree_text_format(stream_color));
        }
        job
    }

    pub(crate) fn ensure_object_row(
        &mut self,
        doc: scalpel_core::DocumentId,
        object: ObjectId,
    ) -> usize {
        if let Some(index) = self
            .rows
            .iter()
            .position(|row| row.summary.object == Some(object))
        {
            return index;
        }
        let id = NodeId::XrefObject { doc, object };
        self.rows.push(RealTreeRow {
            summary: ObjectSummary {
                id,
                kind: ObjectKind::Unknown,
                label: format!("Object {}", object.num),
                preview: format!("{} {} R", object.num, object.gen),
                object: Some(object),
                has_children: true,
                has_stream: false,
                child_count: None,
                byte_size_hint: None,
                diagnostics: Vec::new(),
            },
            depth: if self.has_document_root_row() { 1 } else { 0 },
            expanded: false,
        });
        self.rows.len() - 1
    }

    pub(crate) fn row_for_node(&self, node: &NodeId) -> Option<usize> {
        self.rows.iter().position(|row| row.summary.id == *node)
    }

    pub(crate) fn ensure_search_hit_row(
        &mut self,
        doc: scalpel_core::DocumentId,
        hit: &ObjectSearchHit,
    ) -> Option<usize> {
        if let Some(node) = &hit.node {
            if let Some(index) = self.row_for_node(node) {
                return Some(index);
            }
            if let Some(object) = hit.object {
                if let Some(index) = self
                    .rows
                    .iter()
                    .position(|row| row.summary.object == Some(object))
                {
                    return Some(index);
                }
            }
            self.rows.push(RealTreeRow {
                summary: ObjectSummary {
                    id: node.clone(),
                    kind: ObjectKind::Unknown,
                    label: hit.label.clone(),
                    preview: hit.excerpt.clone(),
                    object: hit.object,
                    has_children: true,
                    has_stream: false,
                    child_count: None,
                    byte_size_hint: None,
                    diagnostics: Vec::new(),
                },
                depth: hit
                    .depth
                    .saturating_add(self.has_document_root_row() as usize)
                    .min(8),
                expanded: false,
            });
            return Some(self.rows.len() - 1);
        }

        hit.object.map(|object| self.ensure_object_row(doc, object))
    }

    pub(crate) fn ensure_page_child_row(
        &mut self,
        page_root_row: usize,
        summary: ObjectSummary,
    ) -> Option<usize> {
        if let Some(row) = self.row_for_node(&summary.id) {
            return Some(row);
        }
        let parent_depth = self.rows.get(page_root_row)?.depth;
        if let Some(parent) = self.rows.get_mut(page_root_row) {
            parent.expanded = true;
        }
        let insert_at = self.subtree_end(page_root_row);
        self.rows.insert(
            insert_at,
            RealTreeRow {
                summary,
                depth: parent_depth + 1,
                expanded: false,
            },
        );
        Some(insert_at)
    }

    pub(crate) fn subtree_end(&self, row: usize) -> usize {
        let Some(parent) = self.rows.get(row) else {
            return self.rows.len();
        };
        let parent_depth = parent.depth;
        let mut end = row + 1;
        while end < self.rows.len() && self.rows[end].depth > parent_depth {
            end += 1;
        }
        end
    }

    pub(crate) fn update_row_from_detail(&mut self, row: usize, detail: &ObjectDetail) {
        let Some(row) = self.rows.get_mut(row) else {
            return;
        };
        row.summary.kind = detail.kind.clone();
        row.summary.object = detail.object;
        row.summary.has_stream = detail.stream.is_some();
        row.summary.has_children =
            detail.dictionary_entries.is_some() || detail.array_entries.is_some();
        row.summary.child_count = detail
            .dictionary_entries
            .as_ref()
            .and_then(|entries| entries.total)
            .or_else(|| {
                detail
                    .array_entries
                    .as_ref()
                    .and_then(|entries| entries.total)
            });
        row.summary.diagnostics = detail.diagnostics.clone();
    }

    pub(crate) fn expand_row_from_detail(&mut self, row: usize, detail: &ObjectDetail) -> usize {
        let Some(parent) = self.rows.get_mut(row) else {
            return 0;
        };
        if parent.expanded {
            return 0;
        }
        parent.expanded = true;
        let child_depth = parent.depth + 1;

        let mut children = Vec::new();
        if let Some(entries) = &detail.dictionary_entries {
            children.extend(entries.items.iter().map(|entry| RealTreeRow {
                summary: entry.value.clone(),
                depth: child_depth,
                expanded: false,
            }));
        }
        if let Some(entries) = &detail.array_entries {
            children.extend(entries.items.iter().cloned().map(|summary| RealTreeRow {
                summary,
                depth: child_depth,
                expanded: false,
            }));
        }

        let inserted = children.len();
        self.rows.splice(row + 1..row + 1, children);
        inserted
    }

    pub(crate) fn expand_cached_document_root(&mut self, row: usize) -> usize {
        let Some(parent) = self.rows.get_mut(row) else {
            return 0;
        };
        if parent.expanded || !matches!(parent.summary.id, NodeId::DocumentRoot { .. }) {
            return 0;
        }
        parent.expanded = true;
        let child_depth = parent.depth + 1;
        let children = self
            .root_children
            .iter()
            .cloned()
            .map(|summary| RealTreeRow {
                summary,
                depth: child_depth,
                expanded: false,
            })
            .collect::<Vec<_>>();
        let inserted = children.len();
        self.rows.splice(row + 1..row + 1, children);
        inserted
    }

    pub(crate) fn collapse_expanded_rows_except_selected_path(
        &mut self,
        selected_id: &NodeId,
    ) -> Option<usize> {
        let selected_row = self.row_for_node(selected_id)?;
        let keep = self.selected_path_node_ids(selected_row);
        let mut row = self.rows.len();
        while row > 0 {
            row -= 1;
            if keep.contains(&self.rows[row].summary.id) {
                continue;
            }
            if self.rows[row].expanded {
                self.collapse_row(row);
            }
        }
        self.row_for_node(selected_id)
    }

    pub(crate) fn selected_path_node_ids(&self, selected_row: usize) -> HashSet<NodeId> {
        let mut keep = HashSet::new();
        let Some(selected) = self.rows.get(selected_row) else {
            return keep;
        };
        keep.insert(selected.summary.id.clone());
        let mut next_depth = selected.depth;
        for row in (0..selected_row).rev() {
            let candidate = &self.rows[row];
            if candidate.depth < next_depth {
                keep.insert(candidate.summary.id.clone());
                next_depth = candidate.depth;
                if next_depth == 0 {
                    break;
                }
            }
        }
        keep
    }

    pub(crate) fn collapse_row(&mut self, row: usize) -> usize {
        let Some(parent) = self.rows.get_mut(row) else {
            return 0;
        };
        if !parent.expanded || !parent.summary.has_children {
            return 0;
        }
        parent.expanded = false;
        let parent_depth = parent.depth;
        let end = self.rows[row + 1..]
            .iter()
            .position(|child| child.depth <= parent_depth)
            .map(|offset| row + 1 + offset)
            .unwrap_or(self.rows.len());
        let removed = end.saturating_sub(row + 1);
        if removed > 0 {
            self.rows.drain(row + 1..end);
        }
        removed
    }
}

#[derive(Clone, Debug)]
pub(crate) struct VirtualObjectTree {
    pub(crate) rows: usize,
}

impl VirtualObjectTree {
    pub(crate) fn new(rows: usize) -> Self {
        Self { rows }
    }

    pub(crate) fn row_count(&self) -> usize {
        self.rows + 1
    }

    pub(crate) fn row_label(&self, row: usize) -> String {
        if row == 0 {
            return "root / catalog".to_string();
        }
        format!(
            "obj {:06} 0 R  /FakeNode{}",
            row,
            row.wrapping_mul(31) % 997
        )
    }

    pub(crate) fn row_layout_job(&self, row: usize, selected: bool) -> egui::text::LayoutJob {
        let label = self.row_label(row);
        // Two-tone row: object id in primary text, the /Name in muted
        // (both selected foreground when selected) so the tree reads as structure,
        // not a flat dump.
        let id_color = if selected {
            theme().selected_text
        } else {
            theme().text
        };
        let name_color = if selected {
            theme().selected_text
        } else {
            theme().muted
        };
        let mut job = egui::text::LayoutJob::default();
        match label.split_once("  ") {
            Some((id_part, name_part)) => {
                job.append(id_part, 0.0, tree_text_format(id_color));
                job.append(&format!("  {name_part}"), 0.0, tree_text_format(name_color));
            }
            None => job.append(&label, 0.0, tree_text_format(id_color)),
        }
        job
    }

    pub(crate) fn reference_targets(&self, row: usize) -> [usize; 3] {
        let base = row.max(1);
        [
            (base * 3 % self.rows).max(1),
            (base * 11 % self.rows).max(1),
            (base * 101 % self.rows).max(1),
        ]
    }
}

#[derive(Clone, Debug)]
pub(crate) struct LargeStreamModel {
    pub(crate) offset: usize,
    pub(crate) selection_offset: usize,
    pub(crate) selection_len: usize,
    pub(crate) hex_text: String,
    pub(crate) hex_text_offset: usize,
    pub(crate) selected_hex_text: Option<String>,
}

impl Default for LargeStreamModel {
    fn default() -> Self {
        let mut model = Self {
            offset: 0,
            selection_offset: 0,
            selection_len: 256,
            hex_text: String::new(),
            hex_text_offset: usize::MAX,
            selected_hex_text: None,
        };
        model.sync_hex_window();
        model
    }
}

impl LargeStreamModel {
    pub(crate) fn hex_window(&self) -> String {
        self.hex_dump(self.offset, HEX_WINDOW_BYTES)
    }

    pub(crate) fn sync_hex_window(&mut self) {
        if self.hex_text_offset != self.offset {
            self.reset_hex_window();
            self.selected_hex_text = None;
        }
    }

    pub(crate) fn reset_hex_window(&mut self) {
        self.hex_text = self.hex_window();
        self.hex_text_offset = self.offset;
    }

    pub(crate) fn escaped_copy_text(&self) -> EscapedText {
        if let Some(selected) = self
            .selected_hex_text
            .as_ref()
            .filter(|selected| !selected.is_empty())
        {
            return escape_pdf_text(selected, EgressFormat::Markdown, COPY_LIMIT_BYTES);
        }
        self.escaped_range_selection()
    }

    pub(crate) fn copy_source_label(&self) -> &'static str {
        if self
            .selected_hex_text
            .as_ref()
            .is_some_and(|selected| !selected.is_empty())
        {
            "selected hex text"
        } else {
            "byte range"
        }
    }

    pub(crate) fn escaped_range_selection(&self) -> EscapedText {
        let text = self.hex_dump(
            self.selection_offset,
            self.selection_len.min(COPY_LIMIT_BYTES),
        );
        let max_bytes = COPY_LIMIT_BYTES.min(text.len());
        let mut escaped = escape_pdf_text(&text, EgressFormat::Markdown, max_bytes);
        escaped.truncated |= self.selection_len > COPY_LIMIT_BYTES;
        escaped
    }

    pub(crate) fn hex_dump(&self, offset: usize, len: usize) -> String {
        let start = offset.min(STREAM_TOTAL_BYTES);
        let end = start.saturating_add(len).min(STREAM_TOTAL_BYTES);
        let mut out = String::new();
        let mut line = start;
        while line < end {
            let line_end = (line + 16).min(end);
            out.push_str(&format!("{line:08x}  "));
            for index in line..line_end {
                out.push_str(&format!("{:02x} ", fake_stream_byte(index)));
            }
            for _ in line_end..line + 16 {
                out.push_str("   ");
            }
            out.push(' ');
            for index in line..line_end {
                let byte = fake_stream_byte(index);
                out.push(if byte.is_ascii_graphic() {
                    byte as char
                } else {
                    '.'
                });
            }
            out.push('\n');
            line = line_end;
        }
        out
    }
}

pub(crate) fn fake_stream_byte(index: usize) -> u8 {
    (index as u8).wrapping_mul(31).wrapping_add(17)
}
