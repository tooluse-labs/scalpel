#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub mod raw;

#[cfg(test)]
mod tests {
    use super::raw;
    #[cfg(feature = "real-mupdf")]
    use std::ffi::{CStr, CString};
    #[cfg(all(feature = "real-mupdf", unix))]
    use std::os::fd::{AsFd, AsRawFd};
    use std::ptr;

    #[test]
    fn fake_context_smoke() {
        unsafe {
            let mut ctx: *mut raw::pdbg_context = ptr::null_mut();
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_context_new(&mut ctx, &mut err);
            assert_eq!(status, raw::pdbg_status::PDBG_OK);
            assert!(!ctx.is_null());
            raw::pdbg_context_drop(ctx);
        }
    }

    #[cfg(feature = "real-mupdf")]
    fn real_open_options() -> raw::pdbg_open_options {
        raw::pdbg_open_options {
            safe_mode: 1,
            disable_javascript: 1,
            enable_ocr: 0,
            max_store_bytes: 64 * 1024 * 1024,
            max_decoded_stream_bytes: 16 * 1024 * 1024,
            max_filter_expansion_ratio: 100,
            max_object_depth: 128,
            repair_policy: raw::pdbg_repair_policy::PDBG_REPAIR_DEFAULT,
        }
    }

    #[cfg(feature = "real-mupdf")]
    fn minimal_pdf_path() -> CString {
        CString::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/synthetic/minimal.pdf"
        ))
        .unwrap()
    }

    #[cfg(feature = "real-mupdf")]
    fn write_temp_pdf(prefix: &str, bytes: &[u8]) -> std::path::PathBuf {
        let temp_path = std::env::temp_dir().join(format!(
            "pdbg-{}-{}-{}.pdf",
            prefix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&temp_path, bytes).unwrap();
        temp_path
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_opens_minimal_pdf_and_returns_summary() {
        unsafe {
            let mut ctx: *mut raw::pdbg_context = ptr::null_mut();
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_context_new(&mut ctx, &mut err);
            assert_eq!(status, raw::pdbg_status::PDBG_OK);
            assert!(!ctx.is_null());

            let path = minimal_pdf_path();
            let options = real_open_options();

            let mut doc: *mut raw::pdbg_doc = ptr::null_mut();
            let status = raw::pdbg_document_open(
                ctx,
                path.as_ptr(),
                ptr::null(),
                &options,
                &mut doc,
                &mut err,
            );
            assert_eq!(status, raw::pdbg_status::PDBG_OK);
            assert!(!doc.is_null());

            let mut summary = std::mem::zeroed::<raw::pdbg_document_summary_out>();
            let status = raw::pdbg_document_summary(doc, &mut summary, &mut err);
            assert_eq!(status, raw::pdbg_status::PDBG_OK);
            assert_eq!(summary.page_count, 1);
            assert!(summary.xref_size > 0);
            assert_eq!(summary.has_parsed_object_count, 1);
            assert!(summary.parsed_object_count > 0);
            assert_eq!(summary.needs_password, 0);
            assert_eq!(summary.safe_mode, 1);
            assert_eq!(summary.javascript_disabled, 1);
            assert!(!summary.pdf_version.is_null());
            let version = CStr::from_ptr(summary.pdf_version).to_string_lossy();
            assert!(version.starts_with("1."));
            let document_id = summary.document_id;

            raw::pdbg_document_summary_out_drop(&mut summary);

            let root = raw::pdbg_node_id {
                document_id,
                kind: raw::pdbg_node_kind::PDBG_NODE_DOCUMENT_ROOT,
                object: raw::pdbg_object_id { num: 0, gen: 0 },
                has_object: 0,
                page_index: 0,
                path_token: 0,
                decoded: 0,
                resource_group: raw::pdbg_resource_group::PDBG_RESOURCE_FONTS,
            };
            let mut children: *mut raw::pdbg_node_list = ptr::null_mut();
            let status = raw::pdbg_node_children(doc, &root, 0, 16, &mut children, &mut err);
            assert_eq!(status, raw::pdbg_status::PDBG_OK);
            assert_eq!(raw::pdbg_node_list_has_total_count(children), 1);
            assert_eq!(raw::pdbg_node_list_total_count(children), 4);
            assert_eq!(raw::pdbg_node_list_len(children), 4);

            let mut first = std::mem::zeroed::<raw::pdbg_dict_entry>();
            let status = raw::pdbg_node_list_get(children, 0, &mut first, &mut err);
            assert_eq!(status, raw::pdbg_status::PDBG_OK);
            assert_eq!(first.node.kind, raw::pdbg_node_kind::PDBG_NODE_TRAILER);
            assert_eq!(CStr::from_ptr(first.label).to_string_lossy(), "Trailer");

            let mut detail = std::mem::zeroed::<raw::pdbg_object_detail_out>();
            let status = raw::pdbg_object_detail(doc, &first.node, &mut detail, &mut err);
            assert_eq!(status, raw::pdbg_status::PDBG_OK);
            assert_eq!(detail.kind, raw::pdbg_object_kind::PDBG_OBJECT_TRAILER);
            assert!(!detail.preview.is_null());
            assert!(!detail.dictionary_entries.is_null());

            raw::pdbg_object_detail_out_drop(&mut detail);

            let mut pages = std::mem::zeroed::<raw::pdbg_dict_entry>();
            let status = raw::pdbg_node_list_get(children, 2, &mut pages, &mut err);
            assert_eq!(status, raw::pdbg_status::PDBG_OK);
            assert_eq!(pages.node.kind, raw::pdbg_node_kind::PDBG_NODE_PAGE_ROOT);
            let mut page_children: *mut raw::pdbg_node_list = ptr::null_mut();
            let status =
                raw::pdbg_node_children(doc, &pages.node, 0, 16, &mut page_children, &mut err);
            assert_eq!(status, raw::pdbg_status::PDBG_OK);
            assert_eq!(raw::pdbg_node_list_total_count(page_children), 1);
            let mut first_page = std::mem::zeroed::<raw::pdbg_dict_entry>();
            let status = raw::pdbg_node_list_get(page_children, 0, &mut first_page, &mut err);
            assert_eq!(status, raw::pdbg_status::PDBG_OK);
            assert_eq!(first_page.node.kind, raw::pdbg_node_kind::PDBG_NODE_PAGE);
            raw::pdbg_node_list_drop(page_children);

            let mut xref = std::mem::zeroed::<raw::pdbg_dict_entry>();
            let status = raw::pdbg_node_list_get(children, 3, &mut xref, &mut err);
            assert_eq!(status, raw::pdbg_status::PDBG_OK);
            assert_eq!(xref.node.kind, raw::pdbg_node_kind::PDBG_NODE_XREF_ROOT);
            let mut xref_children: *mut raw::pdbg_node_list = ptr::null_mut();
            let status =
                raw::pdbg_node_children(doc, &xref.node, 0, 2, &mut xref_children, &mut err);
            assert_eq!(status, raw::pdbg_status::PDBG_OK);
            assert!(raw::pdbg_node_list_len(xref_children) > 0);
            let mut first_xref = std::mem::zeroed::<raw::pdbg_dict_entry>();
            let status = raw::pdbg_node_list_get(xref_children, 0, &mut first_xref, &mut err);
            assert_eq!(status, raw::pdbg_status::PDBG_OK);
            assert_eq!(
                first_xref.node.kind,
                raw::pdbg_node_kind::PDBG_NODE_XREF_OBJECT
            );
            raw::pdbg_node_list_drop(xref_children);

            raw::pdbg_node_list_drop(children);
            raw::pdbg_document_drop(doc);
            raw::pdbg_context_drop(ctx);
        }
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_malformed_open_loop_preserves_exception_stack() {
        unsafe {
            let mut ctx: *mut raw::pdbg_context = ptr::null_mut();
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_context_new(&mut ctx, &mut err);
            assert_eq!(status, raw::pdbg_status::PDBG_OK);
            assert!(!ctx.is_null());

            let temp_path = write_temp_pdf("malformed", b"this is deliberately not a pdf\n");
            let path = CString::new(temp_path.to_string_lossy().into_owned()).unwrap();
            let options = real_open_options();

            for _ in 0..128 {
                let mut doc: *mut raw::pdbg_doc = ptr::null_mut();
                let mut open_err = raw::pdbg_error::default();
                let status = raw::pdbg_document_open(
                    ctx,
                    path.as_ptr(),
                    ptr::null(),
                    &options,
                    &mut doc,
                    &mut open_err,
                );
                assert!(matches!(
                    status,
                    raw::pdbg_status::PDBG_ERROR_FORMAT
                        | raw::pdbg_status::PDBG_ERROR_UNSUPPORTED
                        | raw::pdbg_status::PDBG_ERROR_GENERIC
                ));
                assert_eq!(open_err.status, status);
                assert!(doc.is_null());
            }

            let good_path = minimal_pdf_path();
            let mut good_doc: *mut raw::pdbg_doc = ptr::null_mut();
            let status = raw::pdbg_document_open(
                ctx,
                good_path.as_ptr(),
                ptr::null(),
                &options,
                &mut good_doc,
                &mut err,
            );
            assert_eq!(status, raw::pdbg_status::PDBG_OK);
            assert!(!good_doc.is_null());
            raw::pdbg_document_drop(good_doc);
            raw::pdbg_context_drop(ctx);
            let _ = std::fs::remove_file(temp_path);
        }
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_repairable_pdf_emits_repair_warning() {
        unsafe {
            let mut ctx: *mut raw::pdbg_context = ptr::null_mut();
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_context_new(&mut ctx, &mut err);
            assert_eq!(status, raw::pdbg_status::PDBG_OK);
            assert!(!ctx.is_null());

            let mut bytes = include_bytes!("../../../fixtures/synthetic/minimal.pdf").to_vec();
            let needle = b"startxref\n184\n";
            let pos = bytes
                .windows(needle.len())
                .position(|window| window == needle)
                .unwrap();
            bytes.splice(pos..pos + needle.len(), b"startxref\n0\n".iter().copied());

            let temp_path = write_temp_pdf("repairable", &bytes);
            let path = CString::new(temp_path.to_string_lossy().into_owned()).unwrap();
            let options = real_open_options();

            let mut doc: *mut raw::pdbg_doc = ptr::null_mut();
            let status = raw::pdbg_document_open(
                ctx,
                path.as_ptr(),
                ptr::null(),
                &options,
                &mut doc,
                &mut err,
            );
            assert_eq!(status, raw::pdbg_status::PDBG_OK);
            assert!(!doc.is_null());

            let mut summary = std::mem::zeroed::<raw::pdbg_document_summary_out>();
            let status = raw::pdbg_document_summary(doc, &mut summary, &mut err);
            assert_eq!(status, raw::pdbg_status::PDBG_OK);
            assert_eq!(summary.repaired_or_damaged, 1);
            assert_eq!(summary.has_parsed_object_count, 1);
            assert!(summary.parsed_object_count > 0);
            assert!(!summary.diagnostics.is_null());
            let diagnostic_count = raw::pdbg_diagnostic_list_len(summary.diagnostics);
            assert!(diagnostic_count > 0);
            let mut saw_repair_warning = false;
            for index in 0..diagnostic_count {
                let mut diagnostic = std::mem::zeroed::<raw::pdbg_diagnostic>();
                let status = raw::pdbg_diagnostic_list_get(
                    summary.diagnostics,
                    index,
                    &mut diagnostic,
                    &mut err,
                );
                assert_eq!(status, raw::pdbg_status::PDBG_OK);
                if diagnostic.code == raw::pdbg_diagnostic_code::PDBG_DIAG_REPAIR_WARNING {
                    assert_eq!(
                        diagnostic.severity,
                        raw::pdbg_diagnostic_severity::PDBG_DIAG_WARNING
                    );
                    saw_repair_warning = true;
                }
            }
            assert!(saw_repair_warning);

            raw::pdbg_document_summary_out_drop(&mut summary);
            raw::pdbg_document_drop(doc);
            raw::pdbg_context_drop(ctx);
            let _ = std::fs::remove_file(temp_path);
        }
    }

    #[cfg(all(feature = "real-mupdf", unix))]
    #[test]
    fn real_mupdf_open_fd_owns_dup_and_preserves_caller_fd() {
        unsafe {
            let mut ctx: *mut raw::pdbg_context = ptr::null_mut();
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_context_new(&mut ctx, &mut err);
            assert_eq!(status, raw::pdbg_status::PDBG_OK);

            let fixture = concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../fixtures/synthetic/minimal.pdf"
            );
            let file = std::fs::File::open(fixture).unwrap();
            let display_path = CString::new("minimal-fd.pdf").unwrap();
            let options = real_open_options();

            let mut doc: *mut raw::pdbg_doc = ptr::null_mut();
            let status = raw::pdbg_document_open_fd(
                ctx,
                file.as_fd().as_raw_fd(),
                display_path.as_ptr(),
                ptr::null(),
                &options,
                &mut doc,
                &mut err,
            );
            assert_eq!(status, raw::pdbg_status::PDBG_OK);
            assert!(!doc.is_null());
            let owned_fd = raw::pdbg_test_document_owned_fd(doc);
            assert!(owned_fd >= 0);
            assert_ne!(owned_fd, file.as_fd().as_raw_fd());
            assert_eq!(raw::pdbg_test_fd_is_open(owned_fd), 1);

            let mut summary = std::mem::zeroed::<raw::pdbg_document_summary_out>();
            let status = raw::pdbg_document_summary(doc, &mut summary, &mut err);
            assert_eq!(status, raw::pdbg_status::PDBG_OK);
            assert_eq!(summary.page_count, 1);
            raw::pdbg_document_summary_out_drop(&mut summary);

            raw::pdbg_document_drop(doc);
            assert_eq!(raw::pdbg_test_fd_is_open(owned_fd), 0);
            assert!(file.metadata().is_ok());
            raw::pdbg_context_drop(ctx);
        }
    }
}
