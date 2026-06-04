use std::os::raw::{c_char, c_double, c_float, c_int, c_void};

#[repr(C)]
pub struct pdbg_context {
    _private: [u8; 0],
}

#[repr(C)]
pub struct pdbg_doc {
    _private: [u8; 0],
}

#[repr(C)]
pub struct pdbg_buffer {
    _private: [u8; 0],
}

#[repr(C)]
pub struct pdbg_image {
    _private: [u8; 0],
}

#[repr(C)]
pub struct pdbg_node_list {
    _private: [u8; 0],
}

#[repr(C)]
pub struct pdbg_diagnostic_list {
    _private: [u8; 0],
}

#[repr(C)]
pub struct pdbg_text_page {
    _private: [u8; 0],
}

#[repr(C)]
pub struct pdbg_cancel_token {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum pdbg_status {
    PDBG_OK = 0,
    PDBG_ERROR_GENERIC = 1,
    PDBG_ERROR_PASSWORD = 2,
    PDBG_ERROR_FORMAT = 3,
    PDBG_ERROR_UNSUPPORTED = 4,
    PDBG_ERROR_CANCELLED = 5,
    PDBG_ERROR_OOM = 6,
    PDBG_ERROR_SECURITY = 7,
    PDBG_ERROR_LIMIT = 8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct pdbg_error {
    pub status: pdbg_status,
    pub mupdf_code: c_int,
    pub message: [c_char; 1024],
}

impl Default for pdbg_error {
    fn default() -> Self {
        Self {
            status: pdbg_status::PDBG_OK,
            mupdf_code: 0,
            message: [0; 1024],
        }
    }
}

pub type pdbg_test_callback =
    Option<unsafe extern "C" fn(user: *mut c_void, err: *mut pdbg_error) -> pdbg_status>;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct pdbg_object_id {
    pub num: c_int,
    pub gen: c_int,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum pdbg_node_kind {
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
    PDBG_NODE_RESOURCE_GROUP = 10,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum pdbg_resource_group {
    PDBG_RESOURCE_FONTS = 0,
    PDBG_RESOURCE_IMAGES = 1,
    PDBG_RESOURCE_XOBJECTS = 2,
    PDBG_RESOURCE_COLOR_SPACES = 3,
    PDBG_RESOURCE_PATTERNS = 4,
    PDBG_RESOURCE_SHADINGS = 5,
    PDBG_RESOURCE_ANNOTATIONS = 6,
    PDBG_RESOURCE_WIDGETS = 7,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct pdbg_node_id {
    pub document_id: u64,
    pub kind: pdbg_node_kind,
    pub object: pdbg_object_id,
    pub has_object: c_int,
    pub page_index: u32,
    pub path_token: u64,
    pub decoded: c_int,
    pub resource_group: pdbg_resource_group,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct pdbg_string_pair {
    pub key: *mut c_char,
    pub value: *mut c_char,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct pdbg_permissions {
    pub print: c_int,
    pub modify: c_int,
    pub copy: c_int,
    pub annotate: c_int,
    pub fill_forms: c_int,
    pub extract_accessibility: c_int,
    pub assemble: c_int,
    pub high_quality_print: c_int,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum pdbg_object_kind {
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
    PDBG_OBJECT_UNKNOWN = 14,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum pdbg_object_value_kind {
    PDBG_VALUE_NULL = 0,
    PDBG_VALUE_BOOL = 1,
    PDBG_VALUE_INT = 2,
    PDBG_VALUE_REAL = 3,
    PDBG_VALUE_NAME = 4,
    PDBG_VALUE_STRING_BYTES = 5,
    PDBG_VALUE_INDIRECT_REF = 6,
    PDBG_VALUE_CONTAINER = 7,
    PDBG_VALUE_UNKNOWN = 8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum pdbg_pdf_string_kind {
    PDBG_STRING_LITERAL = 0,
    PDBG_STRING_HEX = 1,
    PDBG_STRING_UNKNOWN = 2,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum pdbg_diagnostic_severity {
    PDBG_DIAG_INFO = 0,
    PDBG_DIAG_WARNING = 1,
    PDBG_DIAG_ERROR = 2,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum pdbg_diagnostic_code {
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
    PDBG_DIAG_UNKNOWN = 10,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct pdbg_diagnostic {
    pub severity: pdbg_diagnostic_severity,
    pub code: pdbg_diagnostic_code,
    pub message: *mut c_char,
    pub node: pdbg_node_id,
    pub has_node: c_int,
    pub page_index: u32,
    pub has_page_index: c_int,
    pub object: pdbg_object_id,
    pub has_object: c_int,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct pdbg_object_value {
    pub kind: pdbg_object_value_kind,
    pub bool_value: c_int,
    pub int_value: i64,
    pub real_value: c_double,
    pub name_value: *mut c_char,
    pub bytes: *mut u8,
    pub byte_len: usize,
    pub string_kind: pdbg_pdf_string_kind,
    pub is_text_string: c_int,
    pub decoded_text: *mut c_char,
    pub ref_value: pdbg_object_id,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct pdbg_dict_entry {
    pub key: *mut c_char,
    pub node: pdbg_node_id,
    pub object_kind: pdbg_object_kind,
    pub object: pdbg_object_id,
    pub has_object: c_int,
    pub label: *mut c_char,
    pub preview: *mut c_char,
    pub has_children: c_int,
    pub has_stream: c_int,
    pub child_count: usize,
    pub has_child_count: c_int,
    pub byte_size_hint: u64,
    pub has_byte_size_hint: c_int,
    pub max_diagnostic_severity: pdbg_diagnostic_severity,
    pub diagnostic_count: usize,
    pub diagnostics: *mut pdbg_diagnostic_list,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct pdbg_text_span {
    pub text: *mut c_char,
    pub text_len: usize,
    pub x: c_float,
    pub y: c_float,
    pub width: c_float,
    pub height: c_float,
    pub page_index: u32,
    pub untrusted: c_int,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum pdbg_repair_policy {
    PDBG_REPAIR_DEFAULT = 0,
    PDBG_REPAIR_NEVER = 1,
    PDBG_REPAIR_ALLOW = 2,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum pdbg_color_mode {
    PDBG_COLOR_RGBA = 0,
    PDBG_COLOR_GRAYSCALE = 1,
    PDBG_COLOR_INVERTED = 2,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct pdbg_stream_summary {
    pub object: pdbg_object_id,
    pub filters: *mut *mut c_char,
    pub filter_count: usize,
    pub raw_size_hint: u64,
    pub has_raw_size_hint: c_int,
    pub decoded_size_hint: u64,
    pub has_decoded_size_hint: c_int,
    pub can_decode: c_int,
    pub image_preview_available: c_int,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct pdbg_open_options {
    pub safe_mode: c_int,
    pub disable_javascript: c_int,
    pub enable_ocr: c_int,
    pub max_store_bytes: u64,
    pub max_decoded_stream_bytes: u64,
    pub max_filter_expansion_ratio: u32,
    pub max_object_depth: u32,
    pub repair_policy: pdbg_repair_policy,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct pdbg_render_options {
    pub zoom: c_float,
    pub rotation_degrees: c_int,
    pub max_width: u32,
    pub max_height: u32,
    pub max_pixels: u64,
    pub max_output_bytes: u64,
    pub color_mode: pdbg_color_mode,
    pub layer_config_token: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct pdbg_text_options {
    pub sort_by_position: c_int,
    pub include_coordinates: c_int,
    pub max_chars: usize,
    pub max_blocks: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct pdbg_document_summary_out {
    pub document_id: u64,
    pub file_path: *mut c_char,
    pub file_hash: *mut c_char,
    pub pdf_version: *mut c_char,
    pub page_count: usize,
    pub xref_size: usize,
    pub parsed_object_count: usize,
    pub has_parsed_object_count: c_int,
    pub encrypted: c_int,
    pub needs_password: c_int,
    pub permissions: pdbg_permissions,
    pub metadata: *mut pdbg_string_pair,
    pub metadata_len: usize,
    pub safe_mode: c_int,
    pub javascript_disabled: c_int,
    pub repaired_or_damaged: c_int,
    pub embedded_files_detected: c_int,
    pub external_references_detected: c_int,
    pub ocr_enabled: c_int,
    pub diagnostics: *mut pdbg_diagnostic_list,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct pdbg_object_detail_out {
    pub id: pdbg_node_id,
    pub object: pdbg_object_id,
    pub has_object: c_int,
    pub kind: pdbg_object_kind,
    pub label: *mut c_char,
    pub preview: *mut c_char,
    pub value: pdbg_object_value,
    pub children: *mut pdbg_node_list,
    pub dictionary_entries: *mut pdbg_node_list,
    pub has_stream: c_int,
    pub stream: pdbg_stream_summary,
    pub diagnostics: *mut pdbg_diagnostic_list,
}

unsafe extern "C" {
    pub fn pdbg_context_new(out: *mut *mut pdbg_context, err: *mut pdbg_error) -> pdbg_status;
    pub fn pdbg_context_drop(ctx: *mut pdbg_context);

    pub fn pdbg_cancel_token_new(
        out: *mut *mut pdbg_cancel_token,
        err: *mut pdbg_error,
    ) -> pdbg_status;
    pub fn pdbg_cancel_token_cancel(token: *mut pdbg_cancel_token);
    pub fn pdbg_cancel_token_drop(token: *mut pdbg_cancel_token);

    pub fn pdbg_document_open(
        ctx: *mut pdbg_context,
        path: *const c_char,
        password: *const c_char,
        options: *const pdbg_open_options,
        out: *mut *mut pdbg_doc,
        err: *mut pdbg_error,
    ) -> pdbg_status;

    pub fn pdbg_document_open_fd(
        ctx: *mut pdbg_context,
        fd: c_int,
        display_path: *const c_char,
        password: *const c_char,
        options: *const pdbg_open_options,
        out: *mut *mut pdbg_doc,
        err: *mut pdbg_error,
    ) -> pdbg_status;

    pub fn pdbg_document_drop(doc: *mut pdbg_doc);

    pub fn pdbg_document_summary(
        doc: *mut pdbg_doc,
        out: *mut pdbg_document_summary_out,
        err: *mut pdbg_error,
    ) -> pdbg_status;

    pub fn pdbg_node_children(
        doc: *mut pdbg_doc,
        node: *const pdbg_node_id,
        offset: usize,
        limit: usize,
        out: *mut *mut pdbg_node_list,
        err: *mut pdbg_error,
    ) -> pdbg_status;

    pub fn pdbg_object_detail(
        doc: *mut pdbg_doc,
        node: *const pdbg_node_id,
        out: *mut pdbg_object_detail_out,
        err: *mut pdbg_error,
    ) -> pdbg_status;

    pub fn pdbg_stream_load(
        doc: *mut pdbg_doc,
        object: pdbg_object_id,
        decoded: c_int,
        offset: u64,
        limit: usize,
        cancel: *mut pdbg_cancel_token,
        out: *mut *mut pdbg_buffer,
        err: *mut pdbg_error,
    ) -> pdbg_status;

    pub fn pdbg_page_render(
        doc: *mut pdbg_doc,
        page_index: u32,
        options: *const pdbg_render_options,
        cancel: *mut pdbg_cancel_token,
        out: *mut *mut pdbg_image,
        err: *mut pdbg_error,
    ) -> pdbg_status;

    pub fn pdbg_page_extract_text(
        doc: *mut pdbg_doc,
        page_index: u32,
        options: *const pdbg_text_options,
        cancel: *mut pdbg_cancel_token,
        out: *mut *mut pdbg_text_page,
        err: *mut pdbg_error,
    ) -> pdbg_status;

    pub fn pdbg_buffer_drop(buffer: *mut pdbg_buffer);
    pub fn pdbg_image_drop(image: *mut pdbg_image);
    pub fn pdbg_node_list_drop(list: *mut pdbg_node_list);
    pub fn pdbg_text_page_drop(text: *mut pdbg_text_page);
    pub fn pdbg_document_summary_out_drop(out: *mut pdbg_document_summary_out);
    pub fn pdbg_object_detail_out_drop(out: *mut pdbg_object_detail_out);

    pub fn pdbg_buffer_data(buffer: *const pdbg_buffer) -> *const u8;
    pub fn pdbg_buffer_len(buffer: *const pdbg_buffer) -> usize;
    pub fn pdbg_buffer_total_size_hint(buffer: *const pdbg_buffer) -> u64;
    pub fn pdbg_buffer_truncated(buffer: *const pdbg_buffer) -> c_int;
    pub fn pdbg_buffer_diagnostic_count(buffer: *const pdbg_buffer) -> usize;
    pub fn pdbg_buffer_diagnostic_get(
        buffer: *const pdbg_buffer,
        index: usize,
        out: *mut pdbg_diagnostic,
        err: *mut pdbg_error,
    ) -> pdbg_status;

    pub fn pdbg_image_width(image: *const pdbg_image) -> u32;
    pub fn pdbg_image_height(image: *const pdbg_image) -> u32;
    pub fn pdbg_image_stride(image: *const pdbg_image) -> usize;
    pub fn pdbg_image_rgba_pixels(image: *const pdbg_image) -> *const u8;
    pub fn pdbg_image_diagnostic_count(image: *const pdbg_image) -> usize;
    pub fn pdbg_image_diagnostic_get(
        image: *const pdbg_image,
        index: usize,
        out: *mut pdbg_diagnostic,
        err: *mut pdbg_error,
    ) -> pdbg_status;

    pub fn pdbg_node_list_len(list: *const pdbg_node_list) -> usize;
    pub fn pdbg_node_list_has_total_count(list: *const pdbg_node_list) -> c_int;
    pub fn pdbg_node_list_total_count(list: *const pdbg_node_list) -> usize;
    pub fn pdbg_node_list_get(
        list: *const pdbg_node_list,
        index: usize,
        out: *mut pdbg_dict_entry,
        err: *mut pdbg_error,
    ) -> pdbg_status;

    pub fn pdbg_diagnostic_list_len(list: *const pdbg_diagnostic_list) -> usize;
    pub fn pdbg_diagnostic_list_get(
        list: *const pdbg_diagnostic_list,
        index: usize,
        out: *mut pdbg_diagnostic,
        err: *mut pdbg_error,
    ) -> pdbg_status;

    pub fn pdbg_text_page_span_count(text: *const pdbg_text_page) -> usize;
    pub fn pdbg_text_page_span_get(
        text: *const pdbg_text_page,
        index: usize,
        out: *mut pdbg_text_span,
        err: *mut pdbg_error,
    ) -> pdbg_status;

    pub fn pdbg_test_invoke_callback(
        callback: pdbg_test_callback,
        user: *mut c_void,
        err: *mut pdbg_error,
    ) -> pdbg_status;

    pub fn pdbg_test_document_owned_fd(doc: *const pdbg_doc) -> c_int;
    pub fn pdbg_test_fd_is_open(fd: c_int) -> c_int;
}
