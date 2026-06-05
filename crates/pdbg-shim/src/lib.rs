#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub mod raw;

#[cfg(test)]
mod tests {
    use super::raw;
    #[cfg(feature = "real-mupdf")]
    use std::ffi::{CStr, CString};
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
    #[test]
    fn real_mupdf_opens_minimal_pdf_and_returns_summary() {
        unsafe {
            let mut ctx: *mut raw::pdbg_context = ptr::null_mut();
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_context_new(&mut ctx, &mut err);
            assert_eq!(status, raw::pdbg_status::PDBG_OK);
            assert!(!ctx.is_null());

            let fixture = concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../fixtures/synthetic/minimal.pdf"
            );
            let path = CString::new(fixture).unwrap();
            let options = raw::pdbg_open_options {
                safe_mode: 1,
                disable_javascript: 1,
                enable_ocr: 0,
                max_store_bytes: 64 * 1024 * 1024,
                max_decoded_stream_bytes: 16 * 1024 * 1024,
                max_filter_expansion_ratio: 100,
                max_object_depth: 128,
                repair_policy: raw::pdbg_repair_policy::PDBG_REPAIR_DEFAULT,
            };

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

            raw::pdbg_document_summary_out_drop(&mut summary);
            raw::pdbg_document_drop(doc);
            raw::pdbg_context_drop(ctx);
        }
    }
}
