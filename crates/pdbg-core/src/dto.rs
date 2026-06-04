pub const PUBLIC_SCHEMA_VERSION: u32 = 1;
pub const DIAGNOSTIC_SCHEMA_VERSION: u32 = 1;

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

#[derive(Clone, Debug)]
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
