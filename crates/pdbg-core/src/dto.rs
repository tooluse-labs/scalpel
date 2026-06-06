use crate::egress::{append_bounded_json_string, append_json_string};

pub const PUBLIC_SCHEMA_VERSION: u32 = 1;
pub const DIAGNOSTIC_SCHEMA_VERSION: u32 = 1;
const DIAGNOSTIC_JSON_FIELD_LIMIT_BYTES: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct DocumentId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ObjectId {
    pub num: i32,
    pub gen: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SerializedNodeId {
    pub schema_version: u32,
    pub doc: DocumentId,
    pub segments: Vec<NodePathSegment>,
    pub object: Option<ObjectId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NodePathSegment {
    DocumentRoot,
    Trailer,
    Catalog,
    XrefRoot,
    XrefObject(ObjectId),
    PageRoot,
    Page {
        index: usize,
    },
    DictKey(String),
    ArrayIndex(usize),
    IndirectRef(ObjectId),
    Stream {
        object: ObjectId,
        decoded: bool,
    },
    ResourceGroup {
        page_index: usize,
        group: ResourceGroup,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum NodeId {
    DocumentRoot {
        doc: DocumentId,
    },
    Trailer {
        doc: DocumentId,
    },
    Catalog {
        doc: DocumentId,
    },
    XrefRoot {
        doc: DocumentId,
    },
    XrefObject {
        doc: DocumentId,
        object: ObjectId,
    },
    PageRoot {
        doc: DocumentId,
    },
    Page {
        doc: DocumentId,
        index: usize,
    },
    DictEntry {
        doc: DocumentId,
        parent: Box<NodeId>,
        key: String,
    },
    ArrayEntry {
        doc: DocumentId,
        parent: Box<NodeId>,
        index: usize,
    },
    IndirectRef {
        doc: DocumentId,
        object: ObjectId,
    },
    Stream {
        doc: DocumentId,
        object: ObjectId,
        decoded: bool,
    },
    ResourceGroup {
        doc: DocumentId,
        page_index: usize,
        group: ResourceGroup,
    },
}

impl NodeId {
    pub fn document_id(&self) -> DocumentId {
        match self {
            Self::DocumentRoot { doc }
            | Self::Trailer { doc }
            | Self::Catalog { doc }
            | Self::XrefRoot { doc }
            | Self::XrefObject { doc, .. }
            | Self::PageRoot { doc }
            | Self::Page { doc, .. }
            | Self::DictEntry { doc, .. }
            | Self::ArrayEntry { doc, .. }
            | Self::IndirectRef { doc, .. }
            | Self::Stream { doc, .. }
            | Self::ResourceGroup { doc, .. } => doc.clone(),
        }
    }

    pub fn object_id(&self) -> Option<ObjectId> {
        match self {
            Self::XrefObject { object, .. }
            | Self::IndirectRef { object, .. }
            | Self::Stream { object, .. } => Some(*object),
            Self::DictEntry { parent, .. } | Self::ArrayEntry { parent, .. } => parent.object_id(),
            _ => None,
        }
    }

    pub fn to_serialized(&self) -> SerializedNodeId {
        let mut segments = Vec::new();
        self.push_segments(&mut segments);
        SerializedNodeId {
            schema_version: PUBLIC_SCHEMA_VERSION,
            doc: self.document_id(),
            segments,
            object: self.object_id(),
        }
    }

    fn push_segments(&self, segments: &mut Vec<NodePathSegment>) {
        match self {
            Self::DocumentRoot { .. } => segments.push(NodePathSegment::DocumentRoot),
            Self::Trailer { .. } => segments.push(NodePathSegment::Trailer),
            Self::Catalog { .. } => segments.push(NodePathSegment::Catalog),
            Self::XrefRoot { .. } => segments.push(NodePathSegment::XrefRoot),
            Self::XrefObject { object, .. } => segments.push(NodePathSegment::XrefObject(*object)),
            Self::PageRoot { .. } => segments.push(NodePathSegment::PageRoot),
            Self::Page { index, .. } => segments.push(NodePathSegment::Page { index: *index }),
            Self::DictEntry { parent, key, .. } => {
                parent.push_segments(segments);
                segments.push(NodePathSegment::DictKey(key.clone()));
            }
            Self::ArrayEntry { parent, index, .. } => {
                parent.push_segments(segments);
                segments.push(NodePathSegment::ArrayIndex(*index));
            }
            Self::IndirectRef { object, .. } => {
                segments.push(NodePathSegment::IndirectRef(*object))
            }
            Self::Stream {
                object, decoded, ..
            } => segments.push(NodePathSegment::Stream {
                object: *object,
                decoded: *decoded,
            }),
            Self::ResourceGroup {
                page_index, group, ..
            } => segments.push(NodePathSegment::ResourceGroup {
                page_index: *page_index,
                group: group.clone(),
            }),
        }
    }
}

impl SerializedNodeId {
    pub fn to_json_string(&self) -> String {
        let mut out = String::new();
        out.push('{');
        push_json_field_u32(&mut out, "schema_version", self.schema_version);
        out.push(',');
        push_json_field_u64(&mut out, "doc", self.doc.0);
        out.push_str(",\"segments\":[");
        for (index, segment) in self.segments.iter().enumerate() {
            if index != 0 {
                out.push(',');
            }
            segment.push_json(&mut out);
        }
        out.push(']');
        if let Some(object) = self.object {
            out.push_str(",\"object\":");
            push_object_id_json(&mut out, object);
        } else {
            out.push_str(",\"object\":null");
        }
        out.push('}');
        out
    }
}

impl NodePathSegment {
    fn push_json(&self, out: &mut String) {
        out.push('{');
        match self {
            Self::DocumentRoot => push_json_field_str(out, "tag", "document_root"),
            Self::Trailer => push_json_field_str(out, "tag", "trailer"),
            Self::Catalog => push_json_field_str(out, "tag", "catalog"),
            Self::XrefRoot => push_json_field_str(out, "tag", "xref_root"),
            Self::XrefObject(object) => {
                push_json_field_str(out, "tag", "xref_object");
                out.push_str(",\"object\":");
                push_object_id_json(out, *object);
            }
            Self::PageRoot => push_json_field_str(out, "tag", "page_root"),
            Self::Page { index } => {
                push_json_field_str(out, "tag", "page");
                out.push_str(",\"index\":");
                out.push_str(&index.to_string());
            }
            Self::DictKey(key) => {
                push_json_field_str(out, "tag", "dict_key");
                out.push_str(",\"key\":");
                push_json_string(out, key);
            }
            Self::ArrayIndex(index) => {
                push_json_field_str(out, "tag", "array_index");
                out.push_str(",\"index\":");
                out.push_str(&index.to_string());
            }
            Self::IndirectRef(object) => {
                push_json_field_str(out, "tag", "indirect_ref");
                out.push_str(",\"object\":");
                push_object_id_json(out, *object);
            }
            Self::Stream { object, decoded } => {
                push_json_field_str(out, "tag", "stream");
                out.push_str(",\"object\":");
                push_object_id_json(out, *object);
                out.push_str(",\"decoded\":");
                out.push_str(if *decoded { "true" } else { "false" });
            }
            Self::ResourceGroup { page_index, group } => {
                push_json_field_str(out, "tag", "resource_group");
                out.push_str(",\"page_index\":");
                out.push_str(&page_index.to_string());
                out.push_str(",\"group\":");
                push_json_string(out, group.as_public_str());
            }
        }
        out.push('}');
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum ResourceGroup {
    Fonts,
    Images,
    XObjects,
    ColorSpaces,
    Patterns,
    Shadings,
    Annotations,
    Widgets,
}

impl ResourceGroup {
    pub fn as_public_str(&self) -> &'static str {
        match self {
            Self::Fonts => "fonts",
            Self::Images => "images",
            Self::XObjects => "xobjects",
            Self::ColorSpaces => "color_spaces",
            Self::Patterns => "patterns",
            Self::Shadings => "shadings",
            Self::Annotations => "annotations",
            Self::Widgets => "widgets",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ObjectSummary {
    pub id: NodeId,
    pub kind: ObjectKind,
    pub label: String,
    pub preview: String,
    pub object: Option<ObjectId>,
    pub has_children: bool,
    pub has_stream: bool,
    pub child_count: Option<usize>,
    pub byte_size_hint: Option<u64>,
    pub diagnostics: Vec<DiagnosticSummary>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObjectKind {
    Null,
    Bool,
    Int,
    Real,
    Name,
    String,
    Array,
    Dict,
    IndirectRef,
    Stream,
    Page,
    XrefEntry,
    Trailer,
    Metadata,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct ObjectDetail {
    pub id: NodeId,
    pub kind: ObjectKind,
    pub object: Option<ObjectId>,
    pub label: String,
    pub preview: String,
    pub value: ObjectValue,
    pub dictionary_entries: Option<ChildPage<DictEntryDetail>>,
    pub array_entries: Option<ChildPage>,
    pub stream: Option<StreamSummary>,
    pub diagnostics: Vec<DiagnosticSummary>,
}

#[derive(Clone, Debug)]
pub enum ObjectValue {
    Null,
    Bool(bool),
    Int(i64),
    Real(f64),
    Name(String),
    StringBytes {
        bytes: Vec<u8>,
        string_kind: PdfStringKind,
        is_text_string: bool,
        decoded_text: Option<String>,
    },
    IndirectRef(ObjectId),
    Container,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PdfStringKind {
    Literal,
    Hex,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct DictEntryDetail {
    pub key: String,
    pub value: ObjectSummary,
}

#[derive(Clone, Debug)]
pub struct StreamSummary {
    pub object: ObjectId,
    pub filters: Vec<String>,
    pub raw_size_hint: Option<u64>,
    pub decoded_size_hint: Option<u64>,
    pub can_decode: bool,
    pub image_preview_available: bool,
}

#[derive(Clone, Debug)]
pub struct DocumentSummary {
    pub doc: DocumentId,
    pub file_path: String,
    pub file_hash: Option<String>,
    pub pdf_version: Option<String>,
    pub page_count: usize,
    pub xref_size: usize,
    pub parsed_object_count: Option<usize>,
    pub encrypted: bool,
    pub needs_password: bool,
    pub permissions: DocumentPermissions,
    pub metadata_summary: Vec<(String, String)>,
    pub safety: DocumentSafetyState,
    pub diagnostics: Vec<DiagnosticSummary>,
}

#[derive(Clone, Debug)]
pub struct DocumentSafetyState {
    pub safe_mode: bool,
    pub javascript_disabled: bool,
    pub repaired_or_damaged: bool,
    pub embedded_files_detected: bool,
    pub external_references_detected: bool,
    pub ocr_enabled: bool,
}

#[derive(Clone, Debug)]
pub struct DocumentPermissions {
    pub print: bool,
    pub modify: bool,
    pub copy: bool,
    pub annotate: bool,
    pub fill_forms: bool,
    pub extract_accessibility: bool,
    pub assemble: bool,
    pub high_quality_print: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChildRange {
    pub offset: usize,
    pub limit: usize,
}

#[derive(Clone, Debug)]
pub struct ChildPage<T = ObjectSummary> {
    pub total: Option<usize>,
    pub items: Vec<T>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamMode {
    Raw,
    Decoded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamViewMode {
    Hex,
    Text,
    Bytes,
}

#[derive(Clone, Debug)]
pub struct StreamChunk {
    pub mode: StreamMode,
    pub offset: u64,
    pub bytes: Vec<u8>,
    pub total_size: Option<u64>,
    pub truncated: bool,
    pub decode_diagnostics: Vec<DiagnosticSummary>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderColorMode {
    Rgba,
    Grayscale,
    Inverted,
}

#[derive(Clone, Debug)]
pub struct RenderRequest {
    pub page_index: usize,
    pub zoom: f32,
    pub rotation_degrees: i32,
    pub max_width: u32,
    pub max_height: u32,
    pub max_pixels: u64,
    pub max_output_bytes: u64,
    pub color_mode: RenderColorMode,
    pub layer_config_token: Option<u64>,
}

impl RenderRequest {
    pub fn page(page_index: usize) -> Self {
        Self {
            page_index,
            zoom: 1.0,
            rotation_degrees: 0,
            max_width: 4096,
            max_height: 4096,
            max_pixels: 16_777_216,
            max_output_bytes: 128 * 1024 * 1024,
            color_mode: RenderColorMode::Rgba,
            layer_config_token: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RenderResult {
    pub page_index: usize,
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub pixels_rgba: Vec<u8>,
    pub duration_ms: u64,
    pub diagnostics: Vec<DiagnosticSummary>,
}

#[derive(Clone, Debug)]
pub struct TextRequest {
    pub page_index: usize,
    pub sort_by_position: bool,
    pub include_coordinates: bool,
    pub max_chars: usize,
    pub max_blocks: usize,
}

impl TextRequest {
    pub fn page(page_index: usize) -> Self {
        Self {
            page_index,
            sort_by_position: true,
            include_coordinates: true,
            max_chars: 1_000_000,
            max_blocks: 100_000,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TextPage {
    pub page_index: usize,
    pub spans: Vec<TextSpan>,
}

#[derive(Clone, Debug)]
pub struct TextSpan {
    pub text: String,
    pub bbox: PageRect,
    pub untrusted: bool,
}

#[derive(Clone, Debug)]
pub struct PageRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug)]
pub struct MuPdfCapabilities {
    pub can_inspect_structure: bool,
    pub can_load_raw_streams: bool,
    pub can_load_decoded_streams: bool,
    pub can_render_pages: bool,
    pub can_extract_text: bool,
    pub can_extract_positioned_text: bool,
    pub can_ocr: bool,
    pub can_list_incremental_sections: bool,
    pub can_report_repair_diagnostics: bool,
    pub cancellation: CancellationCapability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancellationCapability {
    CooperativeDuringOperation,
    BetweenOperationsOnly,
    IsolatedProcessAbort,
}

#[derive(Clone, Debug)]
pub struct DiagnosticSummary {
    pub severity: DiagnosticSeverity,
    pub code: DiagnosticCode,
    pub message: String,
    pub node: Option<NodeId>,
    pub page_index: Option<usize>,
    pub object: Option<ObjectId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

impl DiagnosticSeverity {
    pub fn as_public_str(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum DiagnosticCode {
    MissingObject,
    BrokenXrefEntry,
    StreamDecodeFailure,
    EncryptionPasswordFailure,
    RepairWarning,
    JavaScriptDisabled,
    EmbeddedFileDetected,
    ExternalReferenceDetected,
    ResourceMissing,
    RenderWarning,
    Unknown,
}

impl DiagnosticCode {
    pub fn as_public_str(self) -> &'static str {
        match self {
            Self::MissingObject => "missing_object",
            Self::BrokenXrefEntry => "broken_xref_entry",
            Self::StreamDecodeFailure => "stream_decode_failure",
            Self::EncryptionPasswordFailure => "encryption_password_failure",
            Self::RepairWarning => "repair_warning",
            Self::JavaScriptDisabled => "javascript_disabled",
            Self::EmbeddedFileDetected => "embedded_file_detected",
            Self::ExternalReferenceDetected => "external_reference_detected",
            Self::ResourceMissing => "resource_missing",
            Self::RenderWarning => "render_warning",
            Self::Unknown => "unknown",
        }
    }
}

pub fn diagnostics_payload_to_json_string(diagnostics: &[DiagnosticSummary]) -> String {
    let mut out = String::new();
    out.push('{');
    push_json_field_u32(
        &mut out,
        "diagnostic_schema_version",
        DIAGNOSTIC_SCHEMA_VERSION,
    );
    out.push_str(",\"diagnostics\":[");
    for (index, diagnostic) in diagnostics.iter().enumerate() {
        if index != 0 {
            out.push(',');
        }
        push_diagnostic_json(&mut out, diagnostic);
    }
    out.push_str("]}");
    out
}

fn push_diagnostic_json(out: &mut String, diagnostic: &DiagnosticSummary) {
    out.push('{');
    push_json_field_str(out, "severity", diagnostic.severity.as_public_str());
    out.push(',');
    push_json_field_str(out, "code", diagnostic.code.as_public_str());
    out.push(',');
    push_json_field_str_bounded(
        out,
        "message",
        &diagnostic.message,
        DIAGNOSTIC_JSON_FIELD_LIMIT_BYTES,
    );
    out.push_str(",\"node\":");
    if let Some(node) = &diagnostic.node {
        out.push_str(&node.to_serialized().to_json_string());
    } else {
        out.push_str("null");
    }
    out.push_str(",\"page_index\":");
    if let Some(page_index) = diagnostic.page_index {
        out.push_str(&page_index.to_string());
    } else {
        out.push_str("null");
    }
    out.push_str(",\"object\":");
    if let Some(object) = diagnostic.object {
        push_object_id_json(out, object);
    } else {
        out.push_str("null");
    }
    out.push('}');
}

fn push_json_field_u32(out: &mut String, name: &str, value: u32) {
    push_json_string(out, name);
    out.push(':');
    out.push_str(&value.to_string());
}

fn push_json_field_u64(out: &mut String, name: &str, value: u64) {
    push_json_string(out, name);
    out.push(':');
    out.push_str(&value.to_string());
}

fn push_json_field_str(out: &mut String, name: &str, value: &str) {
    push_json_string(out, name);
    out.push(':');
    push_json_string(out, value);
}

fn push_json_field_str_bounded(out: &mut String, name: &str, value: &str, max_bytes: usize) {
    push_json_string(out, name);
    out.push(':');
    append_bounded_json_string(out, value, max_bytes);
}

fn push_object_id_json(out: &mut String, object: ObjectId) {
    out.push_str("{\"num\":");
    out.push_str(&object.num.to_string());
    out.push_str(",\"gen\":");
    out.push_str(&object.gen.to_string());
    out.push('}');
}

fn push_json_string(out: &mut String, value: &str) {
    append_json_string(out, value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialized_node_id_uses_stable_json_shape() {
        let doc = DocumentId(42);
        let object = ObjectId { num: 12, gen: 0 };
        let node = NodeId::DictEntry {
            doc: doc.clone(),
            parent: Box::new(NodeId::DictEntry {
                doc: doc.clone(),
                parent: Box::new(NodeId::XrefObject { doc, object }),
                key: "Resources".to_string(),
            }),
            key: "Font".to_string(),
        };

        assert_eq!(
            node.to_serialized().to_json_string(),
            "{\"schema_version\":1,\"doc\":42,\"segments\":[{\"tag\":\"xref_object\",\"object\":{\"num\":12,\"gen\":0}},{\"tag\":\"dict_key\",\"key\":\"Resources\"},{\"tag\":\"dict_key\",\"key\":\"Font\"}],\"object\":{\"num\":12,\"gen\":0}}"
        );
    }

    #[test]
    fn serialized_node_id_escapes_keys_and_never_mentions_path_tokens() {
        let node = NodeId::DictEntry {
            doc: DocumentId(7),
            parent: Box::new(NodeId::Trailer { doc: DocumentId(7) }),
            key: "A\"B\nC".to_string(),
        };
        let json = node.to_serialized().to_json_string();

        assert!(json.contains("\"key\":\"A\\\"B\\nC\""));
        assert!(!json.contains("path_token"));
    }

    #[test]
    fn diagnostic_schema_payload_and_public_strings_are_pinned() {
        assert_eq!(DiagnosticSeverity::Info.as_public_str(), "info");
        assert_eq!(DiagnosticSeverity::Warning.as_public_str(), "warning");
        assert_eq!(DiagnosticSeverity::Error.as_public_str(), "error");

        let codes = [
            (DiagnosticCode::MissingObject, "missing_object"),
            (DiagnosticCode::BrokenXrefEntry, "broken_xref_entry"),
            (DiagnosticCode::StreamDecodeFailure, "stream_decode_failure"),
            (
                DiagnosticCode::EncryptionPasswordFailure,
                "encryption_password_failure",
            ),
            (DiagnosticCode::RepairWarning, "repair_warning"),
            (DiagnosticCode::JavaScriptDisabled, "javascript_disabled"),
            (
                DiagnosticCode::EmbeddedFileDetected,
                "embedded_file_detected",
            ),
            (
                DiagnosticCode::ExternalReferenceDetected,
                "external_reference_detected",
            ),
            (DiagnosticCode::ResourceMissing, "resource_missing"),
            (DiagnosticCode::RenderWarning, "render_warning"),
            (DiagnosticCode::Unknown, "unknown"),
        ];

        for (code, expected) in codes {
            assert_eq!(code.as_public_str(), expected);
        }

        let diagnostics = [DiagnosticSummary {
            severity: DiagnosticSeverity::Warning,
            code: DiagnosticCode::RepairWarning,
            message: "xref repaired\nwith fallback".to_string(),
            node: Some(NodeId::Page {
                doc: DocumentId(9),
                index: 2,
            }),
            page_index: Some(2),
            object: Some(ObjectId { num: 4, gen: 0 }),
        }];
        assert_eq!(
            diagnostics_payload_to_json_string(&diagnostics),
            "{\"diagnostic_schema_version\":1,\"diagnostics\":[{\"severity\":\"warning\",\"code\":\"repair_warning\",\"message\":\"xref repaired\\nwith fallback\",\"node\":{\"schema_version\":1,\"doc\":9,\"segments\":[{\"tag\":\"page\",\"index\":2}],\"object\":null},\"page_index\":2,\"object\":{\"num\":4,\"gen\":0}}]}"
        );
    }

    #[test]
    fn diagnostics_json_bounds_and_escapes_control_message() {
        let diagnostics = [DiagnosticSummary {
            severity: DiagnosticSeverity::Warning,
            code: DiagnosticCode::Unknown,
            message: format!("A\u{202e}{}", "x".repeat(5000)),
            node: None,
            page_index: None,
            object: None,
        }];

        let json = diagnostics_payload_to_json_string(&diagnostics);

        assert!(json.contains("\"message\":\"A\\u202e"));
        assert!(!json.contains('\u{202e}'));
        assert!(!json.contains(&"x".repeat(4093)));
    }
}
