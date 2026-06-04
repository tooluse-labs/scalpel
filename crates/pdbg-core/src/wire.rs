#![allow(dead_code)]
// Some wire carriers are conversion-ready before the matching Shim methods land.

use crate::dto::*;
use pdbg_shim::raw;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::slice;

pub(crate) unsafe fn copy_c_string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        String::new()
    } else {
        CStr::from_ptr(ptr).to_string_lossy().into_owned()
    }
}

pub(crate) unsafe fn copy_optional_c_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        None
    } else {
        Some(CStr::from_ptr(ptr).to_string_lossy().into_owned())
    }
}

pub(crate) unsafe fn copy_text_bytes(ptr: *const c_char, len: usize) -> String {
    if ptr.is_null() || len == 0 {
        return String::new();
    }
    let bytes = slice::from_raw_parts(ptr.cast::<u8>(), len);
    String::from_utf8_lossy(bytes).into_owned()
}

pub(crate) unsafe fn copy_bytes(ptr: *const u8, len: usize) -> Vec<u8> {
    if ptr.is_null() || len == 0 {
        Vec::new()
    } else {
        slice::from_raw_parts(ptr, len).to_vec()
    }
}

pub(crate) unsafe fn copy_string_pairs(
    ptr: *const raw::pdbg_string_pair,
    len: usize,
) -> Vec<(String, String)> {
    if ptr.is_null() || len == 0 {
        return Vec::new();
    }

    slice::from_raw_parts(ptr, len)
        .iter()
        .map(|pair| (copy_c_string(pair.key), copy_c_string(pair.value)))
        .collect()
}

pub(crate) fn object_id(raw: raw::pdbg_object_id) -> ObjectId {
    ObjectId {
        num: raw.num,
        gen: raw.gen,
    }
}

pub(crate) fn raw_object_id(object: ObjectId) -> raw::pdbg_object_id {
    raw::pdbg_object_id {
        num: object.num,
        gen: object.gen,
    }
}

pub(crate) fn optional_object_id(raw: raw::pdbg_object_id, has_object: i32) -> Option<ObjectId> {
    (has_object != 0).then_some(object_id(raw))
}

pub(crate) fn object_kind(kind: raw::pdbg_object_kind) -> ObjectKind {
    match kind {
        raw::pdbg_object_kind::PDBG_OBJECT_NULL => ObjectKind::Null,
        raw::pdbg_object_kind::PDBG_OBJECT_BOOL => ObjectKind::Bool,
        raw::pdbg_object_kind::PDBG_OBJECT_INT => ObjectKind::Int,
        raw::pdbg_object_kind::PDBG_OBJECT_REAL => ObjectKind::Real,
        raw::pdbg_object_kind::PDBG_OBJECT_NAME => ObjectKind::Name,
        raw::pdbg_object_kind::PDBG_OBJECT_STRING => ObjectKind::String,
        raw::pdbg_object_kind::PDBG_OBJECT_ARRAY => ObjectKind::Array,
        raw::pdbg_object_kind::PDBG_OBJECT_DICT => ObjectKind::Dict,
        raw::pdbg_object_kind::PDBG_OBJECT_INDIRECT_REF => ObjectKind::IndirectRef,
        raw::pdbg_object_kind::PDBG_OBJECT_STREAM => ObjectKind::Stream,
        raw::pdbg_object_kind::PDBG_OBJECT_PAGE => ObjectKind::Page,
        raw::pdbg_object_kind::PDBG_OBJECT_XREF_ENTRY => ObjectKind::XrefEntry,
        raw::pdbg_object_kind::PDBG_OBJECT_TRAILER => ObjectKind::Trailer,
        raw::pdbg_object_kind::PDBG_OBJECT_METADATA => ObjectKind::Metadata,
        raw::pdbg_object_kind::PDBG_OBJECT_UNKNOWN => ObjectKind::Unknown,
    }
}

pub(crate) fn resource_group(group: raw::pdbg_resource_group) -> ResourceGroup {
    match group {
        raw::pdbg_resource_group::PDBG_RESOURCE_FONTS => ResourceGroup::Fonts,
        raw::pdbg_resource_group::PDBG_RESOURCE_IMAGES => ResourceGroup::Images,
        raw::pdbg_resource_group::PDBG_RESOURCE_XOBJECTS => ResourceGroup::XObjects,
        raw::pdbg_resource_group::PDBG_RESOURCE_COLOR_SPACES => ResourceGroup::ColorSpaces,
        raw::pdbg_resource_group::PDBG_RESOURCE_PATTERNS => ResourceGroup::Patterns,
        raw::pdbg_resource_group::PDBG_RESOURCE_SHADINGS => ResourceGroup::Shadings,
        raw::pdbg_resource_group::PDBG_RESOURCE_ANNOTATIONS => ResourceGroup::Annotations,
        raw::pdbg_resource_group::PDBG_RESOURCE_WIDGETS => ResourceGroup::Widgets,
    }
}

pub(crate) fn diagnostic_severity(severity: raw::pdbg_diagnostic_severity) -> DiagnosticSeverity {
    match severity {
        raw::pdbg_diagnostic_severity::PDBG_DIAG_INFO => DiagnosticSeverity::Info,
        raw::pdbg_diagnostic_severity::PDBG_DIAG_WARNING => DiagnosticSeverity::Warning,
        raw::pdbg_diagnostic_severity::PDBG_DIAG_ERROR => DiagnosticSeverity::Error,
    }
}

pub(crate) fn diagnostic_code(code: raw::pdbg_diagnostic_code) -> DiagnosticCode {
    match code {
        raw::pdbg_diagnostic_code::PDBG_DIAG_MISSING_OBJECT => DiagnosticCode::MissingObject,
        raw::pdbg_diagnostic_code::PDBG_DIAG_BROKEN_XREF_ENTRY => DiagnosticCode::BrokenXrefEntry,
        raw::pdbg_diagnostic_code::PDBG_DIAG_STREAM_DECODE_FAILURE => {
            DiagnosticCode::StreamDecodeFailure
        }
        raw::pdbg_diagnostic_code::PDBG_DIAG_ENCRYPTION_PASSWORD_FAILURE => {
            DiagnosticCode::EncryptionPasswordFailure
        }
        raw::pdbg_diagnostic_code::PDBG_DIAG_REPAIR_WARNING => DiagnosticCode::RepairWarning,
        raw::pdbg_diagnostic_code::PDBG_DIAG_JAVASCRIPT_DISABLED => {
            DiagnosticCode::JavaScriptDisabled
        }
        raw::pdbg_diagnostic_code::PDBG_DIAG_EMBEDDED_FILE_DETECTED => {
            DiagnosticCode::EmbeddedFileDetected
        }
        raw::pdbg_diagnostic_code::PDBG_DIAG_EXTERNAL_REFERENCE_DETECTED => {
            DiagnosticCode::ExternalReferenceDetected
        }
        raw::pdbg_diagnostic_code::PDBG_DIAG_RESOURCE_MISSING => DiagnosticCode::ResourceMissing,
        raw::pdbg_diagnostic_code::PDBG_DIAG_RENDER_WARNING => DiagnosticCode::RenderWarning,
        raw::pdbg_diagnostic_code::PDBG_DIAG_UNKNOWN => DiagnosticCode::Unknown,
    }
}

pub(crate) fn pdf_string_kind(kind: raw::pdbg_pdf_string_kind) -> PdfStringKind {
    match kind {
        raw::pdbg_pdf_string_kind::PDBG_STRING_LITERAL => PdfStringKind::Literal,
        raw::pdbg_pdf_string_kind::PDBG_STRING_HEX => PdfStringKind::Hex,
        raw::pdbg_pdf_string_kind::PDBG_STRING_UNKNOWN => PdfStringKind::Unknown,
    }
}

pub(crate) unsafe fn object_value(value: &raw::pdbg_object_value) -> ObjectValue {
    match value.kind {
        raw::pdbg_object_value_kind::PDBG_VALUE_NULL => ObjectValue::Null,
        raw::pdbg_object_value_kind::PDBG_VALUE_BOOL => ObjectValue::Bool(value.bool_value != 0),
        raw::pdbg_object_value_kind::PDBG_VALUE_INT => ObjectValue::Int(value.int_value),
        raw::pdbg_object_value_kind::PDBG_VALUE_REAL => ObjectValue::Real(value.real_value),
        raw::pdbg_object_value_kind::PDBG_VALUE_NAME => {
            ObjectValue::Name(copy_c_string(value.name_value))
        }
        raw::pdbg_object_value_kind::PDBG_VALUE_STRING_BYTES => ObjectValue::StringBytes {
            bytes: copy_bytes(value.bytes, value.byte_len),
            string_kind: pdf_string_kind(value.string_kind),
            is_text_string: value.is_text_string != 0,
            decoded_text: copy_optional_c_string(value.decoded_text),
        },
        raw::pdbg_object_value_kind::PDBG_VALUE_INDIRECT_REF => {
            ObjectValue::IndirectRef(object_id(value.ref_value))
        }
        raw::pdbg_object_value_kind::PDBG_VALUE_CONTAINER => ObjectValue::Container,
        raw::pdbg_object_value_kind::PDBG_VALUE_UNKNOWN => ObjectValue::Unknown,
    }
}

pub(crate) unsafe fn stream_summary(stream: &raw::pdbg_stream_summary) -> StreamSummary {
    let filters = if stream.filters.is_null() || stream.filter_count == 0 {
        Vec::new()
    } else {
        slice::from_raw_parts(stream.filters, stream.filter_count)
            .iter()
            .map(|filter| copy_c_string(*filter))
            .collect()
    };

    StreamSummary {
        object: object_id(stream.object),
        filters,
        raw_size_hint: (stream.has_raw_size_hint != 0).then_some(stream.raw_size_hint),
        decoded_size_hint: (stream.has_decoded_size_hint != 0).then_some(stream.decoded_size_hint),
        can_decode: stream.can_decode != 0,
        image_preview_available: stream.image_preview_available != 0,
    }
}

pub(crate) unsafe fn text_span(span: &raw::pdbg_text_span) -> TextSpan {
    TextSpan {
        text: copy_text_bytes(span.text, span.text_len),
        bbox: PageRect {
            x: span.x,
            y: span.y,
            width: span.width,
            height: span.height,
        },
        untrusted: span.untrusted != 0,
    }
}

pub(crate) unsafe fn diagnostic_list(
    list: *const raw::pdbg_diagnostic_list,
    resolve_node: &dyn Fn(&raw::pdbg_node_id) -> Option<NodeId>,
) -> Vec<DiagnosticSummary> {
    if list.is_null() {
        return Vec::new();
    }

    let len = raw::pdbg_diagnostic_list_len(list);
    let mut diagnostics = Vec::with_capacity(len);
    for index in 0..len {
        let mut diag = std::mem::zeroed::<raw::pdbg_diagnostic>();
        let mut err = raw::pdbg_error::default();
        if raw::pdbg_diagnostic_list_get(list, index, &mut diag, &mut err)
            == raw::pdbg_status::PDBG_OK
        {
            diagnostics.push(diagnostic(&diag, resolve_node));
        }
    }
    diagnostics
}

pub(crate) unsafe fn diagnostic(
    diag: &raw::pdbg_diagnostic,
    resolve_node: &dyn Fn(&raw::pdbg_node_id) -> Option<NodeId>,
) -> DiagnosticSummary {
    DiagnosticSummary {
        severity: diagnostic_severity(diag.severity),
        code: diagnostic_code(diag.code),
        message: copy_c_string(diag.message),
        node: (diag.has_node != 0)
            .then(|| resolve_node(&diag.node))
            .flatten(),
        page_index: (diag.has_page_index != 0).then_some(diag.page_index as usize),
        object: optional_object_id(diag.object, diag.has_object),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::raw::c_char;

    #[test]
    fn raw_enum_discriminants_are_append_only_guarded() {
        assert_eq!(raw::pdbg_object_kind::PDBG_OBJECT_XREF_ENTRY as i32, 11);
        assert_eq!(raw::pdbg_resource_group::PDBG_RESOURCE_XOBJECTS as i32, 2);
        assert_eq!(
            raw::pdbg_diagnostic_code::PDBG_DIAG_JAVASCRIPT_DISABLED as i32,
            5
        );
        assert_eq!(raw::pdbg_color_mode::PDBG_COLOR_INVERTED as i32, 2);
        assert_eq!(raw::pdbg_repair_policy::PDBG_REPAIR_ALLOW as i32, 2);
    }

    #[test]
    fn copied_text_bytes_preserve_interior_nul() {
        let bytes = [b'A' as c_char, 0, b'B' as c_char];
        let copied = unsafe { copy_text_bytes(bytes.as_ptr(), bytes.len()) };
        assert_eq!(copied.as_bytes(), b"A\0B");
    }
}
