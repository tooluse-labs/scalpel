# MuPDF Rust PDF Debugger Architecture

## 1. Product Positioning

This project is a long-lived native PDF debugger built on MuPDF, Rust, and egui.
It is not a PDFBox-app clone and not a general PDF editor.

The primary goal is to diagnose complex PDFs:

- inspect PDF object graphs, xref tables, streams, pages, resources, annotations,
  forms, signatures, and incremental updates;
- correlate low-level PDF objects with rendered page output;
- remain responsive on large or damaged files;
- expose safe, bounded read-only PDF diagnostics to LLM agents through MCP.

The first product shape is a read-only desktop debugger. Editing, rewriting,
redaction, and save-overwrite operations are out of scope for the initial
releases.

## 2. High-Level Architecture

```text
egui desktop UI
  |
  | stable DTOs, commands, events
  v
Rust application layer
  |
  | DocumentSession / NodeModel / RenderService / Diagnostics / MCP
  v
MuPDF engine adapter / C shim
  |
  | fz_try/fz_catch boundary, pointer ownership, error conversion
  v
MuPDF C core
```

The UI never owns MuPDF pointers. The Rust layer exposes stable identifiers and
plain data transfer objects. The C shim is the only layer allowed to call MuPDF C
APIs directly.

The MVP and planned debugger backend are MuPDF-only. Public DTOs, reports, MCP
output, page coordinates, stream modes, limits, and capability checks should
remain stable across MuPDF versions and build configurations, but they are not a
multi-engine plugin contract. Alternate PDF engines require a separate ADR and
are out of scope for the MVP architecture.

## 3. MVP Scope

The MVP should prove the core debugger architecture before adding advanced
diagnostics.

In this document, MVP means the first public desktop debugger preview. It
includes Milestones 1-3 below. The read-only MCP server is a post-MVP extension
in Milestone 4, although the backend should keep DTOs and limits compatible with
that later interface.

### 3.1 Required MVP Features

- Open local PDF files.
- Prompt for password when needed.
- Show document summary:
  - file path;
  - PDF version;
  - page count;
  - object count;
  - encrypted/password state;
  - permissions;
  - metadata summary.
- Show lazy object tree:
  - trailer;
  - catalog/root;
  - page tree;
  - xref object list;
  - indirect references.
- Inspect PDF objects:
  - null;
  - boolean;
  - integer;
  - real;
  - name;
  - string/text string;
  - array;
  - dictionary;
  - indirect reference;
  - stream object.
- Support indirect reference navigation.
- Support raw and decoded stream views:
  - raw stream;
  - decoded stream;
  - hex view;
  - text view with lossy fallback.
- Render page preview:
  - page list;
  - zoom;
  - rotate;
  - fit to view;
  - rendering cancellation.
- Search:
  - object number;
  - dictionary key;
  - name object;
  - text extracted from pages.
- Basic diagnostics:
  - missing object;
  - broken xref entry;
  - stream decode failure;
  - encryption/password failure;
  - repair warning if MuPDF reports one.

### 3.2 Explicit MVP Non-Goals

- Editing PDF objects.
- Saving modified PDFs.
- Redaction.
- Full content stream visualization.
- Signature validation UI beyond basic object inspection.
- FDF/XFDF import/export.
- Printing.
- Full MCP server.
- Persistent AcroForm repair/rewriting and form-field mutation.
- Alternate PDF engines or a multi-engine plugin system.
- Full PDFBox PDFDebugger feature parity.

## 4. Post-MVP Roadmap

### 4.1 Rendering Diagnostics

- Content stream operator list.
- Operator-to-page highlight.
- Text-extraction debugging overlays on the rendered page:
  - extracted text positions;
  - text flow beads;
  - approximate text bounds;
  - glyph bounds.
- Page resource dependency graph.
- Transparency group and blend mode inspection.
- Optional layer/OCG inspection and toggling for preview.
- Render timing per page.
- Image decode and font load timing.

### 4.2 Resource Inspectors

- Font inspector:
  - font type;
  - embedded/subset state;
  - encoding;
  - ToUnicode/CMap;
  - glyph coverage;
  - missing glyph warnings.
- Image inspector:
  - dimensions;
  - color space;
  - bits per component;
  - filters;
  - masks/SMasks;
  - raw and decoded preview.
- Annotation/form inspector:
  - annotation list;
  - widget tree;
  - appearance stream state;
  - field values;
  - read-only AcroForm repair diagnostics.
- Color space inspector:
  - Indexed/Separation/DeviceN/ICCBased visualization;
  - component values and tint transforms.
- Flag-bits decoder:
  - annotation flags;
  - font descriptor flags;
  - permission bits.

### 4.3 PDF History And Integrity

- Incremental update sections.
- Change history view.
- Signature ByteRange view.
- Digest and certificate diagnostics.
- Object reachability graph.
- Duplicate and orphan object detection.

### 4.4 AI And MCP

- Read-only MCP server.
- Document chunking for LLM context.
- Page render tool for visual models.
- Object explanation tool with bounded context.
- Prompt-injection aware text extraction.

## 5. Data Model

### 5.1 Design Principles

- Use stable IDs, not MuPDF pointers, across the Rust/UI boundary.
- Resolve data lazily.
- Do not keep `pdf_xref_entry*` or temporary MuPDF internal pointers in GUI
  state. MuPDF may reorganize xref data during loading or repair.
- Preserve both PDF object identity and UI path identity.
- Make all expensive values explicit:
  - streams are loaded separately;
  - page previews are rendered separately;
  - full child lists are paginated when needed.

### 5.1.1 MuPDF Feature Capabilities

Capabilities make MuPDF build, version, configuration, and isolation-mode
differences explicit without committing the product to a multi-engine plugin
system.

```rust
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
```

The MuPDF adapter reports the capabilities it actually supports in the current
build and runtime mode. UI panels, menus, and MCP tools must hide, disable, or
return `PDBG_ERROR_UNSUPPORTED` for disabled or unavailable MuPDF features. This
is feature gating for a MuPDF-only product, not support for alternate parsing
engines.

Capability checks are product behavior, not only backend metadata. They must be
covered by contract tests using fake MuPDF capability sets.

### 5.2 Core Identifiers

```rust
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct DocumentId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ObjectId {
    pub num: i32,
    pub gen: i32,
}

#[derive(Clone, Debug)]
pub struct SerializedNodeId {
    pub schema_version: u32,
    pub doc: DocumentId,
    pub segments: Vec<NodePathSegment>,
    pub object: Option<ObjectId>,
}

#[derive(Clone, Debug)]
pub enum NodePathSegment {
    DocumentRoot,
    Trailer,
    Catalog,
    XrefRoot,
    XrefObject(ObjectId),
    PageRoot,
    Page { index: usize },
    DictKey(String),
    ArrayIndex(usize),
    IndirectRef(ObjectId),
    Stream { object: ObjectId, decoded: bool },
    ResourceGroup { page_index: usize, group: ResourceGroup },
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum NodeId {
    DocumentRoot { doc: DocumentId },
    Trailer { doc: DocumentId },
    Catalog { doc: DocumentId },
    XrefRoot { doc: DocumentId },
    XrefObject { doc: DocumentId, object: ObjectId },
    PageRoot { doc: DocumentId },
    Page { doc: DocumentId, index: usize },
    DictEntry { doc: DocumentId, parent: Box<NodeId>, key: String },
    ArrayEntry { doc: DocumentId, parent: Box<NodeId>, index: usize },
    IndirectRef { doc: DocumentId, object: ObjectId },
    Stream { doc: DocumentId, object: ObjectId, decoded: bool },
    ResourceGroup { doc: DocumentId, page_index: usize, group: ResourceGroup },
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
```

`ObjectId` is the PDF object identity and must include both object number and
generation. `NodeId` is a UI path identity. Cache keys that refer to a PDF object
should use `ObjectId`, not a tree path, unless the visual path itself is the
thing being cached.

Public reports and MCP outputs must serialize `NodeId` as `SerializedNodeId`.
The JSON encoding uses stable lowercase segment tags, includes
`schema_version`, and never exposes C `path_token` values. `SerializedNodeId`
is stable within a report or open-document session. It is not guaranteed to
resolve after the document is repaired, reloaded, or opened by a different
backend version unless the same `ObjectId` is also present.

The public JSON shape uses `PUBLIC_SCHEMA_VERSION` for `schema_version`, a
numeric `doc`, segment objects with a `tag` field, and object ids as
`{"num": 12, "gen": 0}`. Segment tags are:
`document_root`, `trailer`, `catalog`, `xref_root`, `xref_object`, `page_root`,
`page`, `dict_key`, `array_index`, `indirect_ref`, `stream`, and
`resource_group`. Resource group values use the fixed strings listed below.

Resource group public strings are fixed as:

- `fonts`;
- `images`;
- `xobjects`;
- `color_spaces`;
- `patterns`;
- `shadings`;
- `annotations`;
- `widgets`.

Example:

```json
{
  "schema_version": 1,
  "doc": 42,
  "segments": [
    { "tag": "xref_object", "object": { "num": 12, "gen": 0 } },
    { "tag": "dict_key", "key": "Resources" },
    { "tag": "dict_key", "key": "Font" }
  ],
  "object": { "num": 12, "gen": 0 }
}
```

### 5.3 Object Summary DTO

```rust
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

#[derive(Clone, Debug)]
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
```

### 5.4 Document Summary DTO

```rust
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
```

`xref_size` is the declared xref span. `parsed_object_count` is the number of
objects the backend could inspect after MuPDF loading or repair; it may be lower
than `xref_size` for damaged files.

### 5.5 Lazy Tree API

```rust
pub trait NodeModel {
    fn root_nodes(&self, doc: DocumentId) -> Result<Vec<ObjectSummary>>;
    fn children(&self, node: &NodeId, range: ChildRange) -> Result<ChildPage>;
    fn object_detail(&self, node: &NodeId) -> Result<ObjectDetail>;
    fn resolve_ref(&self, doc: DocumentId, object: ObjectId) -> Result<NodeId>;
}

pub struct ChildRange {
    pub offset: usize,
    pub limit: usize,
}

pub struct ChildPage<T = ObjectSummary> {
    pub total: Option<usize>,
    pub items: Vec<T>,
}
```

The UI should request children only when a tree node expands. Large arrays and
dictionaries must support paging. `ChildPage.total = Some(n)` means the backend
knows the complete child count. `None` means the total is unknown, expensive, or
not yet computed. A paged traversal is complete when the returned page contains
fewer than `limit` items; clients must not treat `total = None` as either empty
or infinite.

### 5.6 Stream API

```rust
pub enum StreamMode {
    Raw,
    Decoded,
}

pub enum StreamViewMode {
    Hex,
    Text,
    Bytes,
}

pub struct StreamRequest {
    pub doc: DocumentId,
    pub object: ObjectId,
    pub mode: StreamMode,
    pub offset: u64,
    pub limit: usize,
}

pub struct StreamChunk {
    pub mode: StreamMode,
    pub offset: u64,
    pub bytes: Vec<u8>,
    pub total_size: Option<u64>,
    pub truncated: bool,
    pub decode_diagnostics: Vec<DiagnosticSummary>,
}
```

Raw stream means compressed but decrypted bytes. Decoded stream means
uncompressed/decompressed content returned by MuPDF. Stream APIs must apply
limits during reading and decoding, not only after a complete decoded buffer has
already been materialized.

### 5.7 Page Preview API

```rust
pub struct RenderRequest {
    pub doc: DocumentId,
    pub page_index: usize,
    pub zoom: f32,
    pub rotation_degrees: i32,
    pub max_width: u32,
    pub max_height: u32,
    pub color_mode: RenderColorMode,
    pub layer_config: Option<LayerConfig>,
}

pub struct RenderResult {
    pub page_index: usize,
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub pixels_rgba: Vec<u8>,
    pub duration_ms: u64,
    pub diagnostics: Vec<DiagnosticSummary>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderColorMode {
    Rgba,
    Grayscale,
    Inverted,
}

#[derive(Clone, Debug)]
pub struct LayerConfig {
    pub token: u64,
}
```

`RenderResult.pixels_rgba` and `pdbg_image_rgba_pixels` are always 8-bit RGBA
buffers. `RenderColorMode::Grayscale` and `RenderColorMode::Inverted` transform
the returned RGBA pixels; they do not change bytes per pixel or pixel format.

Page indices in Rust, C, and MCP APIs are zero-based. UI labels may be
one-based, but conversion must happen at the UI boundary. The C ABI represents
page indices as `uint32_t`; Rust must use checked `try_into()` conversions
between `usize` and `u32`.

Page rendering requests must follow the MuPDF backend's cancellation capability
and must not block the UI thread.

Object and text search are Rust application-layer features. Object search walks
or queries the lazy tree and object summaries. Text search is built on bounded
`pdbg_page_extract_text` results and cached text extraction output. The C shim
does not need separate search entry points unless later profiling shows this is
a bottleneck.

### 5.8 Text Extraction API

```rust
pub struct TextPage {
    pub page_index: usize,
    pub spans: Vec<TextSpan>,
}

pub struct TextSpan {
    pub text: String,
    pub bbox: PageRect,
    pub untrusted: bool,
}

pub struct PageRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
```

`TextSpan.bbox` uses the normalized page-space coordinate system defined in the
MuPDF C shim section. `untrusted = true` means the text came from PDF-controlled
content and must follow the egress rules in Section 9.

## 6. Threading Model

### 6.1 Rules

- Each opened document is represented by one `DocumentSession`.
- A `DocumentSession` serializes access to the underlying MuPDF document.
- The UI thread never calls MuPDF directly.
- Background workers may render/decode/search, but all operations for the same
  document pass through the session scheduler.
- For the MuPDF adapter, do not access the same `pdf_document`, `pdf_page`, or
  `fz_device` concurrently.
- For the MuPDF adapter, each worker thread must use its own MuPDF context,
  cloned from the root context according to MuPDF's context model.
- For the MuPDF adapter, the root MuPDF context must be created with an
  `fz_locks_context` backed by `FZ_LOCK_MAX` locks before any cloned context is
  used. Cloned contexts share MuPDF global state such as the store and glyph
  cache, so per-document locks do not replace MuPDF's lock callbacks.
- All MuPDF contexts in the process must share the same lock callbacks or
  underlying locks.
- Callback entry points must be protected by explicit locks and must not call
  back into the same document session in a reentrant way.
- Long operations must support cancellation.

### 6.2 Rust Session Shape

```rust
pub struct DocumentSession {
    id: DocumentId,
    raw: parking_lot::Mutex<NonNull<PdbgDoc>>,
    task_queue: DocumentTaskQueue,
    cache: DocumentCache,
}
```

This shape is illustrative; helper types such as `DocumentTaskQueue`,
`DocumentCache`, and the crate-local `Result<T>` alias are omitted. `PdbgDoc` is
the Rust-side alias for the opaque C `pdbg_doc` type exposed through the checked
raw ABI bindings.

Default trait policy:

- `DocumentSession` may be `Send` only if an unsafe implementation documents
  that all access to `raw` is serialized through the session queue and mutex.
- `DocumentSession` should not be `Sync` unless all access goes through the
  internal scheduler and lock.
- MuPDF-owned page/device handles should be `!Send + !Sync` unless audited.
  They should normally be created, used, and dropped inside a single worker
  operation.
- Rendered pixels, stream bytes, summaries, and diagnostics are owned Rust data
  and may be `Send + Sync`.

### 6.3 Work Scheduling

```text
UI event
  -> command sent to app state
  -> app state submits task to document session
  -> worker gets document session lock
  -> C shim calls MuPDF
  -> result DTO sent back to UI
```

The first implementation can use one worker queue per document. Later versions
may allow safe parallelism for read-only page rendering after a focused audit.

### 6.4 Cache Policy

Cache only owned Rust outputs:

- object summaries;
- small decoded streams;
- rendered thumbnails;
- page dimensions;
- text extraction results.

Do not cache:

- `pdf_obj*`;
- `pdf_xref_entry*`;
- `fz_stream*`;
- `fz_device*`;
- borrowed string pointers returned by MuPDF.

All caches need byte limits. Render caches should use LRU eviction.

The first implementation should use explicit defaults:

- per-document decoded stream cache: 64 MiB;
- per-document thumbnail/render cache: 256 MiB;
- per single decoded stream request: 16 MiB unless the user explicitly raises
  the limit;
- per MCP tool response: 1 MiB inline text or JSON before truncation.

## 7. MuPDF C Shim Boundary

### 7.1 Purpose

The MuPDF C shim exists to prevent MuPDF's exception model and pointer ownership
rules from leaking into Rust.

MuPDF uses `fz_try`/`fz_catch`. Rust must never allow a MuPDF `longjmp` to cross
Rust stack frames. Every exported shim function must catch MuPDF errors and
return an explicit status code.

The `pdbg_*` ABI below is the MuPDF adapter ABI, not a stable public plugin ABI.
Alternate PDF engines are out of scope for the MVP architecture and require a
separate ADR before any adapter shape or compatibility contract is defined.

### 7.2 C Result Model

```c
typedef enum pdbg_status {
    PDBG_OK = 0,
    PDBG_ERROR_GENERIC = 1,
    PDBG_ERROR_PASSWORD = 2,
    PDBG_ERROR_FORMAT = 3,
    PDBG_ERROR_UNSUPPORTED = 4,
    PDBG_ERROR_CANCELLED = 5,
    PDBG_ERROR_OOM = 6,
    PDBG_ERROR_SECURITY = 7,
    PDBG_ERROR_LIMIT = 8
} pdbg_status;

typedef struct pdbg_error {
    pdbg_status status;
    int mupdf_code;
    char message[1024];
} pdbg_error;
```

Every exported function returns `pdbg_status` and optionally fills `pdbg_error`.
`pdbg_error.message` must always be NUL-terminated. If context creation fails
before a MuPDF context exists, the shim must fill the error with a no-context
helper rather than calling a helper that requires `fz_context *`.

### 7.3 C Handles

```c
typedef struct pdbg_context pdbg_context;
typedef struct pdbg_doc pdbg_doc;
typedef struct pdbg_buffer pdbg_buffer;
typedef struct pdbg_image pdbg_image;
typedef struct pdbg_node_list pdbg_node_list;
typedef struct pdbg_diagnostic_list pdbg_diagnostic_list;
typedef struct pdbg_text_page pdbg_text_page;
typedef struct pdbg_cancel_token pdbg_cancel_token;
```

Rust treats these as opaque pointers.

The C ABI must use flat wire structs, not the recursive Rust `NodeId`. Recursive
dictionary and array paths are encoded as backend-issued path tokens.

```c
typedef struct pdbg_object_id {
    int num;
    int gen;
} pdbg_object_id;

typedef enum pdbg_node_kind {
    PDBG_NODE_DOCUMENT_ROOT = 0,
    PDBG_NODE_TRAILER = 1,
    PDBG_NODE_CATALOG = 2,
    PDBG_NODE_XREF_ROOT = 3,
    PDBG_NODE_XREF_OBJECT = 4,
    PDBG_NODE_PAGE_ROOT = 5,
    PDBG_NODE_PAGE = 6,
    PDBG_NODE_PATH_TOKEN = 7,
    PDBG_NODE_INDIRECT_REF = 8,
    PDBG_NODE_STREAM = 9,
    PDBG_NODE_RESOURCE_GROUP = 10
} pdbg_node_kind;

typedef enum pdbg_resource_group {
    PDBG_RESOURCE_FONTS = 0,
    PDBG_RESOURCE_IMAGES = 1,
    PDBG_RESOURCE_XOBJECTS = 2,
    PDBG_RESOURCE_COLOR_SPACES = 3,
    PDBG_RESOURCE_PATTERNS = 4,
    PDBG_RESOURCE_SHADINGS = 5,
    PDBG_RESOURCE_ANNOTATIONS = 6,
    PDBG_RESOURCE_WIDGETS = 7
} pdbg_resource_group;

typedef struct pdbg_node_id {
    uint64_t document_id;
    pdbg_node_kind kind;
    pdbg_object_id object;
    int has_object;
    uint32_t page_index;
    uint64_t path_token;
    int decoded;
    pdbg_resource_group resource_group;
} pdbg_node_id;

typedef struct pdbg_string_pair {
    char *key;
    char *value;
} pdbg_string_pair;

typedef struct pdbg_permissions {
    int print;
    int modify;
    int copy;
    int annotate;
    int fill_forms;
    int extract_accessibility;
    int assemble;
    int high_quality_print;
} pdbg_permissions;

typedef enum pdbg_object_kind {
    PDBG_OBJECT_NULL = 0,
    PDBG_OBJECT_BOOL = 1,
    PDBG_OBJECT_INT = 2,
    PDBG_OBJECT_REAL = 3,
    PDBG_OBJECT_NAME = 4,
    PDBG_OBJECT_STRING = 5,
    PDBG_OBJECT_ARRAY = 6,
    PDBG_OBJECT_DICT = 7,
    PDBG_OBJECT_INDIRECT_REF = 8,
    PDBG_OBJECT_STREAM = 9,
    PDBG_OBJECT_PAGE = 10,
    PDBG_OBJECT_XREF_ENTRY = 11,
    PDBG_OBJECT_TRAILER = 12,
    PDBG_OBJECT_METADATA = 13,
    PDBG_OBJECT_UNKNOWN = 14
} pdbg_object_kind;

typedef enum pdbg_object_value_kind {
    PDBG_VALUE_NULL = 0,
    PDBG_VALUE_BOOL = 1,
    PDBG_VALUE_INT = 2,
    PDBG_VALUE_REAL = 3,
    PDBG_VALUE_NAME = 4,
    PDBG_VALUE_STRING_BYTES = 5,
    PDBG_VALUE_INDIRECT_REF = 6,
    PDBG_VALUE_CONTAINER = 7,
    PDBG_VALUE_UNKNOWN = 8
} pdbg_object_value_kind;

typedef enum pdbg_pdf_string_kind {
    PDBG_STRING_LITERAL = 0,
    PDBG_STRING_HEX = 1,
    PDBG_STRING_UNKNOWN = 2
} pdbg_pdf_string_kind;

typedef enum pdbg_diagnostic_severity {
    PDBG_DIAG_INFO = 0,
    PDBG_DIAG_WARNING = 1,
    PDBG_DIAG_ERROR = 2
} pdbg_diagnostic_severity;

typedef enum pdbg_diagnostic_code {
    PDBG_DIAG_MISSING_OBJECT = 0,
    PDBG_DIAG_BROKEN_XREF_ENTRY = 1,
    PDBG_DIAG_STREAM_DECODE_FAILURE = 2,
    PDBG_DIAG_ENCRYPTION_PASSWORD_FAILURE = 3,
    PDBG_DIAG_REPAIR_WARNING = 4,
    PDBG_DIAG_JAVASCRIPT_DISABLED = 5,
    PDBG_DIAG_EMBEDDED_FILE_DETECTED = 6,
    PDBG_DIAG_EXTERNAL_REFERENCE_DETECTED = 7,
    PDBG_DIAG_RESOURCE_MISSING = 8,
    PDBG_DIAG_RENDER_WARNING = 9,
    PDBG_DIAG_UNKNOWN = 10
} pdbg_diagnostic_code;

typedef struct pdbg_diagnostic {
    pdbg_diagnostic_severity severity;
    pdbg_diagnostic_code code;
    char *message;
    pdbg_node_id node;
    int has_node;
    uint32_t page_index;
    int has_page_index;
    pdbg_object_id object;
    int has_object;
} pdbg_diagnostic;

typedef struct pdbg_object_value {
    pdbg_object_value_kind kind;
    int bool_value;
    int64_t int_value;
    double real_value;
    char *name_value;
    uint8_t *bytes;
    size_t byte_len;
    pdbg_pdf_string_kind string_kind;
    int is_text_string;
    char *decoded_text;
    pdbg_object_id ref_value;
} pdbg_object_value;

typedef struct pdbg_dict_entry {
    char *key;
    pdbg_node_id node;
    pdbg_object_kind object_kind;
    pdbg_object_id object;
    int has_object;
    char *label;
    char *preview;
    int has_children;
    int has_stream;
    size_t child_count;
    int has_child_count;
    uint64_t byte_size_hint;
    int has_byte_size_hint;
    pdbg_diagnostic_severity max_diagnostic_severity;
    size_t diagnostic_count;
    pdbg_diagnostic_list *diagnostics;
} pdbg_dict_entry;

typedef struct pdbg_text_span {
    char *text;
    size_t text_len;
    float x;
    float y;
    float width;
    float height;
    uint32_t page_index;
    int untrusted;
} pdbg_text_span;

typedef enum pdbg_repair_policy {
    PDBG_REPAIR_DEFAULT = 0,
    PDBG_REPAIR_NEVER = 1,
    PDBG_REPAIR_ALLOW = 2
} pdbg_repair_policy;

typedef enum pdbg_color_mode {
    PDBG_COLOR_RGBA = 0,
    PDBG_COLOR_GRAYSCALE = 1,
    PDBG_COLOR_INVERTED = 2
} pdbg_color_mode;

typedef struct pdbg_stream_summary {
    pdbg_object_id object;
    char **filters;
    size_t filter_count;
    uint64_t raw_size_hint;
    int has_raw_size_hint;
    uint64_t decoded_size_hint;
    int has_decoded_size_hint;
    int can_decode;
    int image_preview_available;
} pdbg_stream_summary;

typedef struct pdbg_open_options {
    int safe_mode;
    int disable_javascript;
    int enable_ocr;
    uint64_t max_store_bytes;
    uint64_t max_decoded_stream_bytes;
    uint32_t max_filter_expansion_ratio;
    uint32_t max_object_depth;
    pdbg_repair_policy repair_policy;
} pdbg_open_options;

typedef struct pdbg_render_options {
    float zoom;
    int rotation_degrees;
    uint32_t max_width;
    uint32_t max_height;
    uint64_t max_pixels;
    uint64_t max_output_bytes;
    pdbg_color_mode color_mode;
    uint64_t layer_config_token;
} pdbg_render_options;

typedef struct pdbg_text_options {
    int sort_by_position;
    int include_coordinates;
    size_t max_chars;
    size_t max_blocks;
} pdbg_text_options;

typedef struct pdbg_document_summary_out {
    uint64_t document_id;
    char *file_path;
    char *file_hash;
    char *pdf_version;
    size_t page_count;
    size_t xref_size;
    size_t parsed_object_count;
    int has_parsed_object_count;
    int encrypted;
    int needs_password;
    pdbg_permissions permissions;
    pdbg_string_pair *metadata;
    size_t metadata_len;
    int safe_mode;
    int javascript_disabled;
    int repaired_or_damaged;
    int embedded_files_detected;
    int external_references_detected;
    int ocr_enabled;
    pdbg_diagnostic_list *diagnostics;
} pdbg_document_summary_out;

typedef struct pdbg_object_detail_out {
    pdbg_node_id id;
    pdbg_object_id object;
    int has_object;
    pdbg_object_kind kind;
    char *label;
    char *preview;
    pdbg_object_value value;
    pdbg_node_list *children;
    pdbg_node_list *dictionary_entries;
    int has_stream;
    pdbg_stream_summary stream;
    pdbg_diagnostic_list *diagnostics;
} pdbg_object_detail_out;
```

`path_token` values are valid only for the owning document session and must be
discarded when the document closes or the tree model is rebuilt. They are not
stable public IDs and must not appear in MCP or report output. Public API output
uses serialized `NodeId` paths and `ObjectId` values instead.

Rust maintains a session-local node-token registry. When converting a
`pdbg_dict_entry` into an `ObjectSummary`, Rust reconstructs the public `NodeId`
from the parent `NodeId` plus the dictionary key or array index, then records the
returned `pdbg_node_id`/`path_token` for later calls back into the shim. Array
indices are computed from the requested `ChildRange.offset` plus the list
position; dictionary keys come from `pdbg_dict_entry.key`. Diagnostics that carry
a `pdbg_node_id` with a path token must resolve through this registry. If a
diagnostic references an unknown token, Rust should keep the diagnostic and its
`object` field when available, but omit `DiagnosticSummary.node` rather than
exposing the token.

Explicit C enum discriminants are part of the shim ABI. Do not renumber or
repurpose existing values; add new values only at the end and update Rust
conversion tests and public serialization tables at the same time.

`pdbg_text_span` coordinates use normalized page space. Units are PDF points
(1/72 inch). The origin is the top-left corner of the effective visible page
rectangle, using the CropBox when present and the MediaBox otherwise. `x`
increases rightward and `y` increases downward. The page dictionary rotation and
CropBox translation are applied so spans align with a page rendered at
`zoom = 1` and `RenderRequest.rotation_degrees = 0`. Viewer zoom and explicit
render-request rotation are not included in the span; UI and MCP consumers apply
those transforms separately. Backends should not clamp out-of-page coordinates in
the DTO because malformed PDFs may place text outside the visible page.

### 7.4 Required Shim API

```c
pdbg_status pdbg_context_new(pdbg_context **out, pdbg_error *err);
void pdbg_context_drop(pdbg_context *ctx);

pdbg_status pdbg_cancel_token_new(pdbg_cancel_token **out, pdbg_error *err);
void pdbg_cancel_token_cancel(pdbg_cancel_token *token);
void pdbg_cancel_token_drop(pdbg_cancel_token *token);

pdbg_status pdbg_document_open(
    pdbg_context *ctx,
    const char *path,
    const char *password,
    const pdbg_open_options *options,
    pdbg_doc **out,
    pdbg_error *err);

pdbg_status pdbg_document_open_fd(
    pdbg_context *ctx,
    int fd,
    const char *display_path,
    const char *password,
    const pdbg_open_options *options,
    pdbg_doc **out,
    pdbg_error *err);

void pdbg_document_drop(pdbg_doc *doc);

pdbg_status pdbg_document_summary(
    pdbg_doc *doc,
    pdbg_document_summary_out *out,
    pdbg_error *err);

pdbg_status pdbg_node_children(
    pdbg_doc *doc,
    const pdbg_node_id *node,
    size_t offset,
    size_t limit,
    pdbg_node_list **out,
    pdbg_error *err);

pdbg_status pdbg_object_detail(
    pdbg_doc *doc,
    const pdbg_node_id *node,
    pdbg_object_detail_out *out,
    pdbg_error *err);

pdbg_status pdbg_stream_load(
    pdbg_doc *doc,
    pdbg_object_id object,
    int decoded,
    uint64_t offset,
    size_t limit,
    pdbg_cancel_token *cancel,
    pdbg_buffer **out,
    pdbg_error *err);

pdbg_status pdbg_page_render(
    pdbg_doc *doc,
    uint32_t page_index,
    const pdbg_render_options *options,
    pdbg_cancel_token *cancel,
    pdbg_image **out,
    pdbg_error *err);

pdbg_status pdbg_page_extract_text(
    pdbg_doc *doc,
    uint32_t page_index,
    const pdbg_text_options *options,
    pdbg_cancel_token *cancel,
    pdbg_text_page **out,
    pdbg_error *err);

void pdbg_buffer_drop(pdbg_buffer *buffer);
void pdbg_image_drop(pdbg_image *image);
void pdbg_node_list_drop(pdbg_node_list *list);
void pdbg_text_page_drop(pdbg_text_page *text);
void pdbg_document_summary_out_drop(pdbg_document_summary_out *out);
void pdbg_object_detail_out_drop(pdbg_object_detail_out *out);

const uint8_t *pdbg_buffer_data(const pdbg_buffer *buffer);
size_t pdbg_buffer_len(const pdbg_buffer *buffer);
uint64_t pdbg_buffer_total_size_hint(const pdbg_buffer *buffer);
int pdbg_buffer_truncated(const pdbg_buffer *buffer);
size_t pdbg_buffer_diagnostic_count(const pdbg_buffer *buffer);
pdbg_status pdbg_buffer_diagnostic_get(
    const pdbg_buffer *buffer,
    size_t index,
    pdbg_diagnostic *out,
    pdbg_error *err);

uint32_t pdbg_image_width(const pdbg_image *image);
uint32_t pdbg_image_height(const pdbg_image *image);
size_t pdbg_image_stride(const pdbg_image *image);
const uint8_t *pdbg_image_rgba_pixels(const pdbg_image *image);
size_t pdbg_image_diagnostic_count(const pdbg_image *image);
pdbg_status pdbg_image_diagnostic_get(
    const pdbg_image *image,
    size_t index,
    pdbg_diagnostic *out,
    pdbg_error *err);

size_t pdbg_node_list_len(const pdbg_node_list *list);
int pdbg_node_list_has_total_count(const pdbg_node_list *list);
size_t pdbg_node_list_total_count(const pdbg_node_list *list);
pdbg_status pdbg_node_list_get(
    const pdbg_node_list *list,
    size_t index,
    pdbg_dict_entry *out,
    pdbg_error *err);

size_t pdbg_diagnostic_list_len(const pdbg_diagnostic_list *list);
pdbg_status pdbg_diagnostic_list_get(
    const pdbg_diagnostic_list *list,
    size_t index,
    pdbg_diagnostic *out,
    pdbg_error *err);

size_t pdbg_text_page_span_count(const pdbg_text_page *text);
pdbg_status pdbg_text_page_span_get(
    const pdbg_text_page *text,
    size_t index,
    pdbg_text_span *out,
    pdbg_error *err);
```

`pdbg_context_new` is responsible for registering MuPDF document handlers and
installing a process-wide `fz_locks_context` before any worker context is cloned.
Out structs own their heap-allocated strings, stream-summary filter arrays,
diagnostic lists, and nested node lists until their matching cleanup function
runs.

`pdbg_document_open_fd` is the preferred entry point for MCP allowlist roots when
the platform can safely open a file with directory-relative APIs such as
`openat` and `O_NOFOLLOW`. The Rust layer validates and opens the file first,
then passes the already-opened descriptor to the shim. `pdbg_document_open(path)`
is for GUI file-picker paths and platforms where fd/stream opening is not
available; it must not be used to bypass MCP allowlist checks.

`pdbg_document_open_fd` must not consume the caller's file descriptor. The shim
duplicates `fd` at function entry, the caller remains responsible for closing the
original descriptor, and the returned `pdbg_doc` owns the duplicate. On failure,
the shim closes any duplicate it created before returning. If a platform cannot
represent an already-opened file as a POSIX-style fd, it must use a separate
platform-specific stream entry point or the GUI-only path fallback; MCP allowlist
opens must not silently fall back to unchecked paths.

All `char *` fields returned by the shim are UTF-8 and NUL-terminated for C
convenience. Unless a sibling length field is present, returned strings are
display strings and must not contain interior NUL bytes; the shim must replace or
escape such bytes before populating the field. Binary or forensic data must use
an explicit byte pointer and length, such as `pdbg_object_value.bytes` and
`byte_len`. `pdbg_object_value.decoded_text` is a lossy convenience field for
display; `bytes` and `byte_len` are authoritative for PDF string inspection.
`pdbg_text_span.text` may contain interior NUL as valid UTF-8 text data, so Rust
must copy exactly `text_len` bytes and treat the trailing NUL only as a C
terminator.

Accessor pointers are borrowed from their owning handle and are valid only until
the matching drop or cleanup function runs. Rust must copy bytes, pixels, text,
diagnostics, and strings into owned DTOs before releasing the handle.
`pdbg_node_list_get` returns dictionary entries when the list represents
dictionary children and ordinary node summaries otherwise; for ordinary node
summaries `key` is `NULL`. `pdbg_node_list_has_total_count` and
`pdbg_node_list_total_count` map to `ChildPage.total`. A `NULL`
`pdbg_diagnostic_list *` means an empty diagnostics list.

`pdbg_object_detail_out.children` and `dictionary_entries` contain only a bounded
first page of children, using the product default child-page size. Large arrays
and dictionaries must be continued through `pdbg_node_children(offset, limit)`.
`pdbg_dict_entry.max_diagnostic_severity` and `diagnostic_count` are summary
badges for fast tree rendering; `pdbg_dict_entry.diagnostics` is the full
diagnostic list for `ObjectSummary.diagnostics` when available. Full diagnostics
are also available from `pdbg_object_detail_out.diagnostics`,
`pdbg_document_summary_out.diagnostics`, and the buffer/image diagnostic
accessors. `pdbg_object_detail_out.stream` maps to `ObjectDetail.stream` when
`has_stream` is true.

DTO conversion map:

| Rust DTO | C wire / accessors |
| --- | --- |
| `DocumentSummary` | `pdbg_document_summary_out`, `metadata`, `permissions`, safety fields, and `diagnostics` |
| `ObjectSummary` | `pdbg_dict_entry`, list context for array index, node-token registry, and `diagnostics` |
| `ObjectDetail` | `pdbg_object_detail_out`, `children`, `dictionary_entries`, `stream`, and `diagnostics` |
| `StreamSummary` | `pdbg_stream_summary` inside `pdbg_object_detail_out` when `has_stream` is true |
| `StreamChunk` | `pdbg_buffer_*` accessors plus request `mode` and `offset` |
| `RenderResult` | `pdbg_image_*` accessors plus request `page_index`; `duration_ms` is measured by Rust scheduling code |
| `TextPage` / `TextSpan` | `pdbg_text_page_span_count` and `pdbg_text_page_span_get`; `TextPage.page_index` comes from the request |
| `DiagnosticSummary` | `pdbg_diagnostic` plus node-token registry conversion for optional `node` |

### 7.5 Ownership Rules

- The C shim owns MuPDF handles.
- Rust owns returned copied DTO data after conversion.
- Any `pdbg_buffer`, `pdbg_image`, or `pdbg_node_list` allocated by the shim must
  have a matching explicit drop function.
- No borrowed MuPDF pointer is returned to Rust.
- Strings returned to Rust must be copied.
- Stream data returned to Rust must be copied.
- MuPDF `pdf_obj *` values returned by lookup helpers such as dictionary or
  array accessors are borrowed unless the API explicitly says otherwise. The
  shim must call `pdf_keep_obj` before storing such an object beyond the current
  expression and must call `pdf_drop_obj` exactly once for each owned reference.
- Each opaque shim handle must document which MuPDF references it owns. Drops
  must tolerate partially initialized handles and must run on a compatible
  MuPDF context.

### 7.6 Error Boundary Pattern

```c
pdbg_status pdbg_object_detail(..., pdbg_error *err)
{
    fz_context *ctx = doc->ctx;
    pdbg_status status = PDBG_OK;

    fz_try(ctx)
    {
        /* MuPDF calls here. */
    }
    fz_catch(ctx)
    {
        pdbg_fill_error(ctx, err);
        status = pdbg_map_error(ctx);
    }

    return status;
}
```

Shim functions must not `return`, `goto`, or `longjmp` out of an `fz_try` or
`fz_always` block. A local `break` may be used only to leave the current MuPDF
macro block in the way MuPDF documents.

The example helper names `pdbg_fill_error` and `pdbg_map_error` are illustrative;
implementations may use crate-local helper names as long as every exported shim
function maps MuPDF errors to `pdbg_status` and `pdbg_error` before returning.

No shim function may call into Rust while holding an unsafe MuPDF pointer unless
the callback contract is explicitly documented and locked. Any Rust callback
that can be invoked from C must catch panics or the crate must set
`panic = "abort"` and document that choice.

### 7.7 Cancellation And Limits

Long operations must accept a `pdbg_cancel_token`. Cancellation is cooperative and
best effort at the granularity reported by `MuPdfCapabilities`. The Rust
scheduler sets the token when the user cancels a task or when a tool timeout
fires; a cancelled operation returns `PDBG_ERROR_CANCELLED` when the MuPDF backend
can observe cancellation before completion.

The MuPDF shim maps this token to a MuPDF `fz_cookie` where MuPDF supports it,
especially page rendering and display list execution. Operations that cannot be
interrupted mid-call must report `CancellationCapability::BetweenOperationsOnly`
and check cancellation before starting the next bounded unit of work.

For stream and text operations that are not fully covered by `fz_cookie`, the
shim must read through bounded streams and enforce:

- maximum decoded bytes while decoding;
- maximum filter expansion ratio;
- maximum object recursion depth;
- maximum render pixels and output bytes.

These limits are configured through `pdbg_open_options` and
`pdbg_render_options`. A zero limit means "use the product default", not
"unbounded".

Killing worker threads is not an acceptable cancellation mechanism. In a hardened
or network-exposed build that runs parsing and rendering in a dedicated
low-privilege worker process, the MuPDF backend may report
`CancellationCapability::IsolatedProcessAbort`, and process termination may be
used as a last-resort abort for operations that cannot provide finer-grained
cancellation. Such process termination is an isolation boundary behavior, not an
in-process cancellation mechanism; the process must own no GUI state, and the
app must recreate a clean worker before continuing.

### 7.8 Build And Vendoring

Milestone 0 must choose and document:

- MuPDF source strategy: vendored pinned release tarball or pinned git submodule;
- dependency strategy for freetype, harfbuzz, jbig2dec, openjpeg, zlib, and other
  MuPDF third-party libraries;
- static versus dynamic linking;
- `build.rs` integration versus invoking MuPDF's build system;
- bindgen input headers and generated binding policy;
- supported MuPDF version range and upgrade procedure.

This decision must be compatible with the licensing plan in Section 15.

## 8. MCP Boundary

### 8.1 MCP Positioning

MCP is an optional read-only interface over the same Rust backend. It should not
be a separate PDF parser and should not bypass the `DocumentSession`.

```text
MCP client / LLM agent
  |
  v
MCP server
  |
  v
Rust backend services
  |
  v
DocumentSession
```

The MCP server must be safe by default. It should expose bounded diagnostic
tools, not arbitrary file-system or PDF-editing access.

### 8.2 Initial MCP Tool Set

```text
pdf_open
pdf_close
pdf_info
pdf_get_object
pdf_get_object_children
pdf_get_stream
pdf_search_objects
pdf_search_text
pdf_render_page
pdf_get_artifact
pdf_extract_text
pdf_diagnose_document
pdf_diagnose_page
```

This is the initial post-MVP MCP tool set for Milestone 4, not part of the
desktop MVP.

### 8.3 Tool Contracts

#### pdf_open

Input:

- path;
- optional password.

Output:

- document id;
- document summary.

Security:

- path must be inside configured allowlist roots;
- no URL loading in the first version;
- encrypted documents require explicit password input.

Path validation must be performed before MuPDF opens a file:

- canonicalize each configured root at configuration load time;
- canonicalize the requested path before opening it;
- require the canonical requested path to be a path-component descendant of one
  canonical root, not merely a string-prefix match;
- reject canonicalization failures;
- reject or safely handle symbolic links that can escape a root. On platforms
  where it is available, prefer `openat`/`O_NOFOLLOW` style file opening and
  pass the already-opened file descriptor into `pdbg_document_open_fd` to reduce
  TOCTOU windows.

#### pdf_get_object

Input:

- document id;
- object id or node id;
- max output bytes.

Output:

- object kind;
- preview;
- dictionary/array entries if small;
- stream metadata;
- diagnostics.

The tool must not return unbounded streams.

#### pdf_get_stream

Input:

- document id;
- object id;
- raw or decoded;
- offset;
- limit;
- output mode: text, hex, or base64.

Output:

- chunk;
- truncation flag;
- decode diagnostics.

Default limits should be small enough for LLM context.

#### pdf_render_page

Input:

- document id;
- page index;
- max width;
- max height;
- max pixels;
- max output bytes;
- rotation;
- optional layer config.

Output:

- image artifact reference or base64 thumbnail;
- dimensions;
- timing;
- diagnostics.

Large images should be returned as artifacts, not inline text.

#### pdf_get_artifact

Input:

- artifact id;
- max output bytes.

Output:

- bytes or base64;
- media type;
- dimensions if the artifact is an image;
- truncation flag.

Artifact ids are unguessable, scoped to the issuing session and client, and
expire according to the artifact store TTL/LRU policy.

#### pdf_diagnose_page

Input:

- document id;
- page index.

Output:

- resource summary;
- render warnings;
- missing fonts/images;
- slow-path hints;
- annotation/form appearance issues.

### 8.4 MCP Security Rules

- Read-only by default.
- No save, overwrite, delete, redact, or edit tools in the initial MCP tool set.
- File access allowlist.
- Per-tool timeout.
- Per-tool byte output limit.
- Per-tool decoded byte limit.
- Per-document memory budget.
- Per-page render resolution and output byte cap.
- OCR disabled by default for MCP unless explicitly enabled.
- Return extracted PDF text inside structured fields marked `untrusted: true`;
  do not rely on prose warnings alone.
- Do not execute PDF JavaScript. This is an application-wide safe-mode default,
  not only an MCP rule.
- Do not expose arbitrary low-level object mutation.
- Default transport is stdio or localhost-only. Any network transport must be
  disabled by default and must require authentication.
- Artifact references must be unguessable, scoped to the issuing session and
  client, counted against memory limits, and evicted by TTL or LRU.

### 8.5 Future MCP Tools

Future tools may include:

- `pdf_get_resource_graph`;
- `pdf_get_font_diagnostics`;
- `pdf_get_image_diagnostics`;
- `pdf_get_annotations`;
- `pdf_get_signature_diagnostics`;
- `pdf_explain_object_context`.

Any future write tool must require explicit user confirmation in the GUI or a
separate trusted policy file.

## 9. Application Safety Model

PDF files are untrusted input. The first version opens every document in safe
mode:

- JavaScript is disabled for GUI and MCP paths.
- OpenAction and additional-action JavaScript are not executed.
- OCR is disabled unless explicitly enabled by the user.
- External file references are detected and reported but not followed.
- Embedded files are listed as diagnostics but not extracted automatically.
- URL loading is out of scope for the first version.

Safe mode is represented by `pdbg_open_options`, `DocumentSafetyState`, and
document diagnostics. Future dangerous operations such as JavaScript execution,
file extraction, saving, overwriting, redaction, or arbitrary MCP file exposure
must require explicit user confirmation and a separate policy decision.

All GUI and report egress treats PDF-controlled text as untrusted. Copy actions
may copy bounded plain text, but they must not copy hidden markup. Markdown
reports must escape Markdown metacharacters in PDF text or place it in fenced
code blocks. HTML reports must HTML-escape all PDF text and must not inline
active content. JSON reports must preserve untrusted text in data fields, not as
executable templates.

## 10. UI Architecture

> UI reference (functional, not visual): `docs/ui/pdfbox-reference-notes.md`
> distills borrowable PDFBox PDFDebugger workflows (tree node anatomy, two-axis
> stream view, breadcrumb, operator highlighting, overlays), each tagged with
> the milestone where it applies. `docs/ui/pdbg-ui-design-v1.svg` is the visual
> target.

### 10.1 egui Layout

Initial layout:

```text
Top menu / toolbar
  Open | Search | Render settings | MCP status

Left panel
  Document tree

Center panel
  Page preview

Right panel
  Object inspector / stream viewer / diagnostics

Bottom panel
  Logs / task status / warnings
```

### 10.2 UI State

The UI state stores:

- open document ids;
- selected node id;
- expanded tree nodes;
- navigation history: a back/forward stack of visited `NodeId`s (browser-style),
  so the user can retrace cross-reference jumps;
- current page index;
- zoom and rotation;
- active stream view mode;
- search query;
- async task states.

The UI state does not store MuPDF pointers or borrowed buffers.

Indirect references are first-class navigation. When the inspector shows a value
that is an indirect reference (for example `3 0 R`), it is rendered as a
clickable link; activating it resolves the reference via `NodeModel::resolve_ref`
to a `NodeId`, selects and expands that node in the tree, and pushes the prior
selection onto the back stack. The toolbar exposes back/forward controls bound to
the navigation history.

Any text the UI copies or exports from document content (object values, stream
excerpts, reports) is PDF-controlled and therefore untrusted: it must pass
through the egress escaping rules in Section 9 (bounded plain text, no inlined
active content) rather than being copied raw.

## 11. Diagnostics Model

```rust
pub const PUBLIC_SCHEMA_VERSION: u32 = 1;
pub const DIAGNOSTIC_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug)]
pub struct DiagnosticSummary {
    pub severity: DiagnosticSeverity,
    pub code: DiagnosticCode,
    pub message: String,
    pub node: Option<NodeId>,
    pub page_index: Option<usize>,
    pub object: Option<ObjectId>,
}

#[derive(Clone, Debug)]
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
```

Diagnostics should be attached to object summaries where possible and also
available through a document-level diagnostics panel. Public JSON, Markdown
reports, and MCP outputs use `PUBLIC_SCHEMA_VERSION` for the outer payload
schema. Any payload that contains diagnostics must also expose
`DIAGNOSTIC_SCHEMA_VERSION` as `diagnostic_schema_version`.

`DiagnosticCode` serializes as stable lowercase strings:

- `missing_object`;
- `broken_xref_entry`;
- `stream_decode_failure`;
- `encryption_password_failure`;
- `repair_warning`;
- `javascript_disabled`;
- `embedded_file_detected`;
- `external_reference_detected`;
- `resource_missing`;
- `render_warning`;
- `unknown`.

New codes may be added, but existing code names must not be repurposed.

## 12. Testing Strategy

### 12.1 Unit Tests

- Node id serialization/deserialization.
- Object summary conversion.
- Object detail, stream summary, and diagnostic wire conversion.
- Stream chunk paging.
- Error mapping.
- MCP input validation.
- Allowlist canonicalization.
- Safe-mode open options.
- MuPDF feature-capability gating for UI panels and MCP tools.
- FFI string conversion, including text spans with explicit lengths and
  interior NUL bytes.
- GUI, Markdown, HTML, and JSON egress escaping for PDF-controlled text.

### 12.2 Integration Tests

- Open normal PDF.
- Open password PDF.
- Open damaged PDF.
- Inspect trailer/root/pages.
- Load raw and decoded streams.
- Render first page.
- Search text.
- Open a PDF containing OpenAction or form JavaScript and verify it is not
  executed in safe mode.
- Open through `pdbg_document_open_fd` and verify descriptor ownership on success
  and failure.
- Read buffers, images, node lists, and text pages through the opaque handle
  accessors before and after cleanup in sanitizer builds.

### 12.3 Golden Tests

- Object tree snapshots for small fixture PDFs.
- Stream decode output for known PDFs.
- Rendered thumbnail hashes with tolerances.
- Text coordinate normalization snapshots for positioned text fixtures.
- MCP tool JSON outputs.
- `SerializedNodeId` JSON output for representative tree paths.
- `pdf_render_page` artifact references and `pdf_get_artifact` retrieval output.

### 12.4 Stress Tests

- Large object count PDF.
- Large image PDF.
- Deeply nested arrays/dictionaries.
- Object cycles.
- Damaged xref.
- Slow stream decode.
- Rapid page navigation and cancellation.
- Stream decompression bombs.
- Path traversal and symlink escape attempts for MCP allowlists.

### 12.5 Fuzz And Sanitizer Tests

- Fuzz open, object traversal, stream decode, and DTO conversion.
- Run the C shim and MuPDF integration tests under ASAN/UBSan in CI.
- Maintain a small malicious and damaged PDF corpus, including historical crash
  regressions where licensing permits storage.

## 13. Initial Milestones

### Milestone 0: Repository Skeleton

- Rust workspace.
- C shim crate.
- egui desktop crate.
- docs.
- CI skeleton.
- MuPDF build/vendoring decision.
- Licensing: AGPL-3.0 (resolved; see Section 15).
- safe-mode defaults.

Milestone 0 is complete only when the repository has a runnable contract-test
baseline. These tests may use synthetic DTOs, fake shim handles, or tiny fixture
PDFs where the MuPDF-backed implementation is not ready yet; later milestones
must replace fakes with real integration coverage instead of weakening the
contract.

Acceptance checklist:

- [ ] Workspace CI runs the agreed baseline commands for formatting, Rust unit
  tests, C shim build or compile smoke tests, and raw ABI drift checks.
- [ ] C shim static gate rejects `return`, `goto`, or `longjmp` inside
  `fz_try`/`fz_always` blocks. M0 may implement this as a conservative script or
  code-review lint; Milestone 1 must replace or supplement it with MuPDF-backed
  malformed-PDF loop tests.
- [ ] MuPDF source, third-party dependency, linking, bindgen, supported OS/build
  matrix, and upgrade policy are documented.
- [ ] AGPL-3.0 compliance is set up before any binary distribution: a
  corresponding-source publication path, a regenerated `NOTICES` covering MuPDF
  and every bundled dependency, and (for any network MCP transport) a section 13
  source-offer.
- [ ] Safe-mode defaults are represented in config and covered by unit tests:
  JavaScript disabled, OCR disabled unless opted in, no URL loading, and bounded
  resource defaults.
- [ ] `MuPdfCapabilities` contract tests cover fake MuPDF capability sets with
  missing render, positioned-text, OCR, incremental-update, and
  repair-diagnostic support. UI panels and MCP tools must hide, disable, or
  return `PDBG_ERROR_UNSUPPORTED` according to those capabilities.
- [ ] `SerializedNodeId` has golden JSON tests for representative tree paths,
  lowercase segment tags, object ids with generation, `schema_version`, and no
  exposed `path_token`.
- [ ] Node-token registry tests cover dictionary and array child conversion,
  reverse lookup for shim calls, diagnostic node conversion, and the fallback
  behavior for unknown path tokens.
- [ ] Diagnostic code serialization has golden tests for stable lowercase code
  names, `DIAGNOSTIC_SCHEMA_VERSION`, and public schema version fields.
- [ ] C wire enum conversion tests cover `pdbg_object_kind`, `pdbg_resource_group`,
  `pdbg_color_mode`, `pdbg_repair_policy`, `pdbg_diagnostic_severity`, and
  `pdbg_diagnostic_code`, including the stable `javascript_disabled` and
  `xobjects` public strings.
- [ ] Diagnostic wire tests cover `pdbg_diagnostic`, `pdbg_diagnostic_list`,
  object-summary diagnostics from `pdbg_dict_entry`, document-summary
  diagnostics, object-detail diagnostics, stream-buffer diagnostics, and
  render-image diagnostics.
- [ ] Stream summary wire tests cover `pdbg_stream_summary` fields from
  `pdbg_object_detail_out`, including filters, raw/decoded size hints,
  `can_decode`, and `image_preview_available`.
- [ ] FFI string conversion tests cover UTF-8, NUL termination, nullable display
  fields, `pdbg_text_span.text_len`, interior NUL bytes in extracted text, and the
  rule that `pdbg_object_value.bytes`/`byte_len` are authoritative over
  lossy `decoded_text`.
- [ ] Text coordinate golden tests cover normalized top-left page space,
  CropBox/MediaBox selection, page dictionary rotation, and the rule that viewer
  zoom or explicit render-request rotation is applied by the consumer, not stored
  in `pdbg_text_span`.
- [ ] Rust-to-C callback boundary tests cover the chosen panic policy. If the
  crate uses `catch_unwind`, a fake callback that panics must be caught and
  mapped to a normal error. If the crate uses `panic = "abort"`, the abort
  behavior must be documented and covered by a process-level test.
- [ ] Limit-enforcement contract tests cover fake stream decoding that exceeds
  `max_decoded_stream_bytes` or the filter expansion ratio and returns
  `PDBG_ERROR_LIMIT` during decoding, before the full decoded output is
  materialized.
- [ ] GUI/report egress tests cover bounded plaintext copy, Markdown escaping or
  fenced code blocks, HTML escaping with no active content, and JSON data-field
  output for PDF-controlled text.
- [ ] MCP allowlist tests cover canonical root setup, path-component descendant
  checks, rejected canonicalization failures, `..` traversal, symlink escape
  attempts, and the no-URL rule.
- [ ] `pdbg_document_open_fd` ownership tests cover success, failure, and caller
  ownership of the original descriptor. Platforms without POSIX-style fds must
  document the alternate stream API or the GUI-only fallback.
- [ ] Opaque handle accessor tests cover buffers, images, node lists, and text
  pages using test handles: valid borrowed access before cleanup, invalid access
  after cleanup in sanitizer builds, and Rust-side copying before drop.
- [ ] MCP artifact contract tests cover unguessable artifact ids, session/client
  scoping, `pdf_render_page` artifact references, `pdf_get_artifact` retrieval,
  truncation, media type, image dimensions, TTL, and LRU eviction behavior.
- [ ] Fuzz/sanitizer jobs are wired in CI even if the initial corpus is small:
  open, object traversal, stream decode, DTO conversion, C shim ASAN/UBSan smoke
  coverage, and ThreadSanitizer coverage for the lock/callback path.
- [ ] Concurrency smoke tests exercise multiple document sessions and worker
  threads through the scheduler. M0 may use a fake shim with a shared fake store
  to validate lock wiring; Milestone 1 must add MuPDF-backed concurrent open and
  object traversal, and later render/text milestones must add their own real
  concurrent coverage.
- [ ] The malicious/damaged fixture policy is documented, including where
  fixtures live, how license-sensitive files are excluded, and how regressions
  are added.

### Milestone 1.0: UI Shell Spike

The UI engine (egui) is named throughout this document but is not yet validated:
M0 ships `pdbg-app` as a headless app-state crate with no `egui`/`eframe`
dependency and no window. Before Milestones 1-5 build feature panels on top of
egui, this spike stands up the real shell over the existing `FakeShim` and
proves egui can carry the dense-debugger UX — while the cost is a fake-shim
spike, not a mid-M3 rewrite.

Implementation status is tracked in `docs/milestone-1-ui-shell-spike.md`.

- A real `eframe` window rendering the Section 10 four-panel layout (document
  tree | page preview | object inspector / stream / diagnostics | log), driven
  by `AppState` over `FakeShim` (no real MuPDF).
- The highest-risk interactions exercised against fake data:
  - a virtualized lazy object tree backed by ~1,000,000 fake nodes that expands
    and scrolls without materializing or re-hashing the whole set per frame;
  - a hex / stream pane over a large fake buffer with text selection and
    egress-escaped copy of a bounded excerpt;
  - indirect-reference cross-jump: clicking a reference value (e.g. `3 0 R`) in
    the inspector resolves it, then selects and expands the target node in the
    tree, with working back/forward navigation history (per Section 10.2);
  - resizable fixed four-panel split panes and HiDPI-correct rendering.
- A recorded go/no-go: egui carries these interactions, or an alternative
  (retained-mode toolkit, or Tauri/web) is chosen now.
- Freeform docking/reordering is not a Milestone 1.0 gate. If users need
  movable panes after the fixed debugger shell proves out, evaluate it as a
  separate post-spike feature instead of mixing it into this toolkit decision.

Exit gate: the four-panel window launches, the three risk interactions are
demonstrably smooth on the fake corpus, and the egui-vs-alternative decision is
recorded. Headless CI keeps testing app-state; the window itself runs locally or
in an optional, non-required GUI job.

### Milestone 1: Open And Inspect

- Open PDF.
- Password handling.
- Document summary.
- Trailer/root/xref lazy tree.
- Object detail panel.
- MuPDF-backed malformed-PDF loop test for `fz_try`/`fz_catch` integrity:
  repeatedly open damaged PDFs on the same context and verify clean
  `PDBG_ERROR_FORMAT` returns without crashes or poisoned exception state.
- MuPDF-backed concurrent open and object traversal smoke test for
  `fz_locks_context` and document-session scheduling.

### Milestone 2: Streams And Pages

- Raw/decoded stream viewer.
- Hex/text view.
- Page list.
- Page render preview.
- Cancellation.

### Milestone 3: Search And Diagnostics

- Object search.
- Text search.
- Basic diagnostics panel.
- Repair/error reporting.

### Milestone 4: MCP Read-Only Server

- MCP transport.
- Read-only tools.
- Allowlist and limits.
- Tool output tests.

### Milestone 5: Rendering Diagnostics

- Operator viewer.
- Resource graph.
- Font/image inspectors.
- Render timing.

## 14. Key Risks

- MuPDF exception handling crossing into Rust.
- Accidental concurrent access to the same document/device.
- Holding invalid MuPDF internal pointers after xref changes.
- Large decoded streams overwhelming memory.
- UI stalls from synchronous rendering.
- PDF prompt injection through extracted document text.
- AGPL-3.0 compliance obligations: corresponding-source distribution, a NOTICES
  file for MuPDF and bundled dependencies, and a section 13 source-offer for any
  network-exposed MCP server (see Section 15).

## 15. Licensing, Distribution, And Isolation

This project ships under AGPL-3.0. MuPDF is used under its AGPL license, not a
commercial Artifex license, so the entire distributed work (the Rust app, the
hand-written C shim and its raw ABI bindings, and MuPDF with its bundled
third-party libraries) is AGPL-3.0. A commercial Artifex license is not
purchased, and closed-source editions are out of scope.

AGPL compliance obligations:

- Corresponding Source: every distributed binary must offer complete
  corresponding source — the app, the shim, build scripts, and the exact MuPDF
  version plus any patches. A regenerated `NOTICES` file must enumerate MuPDF and
  every bundled dependency (freetype, harfbuzz, jbig2dec, openjpeg, zlib, and any
  others) with its license; confirm each is AGPL-compatible before release.
- AGPL section 13 (network use): if the MCP server is reachable by remote users
  or agents over a network, it must prominently offer those users the
  corresponding source. The default stdio / localhost-only transport (Section
  8.4) minimizes this exposure; any opt-in network transport must carry a
  source-offer.
- Paid distribution is permitted by AGPL, but every edition remains
  source-available; differentiation cannot rely on closed binaries.

For crash isolation, the MVP can run MuPDF in-process if safe mode, fuzzing,
sanitizers, and bounded resources are in place. A hardened release or any
network-exposed MCP mode should move parsing and rendering into a low-privilege
worker process with OS resource limits so a parser crash does not take down the
GUI or other documents. In that isolated mode, terminating the worker process is
an acceptable last-resort timeout or crash recovery mechanism; killing in-process
worker threads remains forbidden.

## 16. Recommended First Implementation Choice

Use:

- Rust backend;
- C shim around MuPDF;
- egui for desktop UI;
- a lazy tree model;
- read-only MCP tools after the desktop model is stable.

This keeps the first version small enough to finish while preserving the
architecture needed for large files, rendering diagnostics, and AI/MCP
extensions.
