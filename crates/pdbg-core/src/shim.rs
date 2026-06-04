use crate::dto::*;
use crate::{wire, SafeModeConfig};
use pdbg_shim::raw;
use std::ffi::{CStr, CString};
use std::ptr::{self, NonNull};

#[derive(Debug)]
pub struct ShimError {
    pub status: raw::pdbg_status,
    pub message: String,
}

pub trait Shim {
    fn open_document_summary(&self, path: &str) -> Result<DocumentSummary, ShimError>;
}

pub struct FakeShim {
    ctx: PdbgContext,
}

impl FakeShim {
    pub fn new() -> Result<Self, ShimError> {
        Ok(Self {
            ctx: PdbgContext::new()?,
        })
    }
}

impl Shim for FakeShim {
    fn open_document_summary(&self, path: &str) -> Result<DocumentSummary, ShimError> {
        let doc = self.ctx.open_document(path)?;
        doc.summary()
    }
}

struct PdbgContext {
    raw: NonNull<raw::pdbg_context>,
}

impl PdbgContext {
    fn new() -> Result<Self, ShimError> {
        unsafe {
            let mut ctx = ptr::null_mut();
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_context_new(&mut ctx, &mut err);
            check_status(status, &err)?;
            let raw = NonNull::new(ctx).ok_or_else(|| ShimError {
                status: raw::pdbg_status::PDBG_ERROR_GENERIC,
                message: "pdbg_context_new returned null".to_string(),
            })?;
            Ok(Self { raw })
        }
    }

    fn open_document(&self, path: &str) -> Result<PdbgDoc, ShimError> {
        let path = CString::new(path).map_err(|_| ShimError {
            status: raw::pdbg_status::PDBG_ERROR_GENERIC,
            message: "path contains interior NUL".to_string(),
        })?;
        let options = SafeModeConfig::default().to_raw_open_options();

        unsafe {
            let mut doc = ptr::null_mut();
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_document_open(
                self.raw.as_ptr(),
                path.as_ptr(),
                ptr::null(),
                &options,
                &mut doc,
                &mut err,
            );
            check_status(status, &err)?;
            let raw = NonNull::new(doc).ok_or_else(|| ShimError {
                status: raw::pdbg_status::PDBG_ERROR_GENERIC,
                message: "pdbg_document_open returned null".to_string(),
            })?;
            Ok(PdbgDoc { raw })
        }
    }
}

impl Drop for PdbgContext {
    fn drop(&mut self) {
        unsafe { raw::pdbg_context_drop(self.raw.as_ptr()) }
    }
}

struct PdbgDoc {
    raw: NonNull<raw::pdbg_doc>,
}

impl PdbgDoc {
    fn summary(&self) -> Result<DocumentSummary, ShimError> {
        unsafe {
            let mut out = std::mem::zeroed::<raw::pdbg_document_summary_out>();
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_document_summary(self.raw.as_ptr(), &mut out, &mut err);
            check_status(status, &err)?;
            let summary = convert_document_summary(&out);
            raw::pdbg_document_summary_out_drop(&mut out);
            Ok(summary)
        }
    }
}

impl Drop for PdbgDoc {
    fn drop(&mut self) {
        unsafe { raw::pdbg_document_drop(self.raw.as_ptr()) }
    }
}

fn check_status(status: raw::pdbg_status, err: &raw::pdbg_error) -> Result<(), ShimError> {
    if status == raw::pdbg_status::PDBG_OK {
        return Ok(());
    }
    Err(ShimError {
        status,
        message: c_char_array_to_string(&err.message),
    })
}

fn c_char_array_to_string(bytes: &[std::os::raw::c_char]) -> String {
    unsafe { CStr::from_ptr(bytes.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

unsafe fn convert_document_summary(out: &raw::pdbg_document_summary_out) -> DocumentSummary {
    DocumentSummary {
        doc: DocumentId(out.document_id),
        file_path: wire::copy_c_string(out.file_path),
        file_hash: wire::copy_optional_c_string(out.file_hash),
        pdf_version: wire::copy_optional_c_string(out.pdf_version),
        page_count: out.page_count,
        xref_size: out.xref_size,
        parsed_object_count: (out.has_parsed_object_count != 0).then_some(out.parsed_object_count),
        encrypted: out.encrypted != 0,
        needs_password: out.needs_password != 0,
        permissions: DocumentPermissions {
            print: out.permissions.print != 0,
            modify: out.permissions.modify != 0,
            copy: out.permissions.copy != 0,
            annotate: out.permissions.annotate != 0,
            fill_forms: out.permissions.fill_forms != 0,
            extract_accessibility: out.permissions.extract_accessibility != 0,
            assemble: out.permissions.assemble != 0,
            high_quality_print: out.permissions.high_quality_print != 0,
        },
        metadata_summary: wire::copy_string_pairs(out.metadata, out.metadata_len),
        safety: DocumentSafetyState {
            safe_mode: out.safe_mode != 0,
            javascript_disabled: out.javascript_disabled != 0,
            repaired_or_damaged: out.repaired_or_damaged != 0,
            embedded_files_detected: out.embedded_files_detected != 0,
            external_references_detected: out.external_references_detected != 0,
            ocr_enabled: out.ocr_enabled != 0,
        },
        diagnostics: wire::diagnostic_list(out.diagnostics, &|_| None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_shim_returns_document_summary() {
        let shim = FakeShim::new().unwrap();
        let summary = shim.open_document_summary("fake.pdf").unwrap();
        assert_eq!(summary.file_path, "fake.pdf");
        assert!(summary.safety.javascript_disabled);
        assert_eq!(
            summary.diagnostics[0].code.as_public_str(),
            "repair_warning"
        );
    }

    #[test]
    fn stable_public_strings_are_pinned() {
        assert_eq!(
            DiagnosticCode::JavaScriptDisabled.as_public_str(),
            "javascript_disabled"
        );
        assert_eq!(ResourceGroup::XObjects.as_public_str(), "xobjects");
    }
}
