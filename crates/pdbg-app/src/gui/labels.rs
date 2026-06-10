use super::*;

pub(crate) fn tree_text_format(color: Color32) -> egui::TextFormat {
    egui::TextFormat {
        font_id: mono_font_id(DENSE_ROW_FONT_SIZE),
        color,
        ..Default::default()
    }
}

pub(crate) fn file_chip_label(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_string()
}

pub(crate) fn display_file_chip_label(path: &str) -> String {
    escape_pdf_text(
        &file_chip_label(path),
        EgressFormat::PlainText,
        PATH_DISPLAY_MAX_BYTES,
    )
    .text
}

pub(crate) fn display_path_hover(path: &str) -> String {
    escape_pdf_text(path, EgressFormat::PlainText, PATH_DISPLAY_MAX_BYTES).text
}

pub(crate) fn kind_badge_text(kind: &ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Null => "null",
        ObjectKind::Bool => "bool",
        ObjectKind::Int => "int",
        ObjectKind::Real => "real",
        ObjectKind::Name => "name",
        ObjectKind::String => "str",
        ObjectKind::Array => "[]",
        ObjectKind::Dict => "<>",
        ObjectKind::IndirectRef => "ref",
        ObjectKind::Stream => "stm",
        ObjectKind::Page => "page",
        ObjectKind::XrefEntry => "xref",
        ObjectKind::Trailer => "trl",
        ObjectKind::Metadata => "meta",
        ObjectKind::Unknown => "?",
    }
}

pub(crate) fn tree_kind_badge_text(summary: &ObjectSummary) -> &'static str {
    if (matches!(summary.kind, ObjectKind::Page) && is_page_list_summary(summary))
        || is_xref_object_summary(summary)
    {
        return kind_badge_text(&ObjectKind::Dict);
    }
    kind_badge_text(&summary.kind)
}

pub(crate) fn is_page_list_summary(summary: &ObjectSummary) -> bool {
    matches!(&summary.id, NodeId::Page { .. })
        || matches!(&summary.id, NodeId::ArrayEntry { parent, .. } if is_page_root_node(parent))
}

pub(crate) fn is_xref_object_summary(summary: &ObjectSummary) -> bool {
    matches!(&summary.id, NodeId::XrefObject { .. })
        || (matches!(summary.kind, ObjectKind::XrefEntry) && summary.object.is_some())
}

pub(crate) fn object_kind_label(kind: &ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Null => "null",
        ObjectKind::Bool => "bool",
        ObjectKind::Int => "int",
        ObjectKind::Real => "real",
        ObjectKind::Name => "name",
        ObjectKind::String => "string",
        ObjectKind::Array => "array",
        ObjectKind::Dict => "dictionary",
        ObjectKind::IndirectRef => "indirect_ref",
        ObjectKind::Stream => "stream",
        ObjectKind::Page => "page",
        ObjectKind::XrefEntry => "xref_entry",
        ObjectKind::Trailer => "trailer",
        ObjectKind::Metadata => "metadata",
        ObjectKind::Unknown => "unknown",
    }
}

pub(crate) fn type_badge(ui: &mut egui::Ui, kind: &ObjectKind) {
    egui::Frame::new()
        .fill(theme().chip_bg)
        .stroke(egui::Stroke::new(1.0, theme().border))
        .corner_radius(3)
        .inner_margin(egui::Margin::symmetric(5, 2))
        .show(ui, |ui| {
            ui.label(
                RichText::new(kind_badge_text(kind))
                    .monospace()
                    .color(theme().muted),
            );
        });
}

pub(crate) fn summary_inline_text(summary: &ObjectSummary) -> String {
    let mut out = String::new();
    let mut reference = None;
    if let Some(object) = summary.object {
        let text = format!("{} {} R", object.num, object.gen);
        out.push_str(&format!("[{text}] "));
        reference = Some(text);
    }
    let preview = summary.preview.trim();
    let body = if preview.is_empty() {
        summary.label.trim()
    } else {
        preview
    };
    // Skip a body that only repeats the bracketed reference ("[7 0 R] 7 0 R").
    if reference.as_deref() == Some(body) {
        out.pop();
    } else {
        out.push_str(body);
    }
    if summary.has_stream {
        out.push_str(" stream");
    }
    out
}

pub(crate) fn object_value_preview(value: &ObjectValue, fallback: &str) -> String {
    match value {
        ObjectValue::Null => "null".to_string(),
        ObjectValue::Bool(value) => value.to_string(),
        ObjectValue::Int(value) => value.to_string(),
        ObjectValue::Real(value) => value.to_string(),
        ObjectValue::Name(value) => format!("/{value}"),
        ObjectValue::StringBytes {
            bytes,
            decoded_text,
            ..
        } => decoded_text
            .clone()
            .unwrap_or_else(|| format!("{} bytes", bytes.len())),
        ObjectValue::IndirectRef(object) => format!("{} {} R", object.num, object.gen),
        ObjectValue::Container => fallback.to_string(),
        ObjectValue::Unknown => fallback.to_string(),
    }
}

pub(crate) fn detail_reference_targets(detail: &ObjectDetail) -> Vec<ObjectId> {
    let mut out = Vec::new();
    push_value_reference(&mut out, &detail.value);
    if let Some(entries) = &detail.dictionary_entries {
        for entry in &entries.items {
            if let Some(object) = entry.value.object {
                push_unique_object(&mut out, object);
            }
        }
    }
    if let Some(entries) = &detail.array_entries {
        for entry in &entries.items {
            if let Some(object) = entry.object {
                push_unique_object(&mut out, object);
            }
        }
    }
    out
}

pub(crate) fn object_search_status_label(
    result: Option<&ObjectSearchResult>,
    error: Option<&str>,
    running: bool,
) -> String {
    if running {
        return "Searching…".to_string();
    }
    if error.is_some() {
        return "Search failed".to_string();
    }
    match result {
        Some(result) => search_match_label(result.hits.len(), result.truncated),
        None => String::new(),
    }
}

pub(crate) fn object_search_field_label(field: ObjectSearchField) -> &'static str {
    match field {
        ObjectSearchField::ObjectNumber => "object",
        ObjectSearchField::DictionaryKey => "key",
        ObjectSearchField::NameObject => "name",
        ObjectSearchField::ScalarPreview => "scalar",
        ObjectSearchField::Label => "label",
    }
}

pub(crate) fn object_search_hit_summary(hit: &ObjectSearchHit) -> String {
    let target = hit
        .object
        .map(object_ref_text)
        .unwrap_or_else(|| hit.label.clone());
    let excerpt = hit.excerpt.trim();
    if excerpt.is_empty() {
        format!(
            "{target}  {}  {}",
            object_search_field_label(hit.matched_field),
            hit.label
        )
    } else {
        format!(
            "{target}  {}  {excerpt}",
            object_search_field_label(hit.matched_field)
        )
    }
}

pub(crate) fn node_label_for_hit(hit: &ObjectSearchHit) -> String {
    hit.node
        .as_ref()
        .map(node_breadcrumb)
        .unwrap_or_else(|| hit.label.clone())
}

pub(crate) fn virtual_search_hit_row(hit: &ObjectSearchHit, row_count: usize) -> Option<usize> {
    let row = usize::try_from(hit.object?.num).ok()?;
    (row < row_count).then_some(row)
}

pub(crate) fn object_ref_text(object: ObjectId) -> String {
    format!("{} {} R", object.num, object.gen)
}

pub(crate) fn visual_object_attribution_text(hit: Option<&PreviewVisualHit>) -> Option<String> {
    hit.and_then(|hit| hit.object.map(object_ref_text))
}

pub(crate) fn text_search_status_label(
    result: Option<&TextSearchResult>,
    error: Option<&str>,
    running: bool,
) -> String {
    if running {
        return "Searching…".to_string();
    }
    if error.is_some() {
        return "Search failed".to_string();
    }
    match result {
        Some(result) => search_match_label(result.hits.len(), result.truncated),
        None => String::new(),
    }
}

pub(crate) fn text_search_hit_summary(hit: &TextSearchHit) -> String {
    let excerpt = escape_pdf_text(&hit.excerpt, EgressFormat::PlainText, COPY_LIMIT_BYTES)
        .text
        .chars()
        .map(|ch| {
            if matches!(ch, '\n' | '\r' | '\t') {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>();
    format!(
        "Page {} · span {}  {}",
        hit.page_index + 1,
        hit.span_index,
        excerpt
    )
}

pub(crate) fn text_search_hit_hover(hit: &TextSearchHit) -> String {
    let trust = if hit.untrusted {
        "untrusted"
    } else {
        "trusted"
    };
    match &hit.bbox {
        Some(bbox) => format!(
            "page {} span {} {trust} bbox {:.1},{:.1} {:.1}x{:.1}",
            hit.page_index + 1,
            hit.span_index,
            bbox.x,
            bbox.y,
            bbox.width,
            bbox.height
        ),
        None => format!(
            "page {} span {} {trust}",
            hit.page_index + 1,
            hit.span_index
        ),
    }
}

pub(crate) fn text_hits_same_position(left: &TextSearchHit, right: &TextSearchHit) -> bool {
    left.page_index == right.page_index
        && left.span_index == right.span_index
        && left.excerpt == right.excerpt
}

pub(crate) fn text_hit_bbox_image_rect(
    hit: &TextSearchHit,
    image_rect: egui::Rect,
    render_width: u32,
    render_height: u32,
    zoom: f32,
    rotation_degrees: i32,
) -> Option<egui::Rect> {
    if rotation_degrees != 0 || render_width == 0 || render_height == 0 || zoom <= 0.0 {
        return None;
    }
    let bbox = hit.bbox.as_ref()?;
    page_bbox_image_rect(bbox, image_rect, render_width, render_height, zoom)
}

pub(crate) fn visual_hit_bbox_image_rect(
    hit: &PreviewVisualHit,
    image_rect: egui::Rect,
    render_width: u32,
    render_height: u32,
    zoom: f32,
    rotation_degrees: i32,
) -> Option<egui::Rect> {
    if rotation_degrees != 0 || render_width == 0 || render_height == 0 || zoom <= 0.0 {
        return None;
    }
    page_bbox_image_rect(&hit.bbox, image_rect, render_width, render_height, zoom)
}

pub(crate) fn page_bbox_image_rect(
    bbox: &PageRect,
    image_rect: egui::Rect,
    render_width: u32,
    render_height: u32,
    zoom: f32,
) -> Option<egui::Rect> {
    if bbox.width <= 0.0 || bbox.height <= 0.0 {
        return None;
    }
    let left = image_rect.left() + (bbox.x * zoom / render_width as f32) * image_rect.width();
    let top = image_rect.top() + (bbox.y * zoom / render_height as f32) * image_rect.height();
    let right = image_rect.left()
        + ((bbox.x + bbox.width) * zoom / render_width as f32) * image_rect.width();
    let bottom = image_rect.top()
        + ((bbox.y + bbox.height) * zoom / render_height as f32) * image_rect.height();
    Some(egui::Rect::from_min_max(
        egui::pos2(left, top),
        egui::pos2(right, bottom),
    ))
}

pub(crate) fn push_value_reference(out: &mut Vec<ObjectId>, value: &ObjectValue) {
    if let ObjectValue::IndirectRef(object) = value {
        push_unique_object(out, *object);
    }
}

pub(crate) fn push_unique_object(out: &mut Vec<ObjectId>, object: ObjectId) {
    if !out.contains(&object) {
        out.push(object);
    }
}

pub(crate) fn child_page_detail(total: Option<usize>, loaded: usize) -> String {
    match total {
        Some(total) => format!("{loaded} loaded / {total} total"),
        None => format!("{loaded} loaded"),
    }
}
pub(crate) fn draw_diagnostic_card(ui: &mut egui::Ui, diagnostic: &pdbg_core::DiagnosticSummary) {
    let color = theme().severity_fg(&diagnostic.severity);
    egui::Frame::new()
        .fill(theme().severity_bg(&diagnostic.severity))
        .stroke(egui::Stroke::new(1.0, color))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.colored_label(
                color,
                RichText::new(format!(
                    "{} {} {}",
                    diagnostic.severity.as_public_str(),
                    diagnostic.code.as_public_str(),
                    diagnostic.message
                ))
                .monospace(),
            );
        });
}

pub(crate) fn node_breadcrumb(id: &NodeId) -> String {
    id.to_serialized()
        .segments
        .iter()
        .map(segment_label)
        .collect::<Vec<_>>()
        .join("/")
}

pub(crate) fn page_index_for_node(id: &NodeId) -> Option<usize> {
    match id {
        NodeId::Page { index, .. } => Some(*index),
        NodeId::ResourceGroup { page_index, .. } => Some(*page_index),
        NodeId::ArrayEntry { parent, index, .. } if is_page_root_node(parent) => Some(*index),
        NodeId::DictEntry { parent, .. } | NodeId::ArrayEntry { parent, .. } => {
            page_index_for_node(parent)
        }
        _ => None,
    }
}

pub(crate) fn is_page_row_node_for_index(id: &NodeId, page_index: usize) -> bool {
    match id {
        NodeId::Page { index, .. } => *index == page_index,
        NodeId::ArrayEntry { parent, index, .. } if is_page_root_node(parent) => {
            *index == page_index
        }
        _ => false,
    }
}

pub(crate) fn is_page_content_stream_node(id: &NodeId) -> bool {
    match id {
        NodeId::DictEntry { parent, key, .. } if key == "Contents" => {
            page_index_for_node(parent).is_some()
        }
        NodeId::ArrayEntry { parent, .. } => is_page_content_stream_node(parent),
        _ => false,
    }
}

pub(crate) fn summary_has_stream(summary: &ObjectSummary) -> bool {
    summary.has_stream || matches!(summary.kind, ObjectKind::Stream)
}

pub(crate) fn is_page_root_node(id: &NodeId) -> bool {
    matches!(id, NodeId::PageRoot { .. })
        || matches!(id, NodeId::DictEntry { key, .. } if key == "Pages")
}

pub(crate) fn segment_label(segment: &NodePathSegment) -> String {
    match segment {
        NodePathSegment::DocumentRoot => "Root".to_string(),
        NodePathSegment::Trailer => "Trailer".to_string(),
        NodePathSegment::Catalog => "Catalog".to_string(),
        NodePathSegment::XrefRoot => "Xref".to_string(),
        NodePathSegment::XrefObject(object) => format!("{} {} R", object.num, object.gen),
        NodePathSegment::PageRoot => "Pages".to_string(),
        NodePathSegment::Page { index } => format!("Page[{index}]"),
        NodePathSegment::DictKey(key) => key.clone(),
        NodePathSegment::ArrayIndex(index) => format!("[{index}]"),
        NodePathSegment::IndirectRef(object) => format!("{} {} R", object.num, object.gen),
        NodePathSegment::Stream { object, decoded } => {
            let mode = if *decoded { "decoded" } else { "raw" };
            format!("Stream({} {} R,{mode})", object.num, object.gen)
        }
        NodePathSegment::ResourceGroup { page_index, group } => {
            format!("Page[{page_index}]/{}", group.as_public_str())
        }
    }
}
