use crate::dto::*;
use crate::session::FakeSharedStore;
#[cfg(feature = "real-mupdf")]
use crate::{wire, NodeTokenRegistry};
use crate::{ChildContainer, SafeModeConfig};
use pdbg_shim::raw;
#[cfg(feature = "real-mupdf")]
use std::ffi::CString;
#[cfg(all(unix, feature = "real-mupdf"))]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::fd::{BorrowedFd, OwnedFd};
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
#[cfg(feature = "real-mupdf")]
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct ShimError {
    pub status: raw::pdbg_status,
    pub message: String,
    pub diagnostics: Vec<DiagnosticSummary>,
}

impl ShimError {
    pub(crate) fn new(status: raw::pdbg_status, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            diagnostics: Vec::new(),
        }
    }

    #[cfg(feature = "real-mupdf")]
    fn with_diagnostics(
        status: raw::pdbg_status,
        message: impl Into<String>,
        diagnostics: Vec<DiagnosticSummary>,
    ) -> Self {
        Self {
            status,
            message: message.into(),
            diagnostics,
        }
    }
}

pub trait Shim {
    type Document: ShimDocument;

    fn open_document(&self, path: &str) -> Result<Self::Document, ShimError>;

    fn open_document_summary(&self, path: &str) -> Result<DocumentSummary, ShimError> {
        let mut doc = self.open_document(path)?;
        doc.summary()
    }
}

pub trait ShimDocument {
    fn summary(&mut self) -> Result<DocumentSummary, ShimError>;
    fn children(
        &mut self,
        parent: &NodeId,
        range: ChildRange,
        container: ChildContainer,
    ) -> Result<ChildPage, ShimError>;
    fn object_detail(
        &mut self,
        node: &NodeId,
        range: ChildRange,
    ) -> Result<ObjectDetail, ShimError>;
    fn xref_table(&mut self, range: ChildRange) -> Result<XrefTableSlice, ShimError>;
    fn image_preview(
        &mut self,
        object: ObjectId,
        max_dimension: u32,
    ) -> Result<ImagePreview, ShimError>;
    fn stream_save(
        &mut self,
        object: ObjectId,
        mode: StreamMode,
        path: &str,
        max_bytes: u64,
    ) -> Result<StreamSaveOutcome, ShimError>;
    fn stream_load(
        &mut self,
        object: ObjectId,
        mode: StreamMode,
        offset: u64,
        limit: usize,
    ) -> Result<StreamChunk, ShimError>;
    fn render_page(&mut self, request: &RenderRequest) -> Result<RenderResult, ShimError>;
    fn extract_text(&mut self, request: &TextRequest) -> Result<TextPage, ShimError>;
    fn extract_visuals(&mut self, request: &VisualRequest) -> Result<VisualPage, ShimError>;
}

pub struct FakeShim {
    shared_store: FakeSharedStore,
}

static FAKE_NEXT_DOCUMENT_ID: AtomicU64 = AtomicU64::new(1);

impl FakeShim {
    pub fn new() -> Result<Self, ShimError> {
        let shared_store = FakeSharedStore::new();
        shared_store.record_root_lock_context();
        Ok(Self { shared_store })
    }

    pub fn shared_store(&self) -> FakeSharedStore {
        self.shared_store.clone()
    }

    pub fn shared_store_snapshot(&self) -> crate::FakeSharedStoreSnapshot {
        self.shared_store.snapshot()
    }

    pub fn open_document_with_config(
        &self,
        path: &str,
        config: &SafeModeConfig,
    ) -> Result<OpenDocument, ShimError> {
        let doc = FakeDoc::open_path(path, config)?;
        self.shared_store.record_document_open();
        Ok(OpenDocument {
            backend: OpenDocumentBackend::Fake(doc),
            #[cfg(feature = "real-mupdf")]
            registry: NodeTokenRegistry::default(),
        })
    }

    pub fn open_document_with_password_and_config(
        &self,
        path: &str,
        password: &str,
        config: &SafeModeConfig,
    ) -> Result<OpenDocument, ShimError> {
        let _ = password;
        let doc = FakeDoc::open_path(path, config)?;
        self.shared_store.record_document_open();
        Ok(OpenDocument {
            backend: OpenDocumentBackend::Fake(doc),
            #[cfg(feature = "real-mupdf")]
            registry: NodeTokenRegistry::default(),
        })
    }

    #[cfg(unix)]
    pub fn open_document_fd(
        &self,
        fd: BorrowedFd<'_>,
        display_path: &str,
        config: &SafeModeConfig,
    ) -> Result<OpenDocument, ShimError> {
        let doc = FakeDoc::open_fd(fd, display_path, config)?;
        self.shared_store.record_document_open();
        Ok(OpenDocument {
            backend: OpenDocumentBackend::Fake(doc),
            #[cfg(feature = "real-mupdf")]
            registry: NodeTokenRegistry::default(),
        })
    }
}

impl Shim for FakeShim {
    type Document = OpenDocument;

    fn open_document(&self, path: &str) -> Result<Self::Document, ShimError> {
        let doc = FakeDoc::open_path(path, &SafeModeConfig::default())?;
        self.shared_store.record_document_open();
        Ok(OpenDocument {
            backend: OpenDocumentBackend::Fake(doc),
            #[cfg(feature = "real-mupdf")]
            registry: NodeTokenRegistry::default(),
        })
    }
}

#[cfg(feature = "real-mupdf")]
pub struct RealMuPdfShim {
    ctx: Arc<PdbgContext>,
}

#[cfg(feature = "real-mupdf")]
impl RealMuPdfShim {
    pub fn new() -> Result<Self, ShimError> {
        Ok(Self {
            ctx: Arc::new(PdbgContext::new()?),
        })
    }

    pub fn open_document_with_config(
        &self,
        path: &str,
        config: &SafeModeConfig,
    ) -> Result<OpenDocument, ShimError> {
        Ok(OpenDocument {
            backend: OpenDocumentBackend::Real(PdbgDoc::open_path(
                Arc::clone(&self.ctx),
                path,
                None,
                config,
            )?),
            registry: NodeTokenRegistry::default(),
        })
    }

    pub fn open_document_with_password_and_config(
        &self,
        path: &str,
        password: &str,
        config: &SafeModeConfig,
    ) -> Result<OpenDocument, ShimError> {
        Ok(OpenDocument {
            backend: OpenDocumentBackend::Real(PdbgDoc::open_path(
                Arc::clone(&self.ctx),
                path,
                Some(password),
                config,
            )?),
            registry: NodeTokenRegistry::default(),
        })
    }

    #[cfg(unix)]
    pub fn open_document_fd(
        &self,
        fd: BorrowedFd<'_>,
        display_path: &str,
        config: &SafeModeConfig,
    ) -> Result<OpenDocument, ShimError> {
        Ok(OpenDocument {
            backend: OpenDocumentBackend::Real(PdbgDoc::open_fd(
                Arc::clone(&self.ctx),
                fd,
                display_path,
                None,
                config,
            )?),
            registry: NodeTokenRegistry::default(),
        })
    }
}

#[cfg(feature = "real-mupdf")]
impl Shim for RealMuPdfShim {
    type Document = OpenDocument;

    fn open_document(&self, path: &str) -> Result<Self::Document, ShimError> {
        self.open_document_with_config(path, &SafeModeConfig::default())
    }
}

pub struct OpenDocument {
    backend: OpenDocumentBackend,
    #[cfg(feature = "real-mupdf")]
    registry: NodeTokenRegistry,
}

enum OpenDocumentBackend {
    #[cfg(feature = "real-mupdf")]
    Real(PdbgDoc),
    Fake(FakeDoc),
}

impl ShimDocument for OpenDocument {
    fn summary(&mut self) -> Result<DocumentSummary, ShimError> {
        match &self.backend {
            #[cfg(feature = "real-mupdf")]
            OpenDocumentBackend::Real(doc) => doc.summary(&self.registry),
            OpenDocumentBackend::Fake(doc) => Ok(doc.summary()),
        }
    }

    fn children(
        &mut self,
        parent: &NodeId,
        range: ChildRange,
        container: ChildContainer,
    ) -> Result<ChildPage, ShimError> {
        match &self.backend {
            #[cfg(feature = "real-mupdf")]
            OpenDocumentBackend::Real(doc) => {
                let raw_parent = self.raw_node_for(parent)?;
                doc.children(&mut self.registry, &raw_parent, parent, range, container)
            }
            OpenDocumentBackend::Fake(doc) => Ok(doc.children(parent, range, container)),
        }
    }

    fn object_detail(
        &mut self,
        node: &NodeId,
        range: ChildRange,
    ) -> Result<ObjectDetail, ShimError> {
        match &self.backend {
            #[cfg(feature = "real-mupdf")]
            OpenDocumentBackend::Real(doc) => {
                let raw_node = self.raw_node_for(node)?;
                doc.object_detail(&mut self.registry, &raw_node, node, range)
            }
            OpenDocumentBackend::Fake(doc) => Ok(doc.object_detail(node, range)),
        }
    }

    fn xref_table(&mut self, range: ChildRange) -> Result<XrefTableSlice, ShimError> {
        match &self.backend {
            #[cfg(feature = "real-mupdf")]
            OpenDocumentBackend::Real(doc) => doc.xref_table(range),
            OpenDocumentBackend::Fake(doc) => Ok(doc.xref_table(range)),
        }
    }

    fn image_preview(
        &mut self,
        object: ObjectId,
        max_dimension: u32,
    ) -> Result<ImagePreview, ShimError> {
        match &self.backend {
            #[cfg(feature = "real-mupdf")]
            OpenDocumentBackend::Real(doc) => {
                doc.image_preview(object, max_dimension, ptr::null_mut())
            }
            OpenDocumentBackend::Fake(doc) => doc.image_preview(object, max_dimension),
        }
    }

    fn stream_save(
        &mut self,
        object: ObjectId,
        mode: StreamMode,
        path: &str,
        max_bytes: u64,
    ) -> Result<StreamSaveOutcome, ShimError> {
        match &self.backend {
            #[cfg(feature = "real-mupdf")]
            OpenDocumentBackend::Real(doc) => {
                doc.stream_save(object, mode, path, max_bytes, ptr::null_mut())
            }
            OpenDocumentBackend::Fake(doc) => doc.stream_save(object, mode, path, max_bytes, None),
        }
    }

    fn stream_load(
        &mut self,
        object: ObjectId,
        mode: StreamMode,
        offset: u64,
        limit: usize,
    ) -> Result<StreamChunk, ShimError> {
        match &self.backend {
            #[cfg(feature = "real-mupdf")]
            OpenDocumentBackend::Real(doc) => {
                doc.stream_load(&self.registry, object, mode, offset, limit)
            }
            OpenDocumentBackend::Fake(doc) => doc.stream_load(object, mode, offset, limit, None),
        }
    }

    fn render_page(&mut self, request: &RenderRequest) -> Result<RenderResult, ShimError> {
        match &self.backend {
            #[cfg(feature = "real-mupdf")]
            OpenDocumentBackend::Real(doc) => doc.render_page(&self.registry, request),
            OpenDocumentBackend::Fake(doc) => doc.render_page(request, None),
        }
    }

    fn extract_text(&mut self, request: &TextRequest) -> Result<TextPage, ShimError> {
        match &self.backend {
            #[cfg(feature = "real-mupdf")]
            OpenDocumentBackend::Real(doc) => doc.extract_text(request),
            OpenDocumentBackend::Fake(doc) => doc.extract_text(request, None),
        }
    }

    fn extract_visuals(&mut self, request: &VisualRequest) -> Result<VisualPage, ShimError> {
        match &self.backend {
            #[cfg(feature = "real-mupdf")]
            OpenDocumentBackend::Real(doc) => doc.extract_visuals(request),
            OpenDocumentBackend::Fake(doc) => doc.extract_visuals(request, None),
        }
    }
}

impl OpenDocument {
    #[cfg(feature = "real-mupdf")]
    fn raw_node_for(&self, node: &NodeId) -> Result<raw::pdbg_node_id, ShimError> {
        if let Some(raw_node) = self.registry.raw_for(node) {
            return Ok(raw_node);
        }
        raw_node_from_public(node).ok_or_else(|| {
            ShimError::new(
                raw::pdbg_status::PDBG_ERROR_GENERIC,
                "node is not registered in this document session",
            )
        })
    }

    pub fn stream_load_with_cancel_token(
        &mut self,
        object: ObjectId,
        mode: StreamMode,
        offset: u64,
        limit: usize,
        cancel: &CancelToken,
    ) -> Result<StreamChunk, ShimError> {
        match &self.backend {
            #[cfg(feature = "real-mupdf")]
            OpenDocumentBackend::Real(doc) => doc.stream_load_with_cancel(
                &self.registry,
                object,
                mode,
                offset,
                limit,
                cancel.as_mut_ptr(),
            ),
            OpenDocumentBackend::Fake(doc) => {
                doc.stream_load(object, mode, offset, limit, Some(cancel))
            }
        }
    }

    pub fn stream_save_with_cancel_token(
        &mut self,
        object: ObjectId,
        mode: StreamMode,
        path: &str,
        max_bytes: u64,
        cancel: &CancelToken,
    ) -> Result<StreamSaveOutcome, ShimError> {
        match &self.backend {
            #[cfg(feature = "real-mupdf")]
            OpenDocumentBackend::Real(doc) => {
                doc.stream_save(object, mode, path, max_bytes, cancel.as_mut_ptr())
            }
            OpenDocumentBackend::Fake(doc) => {
                doc.stream_save(object, mode, path, max_bytes, Some(cancel))
            }
        }
    }

    pub fn image_preview_with_cancel_token(
        &mut self,
        object: ObjectId,
        max_dimension: u32,
        cancel: &CancelToken,
    ) -> Result<ImagePreview, ShimError> {
        match &self.backend {
            #[cfg(feature = "real-mupdf")]
            OpenDocumentBackend::Real(doc) => {
                doc.image_preview(object, max_dimension, cancel.as_mut_ptr())
            }
            OpenDocumentBackend::Fake(doc) => {
                let _ = cancel;
                doc.image_preview(object, max_dimension)
            }
        }
    }

    pub fn render_page_with_cancel_token(
        &mut self,
        request: &RenderRequest,
        cancel: &CancelToken,
    ) -> Result<RenderResult, ShimError> {
        match &self.backend {
            #[cfg(feature = "real-mupdf")]
            OpenDocumentBackend::Real(doc) => {
                doc.render_page_with_cancel(&self.registry, request, cancel.as_mut_ptr())
            }
            OpenDocumentBackend::Fake(doc) => doc.render_page(request, Some(cancel)),
        }
    }

    pub fn extract_text_with_cancel_token(
        &mut self,
        request: &TextRequest,
        cancel: &CancelToken,
    ) -> Result<TextPage, ShimError> {
        match &self.backend {
            #[cfg(feature = "real-mupdf")]
            OpenDocumentBackend::Real(doc) => {
                doc.extract_text_with_cancel(request, cancel.as_mut_ptr())
            }
            OpenDocumentBackend::Fake(doc) => doc.extract_text(request, Some(cancel)),
        }
    }

    pub fn extract_visuals_with_cancel_token(
        &mut self,
        request: &VisualRequest,
        cancel: &CancelToken,
    ) -> Result<VisualPage, ShimError> {
        match &self.backend {
            #[cfg(feature = "real-mupdf")]
            OpenDocumentBackend::Real(doc) => {
                doc.extract_visuals_with_cancel(request, cancel.as_mut_ptr())
            }
            OpenDocumentBackend::Fake(doc) => doc.extract_visuals(request, Some(cancel)),
        }
    }

    #[cfg(all(test, unix))]
    fn fake_owned_fd_raw(&self) -> Option<i32> {
        match &self.backend {
            OpenDocumentBackend::Fake(doc) => {
                doc._owned_fd.as_ref().map(std::os::fd::AsRawFd::as_raw_fd)
            }
            #[cfg(feature = "real-mupdf")]
            OpenDocumentBackend::Real(_) => None,
        }
    }
}

struct FakeDoc {
    document_id: DocumentId,
    max_decoded_stream_bytes: u64,
    #[cfg(unix)]
    _owned_fd: Option<OwnedFd>,
}

impl FakeDoc {
    fn open_path(path: &str, config: &SafeModeConfig) -> Result<Self, ShimError> {
        if path == "fail-open" {
            return Err(ShimError::new(
                raw::pdbg_status::PDBG_ERROR_GENERIC,
                "fake open failure",
            ));
        }
        Ok(Self {
            document_id: DocumentId(FAKE_NEXT_DOCUMENT_ID.fetch_add(1, Ordering::Relaxed)),
            max_decoded_stream_bytes: config.max_decoded_stream_bytes,
            #[cfg(unix)]
            _owned_fd: None,
        })
    }

    #[cfg(unix)]
    fn open_fd(
        fd: BorrowedFd<'_>,
        display_path: &str,
        config: &SafeModeConfig,
    ) -> Result<Self, ShimError> {
        if display_path == "fail-open" {
            return Err(ShimError::new(
                raw::pdbg_status::PDBG_ERROR_GENERIC,
                "fake open failure",
            ));
        }
        let owned_fd = fd
            .try_clone_to_owned()
            .map_err(|err| ShimError::new(raw::pdbg_status::PDBG_ERROR_GENERIC, err.to_string()))?;
        Ok(Self {
            document_id: DocumentId(FAKE_NEXT_DOCUMENT_ID.fetch_add(1, Ordering::Relaxed)),
            max_decoded_stream_bytes: config.max_decoded_stream_bytes,
            _owned_fd: Some(owned_fd),
        })
    }

    fn summary(&self) -> DocumentSummary {
        DocumentSummary {
            doc: self.document_id.clone(),
            file_path: "fake.pdf".to_string(),
            file_hash: Some("fake-hash".to_string()),
            pdf_version: Some("1.7".to_string()),
            page_count: 1,
            xref_size: 3,
            parsed_object_count: Some(3),
            encrypted: false,
            needs_password: false,
            permissions: DocumentPermissions {
                print: true,
                modify: false,
                copy: true,
                annotate: false,
                fill_forms: false,
                extract_accessibility: false,
                assemble: false,
                high_quality_print: false,
            },
            metadata_summary: Vec::new(),
            safety: DocumentSafetyState {
                safe_mode: true,
                javascript_disabled: true,
                repaired_or_damaged: false,
                embedded_files_detected: false,
                external_references_detected: false,
                ocr_enabled: false,
            },
            diagnostics: vec![
                fake_diagnostic(
                    DiagnosticCode::RepairWarning,
                    "fake document diagnostic",
                    None,
                ),
                DiagnosticSummary {
                    severity: DiagnosticSeverity::Info,
                    code: DiagnosticCode::JavaScriptDisabled,
                    message: "JavaScript execution is disabled".to_string(),
                    node: None,
                    page_index: None,
                    object: None,
                },
            ],
        }
    }

    fn children(&self, parent: &NodeId, range: ChildRange, container: ChildContainer) -> ChildPage {
        fake_child_page(parent, range, container)
    }

    fn object_detail(&self, node: &NodeId, range: ChildRange) -> ObjectDetail {
        let dictionary_page = fake_child_page(node, range, ChildContainer::Dictionary);
        let array_page = fake_child_page(node, range, ChildContainer::Array);
        ObjectDetail {
            id: node.clone(),
            kind: ObjectKind::Dict,
            object: Some(ObjectId { num: 1, gen: 0 }),
            label: "Fake object".to_string(),
            preview: "<< /Type /Fake >>".to_string(),
            value: ObjectValue::Container,
            dictionary_entries: Some(ChildPage {
                total: dictionary_page.total,
                items: dictionary_page
                    .items
                    .into_iter()
                    .map(|value| {
                        let key = match &value.id {
                            NodeId::DictEntry { key, .. } => key.clone(),
                            _ => String::new(),
                        };
                        DictEntryDetail { key, value }
                    })
                    .collect(),
            }),
            array_entries: Some(array_page),
            stream: Some(fake_stream_summary()),
            diagnostics: vec![fake_diagnostic(
                DiagnosticCode::RepairWarning,
                "fake object diagnostic",
                Some(node.clone()),
            )],
        }
    }

    fn xref_table(&self, range: ChildRange) -> XrefTableSlice {
        // Mirrors the fake summary's xref_size of 3: the free-list head plus
        // two in-use objects at deterministic offsets.
        let total = 3usize;
        let start = range.offset.min(total);
        let len = (total - start).min(range.limit);
        let items = (start..start + len)
            .map(|num| {
                if num == 0 {
                    XrefEntryInfo {
                        object: ObjectId { num: 0, gen: 65535 },
                        kind: XrefEntryKind::Free,
                        offset: 0,
                        objstm_index: None,
                        section: Some(0),
                    }
                } else {
                    XrefEntryInfo {
                        object: ObjectId {
                            num: num as i32,
                            gen: 0,
                        },
                        kind: XrefEntryKind::Normal,
                        offset: 100 * num as u64,
                        objstm_index: None,
                        section: Some(0),
                    }
                }
            })
            .collect();
        XrefTableSlice {
            items,
            offset: start,
            total,
            sections: 1,
        }
    }

    fn image_preview(
        &self,
        _object: ObjectId,
        _max_dimension: u32,
    ) -> Result<ImagePreview, ShimError> {
        Err(ShimError::new(
            raw::pdbg_status::PDBG_ERROR_UNSUPPORTED,
            "fake shim has no image preview",
        ))
    }

    fn stream_load(
        &self,
        _object: ObjectId,
        mode: StreamMode,
        offset: u64,
        limit: usize,
        cancel: Option<&CancelToken>,
    ) -> Result<StreamChunk, ShimError> {
        check_fake_cancel(cancel)?;
        let bytes: &[u8] = match mode {
            StreamMode::Raw => b"fake stream bytes",
            StreamMode::Decoded => b"fake decoded stream expands beyond the configured cap",
        };
        if mode == StreamMode::Decoded && (bytes.len() as u64) > self.max_decoded_stream_bytes {
            return Err(ShimError::new(
                raw::pdbg_status::PDBG_ERROR_LIMIT,
                "decoded stream limit exceeded during decode",
            ));
        }
        let data_len = bytes.len().min(limit);
        Ok(StreamChunk {
            mode,
            offset,
            bytes: bytes[..data_len].to_vec(),
            total_size: Some(bytes.len() as u64),
            truncated: data_len < bytes.len(),
            decode_diagnostics: vec![fake_diagnostic(
                DiagnosticCode::StreamDecodeFailure,
                "fake stream diagnostic",
                None,
            )],
        })
    }

    fn stream_save(
        &self,
        _object: ObjectId,
        mode: StreamMode,
        path: &str,
        max_bytes: u64,
        cancel: Option<&CancelToken>,
    ) -> Result<StreamSaveOutcome, ShimError> {
        check_fake_cancel(cancel)?;
        let bytes: &[u8] = match mode {
            StreamMode::Raw => b"fake stream bytes",
            StreamMode::Decoded => b"fake decoded stream expands beyond the configured cap",
        };
        let capped = max_bytes > 0 && (bytes.len() as u64) > max_bytes;
        let write_len = if capped {
            max_bytes as usize
        } else {
            bytes.len()
        };
        // Mirror the real shim: temp file + rename so failures never clobber
        // an existing destination.
        let tmp_path = format!("{path}.pdbg-tmp");
        std::fs::write(&tmp_path, &bytes[..write_len])
            .map_err(|err| ShimError::new(raw::pdbg_status::PDBG_ERROR_GENERIC, err.to_string()))?;
        if let Err(err) = std::fs::rename(&tmp_path, path) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(ShimError::new(
                raw::pdbg_status::PDBG_ERROR_GENERIC,
                err.to_string(),
            ));
        }
        Ok(StreamSaveOutcome {
            bytes_written: write_len as u64,
            capped,
        })
    }

    fn render_page(
        &self,
        request: &RenderRequest,
        cancel: Option<&CancelToken>,
    ) -> Result<RenderResult, ShimError> {
        check_fake_cancel(cancel)?;
        Ok(RenderResult {
            page_index: request.page_index,
            width: 1,
            height: 1,
            stride: 4,
            pixels_rgba: vec![255, 255, 255, 255],
            duration_ms: 0,
            diagnostics: vec![fake_diagnostic(
                DiagnosticCode::RenderWarning,
                "fake render diagnostic",
                None,
            )],
        })
    }

    fn extract_text(
        &self,
        request: &TextRequest,
        cancel: Option<&CancelToken>,
    ) -> Result<TextPage, ShimError> {
        check_fake_cancel(cancel)?;
        if request.max_blocks != 0 && request.max_blocks < 2 {
            return Err(ShimError::new(
                raw::pdbg_status::PDBG_ERROR_LIMIT,
                "text extraction exceeded configured block limit",
            ));
        }
        if request.max_chars != 0 && request.max_chars < 4 {
            return Err(ShimError::new(
                raw::pdbg_status::PDBG_ERROR_LIMIT,
                "text extraction exceeded configured character limit",
            ));
        }
        Ok(TextPage {
            page_index: request.page_index,
            spans: vec![
                TextSpan {
                    text: "A\0B".to_string(),
                    bbox: PageRect {
                        x: 5.0,
                        y: 7.0,
                        width: 10.0,
                        height: 12.0,
                    },
                    untrusted: true,
                },
                TextSpan {
                    text: "C".to_string(),
                    bbox: PageRect {
                        x: 20.0,
                        y: 28.0,
                        width: 6.0,
                        height: 8.0,
                    },
                    untrusted: true,
                },
            ],
        })
    }

    fn extract_visuals(
        &self,
        request: &VisualRequest,
        cancel: Option<&CancelToken>,
    ) -> Result<VisualPage, ShimError> {
        check_fake_cancel(cancel)?;
        let mut elements = Vec::new();
        if request.include_text {
            elements.push(VisualElement {
                kind: VisualElementKind::Text,
                bbox: PageRect {
                    x: 5.0,
                    y: 7.0,
                    width: 21.0,
                    height: 29.0,
                },
                object: None,
                untrusted: true,
            });
        }
        if request.include_images {
            elements.push(VisualElement {
                kind: VisualElementKind::Image,
                bbox: PageRect {
                    x: 40.0,
                    y: 50.0,
                    width: 80.0,
                    height: 90.0,
                },
                object: None,
                untrusted: true,
            });
        }
        if request.include_vectors {
            elements.push(VisualElement {
                kind: VisualElementKind::Vector,
                bbox: PageRect {
                    x: 140.0,
                    y: 150.0,
                    width: 20.0,
                    height: 10.0,
                },
                object: None,
                untrusted: true,
            });
        }
        if request.max_elements != 0 && request.max_elements < elements.len() {
            return Err(ShimError::new(
                raw::pdbg_status::PDBG_ERROR_LIMIT,
                "visual extraction exceeded configured element limit",
            ));
        }
        Ok(VisualPage {
            page_index: request.page_index,
            elements,
        })
    }
}

fn check_fake_cancel(cancel: Option<&CancelToken>) -> Result<(), ShimError> {
    if cancel.is_some_and(CancelToken::is_cancelled) {
        return Err(ShimError::new(
            raw::pdbg_status::PDBG_ERROR_CANCELLED,
            "cancelled",
        ));
    }
    Ok(())
}

fn fake_child_page(parent: &NodeId, range: ChildRange, container: ChildContainer) -> ChildPage {
    let total: usize = 3;
    let len = range.limit.min(total.saturating_sub(range.offset));
    let items = (0..len)
        .map(|index| fake_child_summary(parent, range, container, index))
        .collect();
    ChildPage {
        total: Some(total),
        items,
    }
}

fn fake_child_summary(
    parent: &NodeId,
    range: ChildRange,
    container: ChildContainer,
    list_index: usize,
) -> ObjectSummary {
    let child_index = range.offset + list_index;
    let doc = parent.document_id();
    let id = match container {
        ChildContainer::Dictionary => NodeId::DictEntry {
            doc,
            parent: Box::new(parent.clone()),
            key: format!("Key{child_index}"),
        },
        ChildContainer::Array => NodeId::ArrayEntry {
            doc,
            parent: Box::new(parent.clone()),
            index: child_index,
        },
    };
    ObjectSummary {
        id: id.clone(),
        kind: ObjectKind::Dict,
        label: format!("Object {child_index}"),
        preview: "fake object".to_string(),
        object: Some(ObjectId {
            num: child_index as i32 + 1,
            gen: 0,
        }),
        has_children: true,
        has_stream: child_index == 0,
        child_count: Some(3),
        byte_size_hint: Some(128),
        diagnostics: vec![fake_diagnostic(
            DiagnosticCode::RepairWarning,
            "fake child diagnostic",
            Some(id),
        )],
    }
}

fn fake_stream_summary() -> StreamSummary {
    StreamSummary {
        object: ObjectId { num: 1, gen: 0 },
        filters: vec!["FlateDecode".to_string()],
        raw_size_hint: Some(32),
        decoded_size_hint: Some(64),
        can_decode: true,
        image_preview_available: false,
    }
}

fn fake_diagnostic(code: DiagnosticCode, message: &str, node: Option<NodeId>) -> DiagnosticSummary {
    DiagnosticSummary {
        severity: DiagnosticSeverity::Warning,
        code,
        message: message.to_string(),
        node,
        page_index: None,
        object: Some(ObjectId { num: 1, gen: 0 }),
    }
}

pub struct CancelToken {
    raw: NonNull<raw::pdbg_cancel_token>,
    cancelled: AtomicBool,
}

impl CancelToken {
    pub fn new() -> Result<Self, ShimError> {
        unsafe {
            let mut token = ptr::null_mut();
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_cancel_token_new(&mut token, &mut err);
            check_status(status, &err)?;
            let raw = NonNull::new(token).ok_or_else(|| {
                ShimError::new(
                    raw::pdbg_status::PDBG_ERROR_GENERIC,
                    "pdbg_cancel_token_new returned null",
                )
            })?;
            Ok(Self {
                raw,
                cancelled: AtomicBool::new(false),
            })
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        unsafe { raw::pdbg_cancel_token_cancel(self.raw.as_ptr()) }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    #[cfg(feature = "real-mupdf")]
    fn as_mut_ptr(&self) -> *mut raw::pdbg_cancel_token {
        self.raw.as_ptr()
    }
}

// Safety: the C cancel token stores its cancellation flag atomically and protects
// active MuPDF cookie registration with an internal mutex. It is intentionally
// shared so a controller can request cancellation while a worker is inside a
// bounded stream/render operation.
unsafe impl Send for CancelToken {}
unsafe impl Sync for CancelToken {}

impl Drop for CancelToken {
    fn drop(&mut self) {
        unsafe { raw::pdbg_cancel_token_drop(self.raw.as_ptr()) }
    }
}

#[cfg(feature = "real-mupdf")]
struct PdbgContext {
    raw: NonNull<raw::pdbg_context>,
    open_lock: Mutex<()>,
}

// Safety: the C `pdbg_context` installs MuPDF lock callbacks before any cloned
// document context is created. The Rust root context is shared to keep that
// lock table alive; root-context open/clone entry points are serialized by
// `open_lock`, while document operations use per-document C handles.
#[cfg(feature = "real-mupdf")]
unsafe impl Send for PdbgContext {}
#[cfg(feature = "real-mupdf")]
unsafe impl Sync for PdbgContext {}

#[cfg(feature = "real-mupdf")]
impl PdbgContext {
    fn new() -> Result<Self, ShimError> {
        unsafe {
            let mut ctx = ptr::null_mut();
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_context_new(&mut ctx, &mut err);
            check_status(status, &err)?;
            let raw = NonNull::new(ctx).ok_or_else(|| {
                ShimError::new(
                    raw::pdbg_status::PDBG_ERROR_GENERIC,
                    "pdbg_context_new returned null",
                )
            })?;
            Ok(Self {
                raw,
                open_lock: Mutex::new(()),
            })
        }
    }

    fn open_raw_document_handle(
        &self,
        path: &str,
        password: Option<&str>,
        config: &SafeModeConfig,
    ) -> Result<NonNull<raw::pdbg_doc>, ShimError> {
        let path = CString::new(path).map_err(|_| {
            ShimError::new(
                raw::pdbg_status::PDBG_ERROR_GENERIC,
                "path contains interior NUL",
            )
        })?;
        let password = password.map(CString::new).transpose().map_err(|_| {
            ShimError::new(
                raw::pdbg_status::PDBG_ERROR_GENERIC,
                "password contains interior NUL",
            )
        })?;
        let options = config.to_raw_open_options();
        let _open_guard = self.open_lock.lock().expect("pdbg context mutex poisoned");

        unsafe {
            let mut doc = ptr::null_mut();
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_document_open(
                self.raw.as_ptr(),
                path.as_ptr(),
                password
                    .as_ref()
                    .map_or(ptr::null(), |password| password.as_ptr()),
                &options,
                &mut doc,
                &mut err,
            );
            check_open_status(status, &err)?;
            let raw = NonNull::new(doc).ok_or_else(|| {
                ShimError::new(
                    raw::pdbg_status::PDBG_ERROR_GENERIC,
                    "pdbg_document_open returned null",
                )
            })?;
            Ok(raw)
        }
    }

    #[cfg(unix)]
    fn open_raw_document_fd_handle(
        &self,
        fd: BorrowedFd<'_>,
        display_path: &str,
        password: Option<&str>,
        config: &SafeModeConfig,
    ) -> Result<NonNull<raw::pdbg_doc>, ShimError> {
        let display_path = CString::new(display_path).map_err(|_| {
            ShimError::new(
                raw::pdbg_status::PDBG_ERROR_GENERIC,
                "display path contains interior NUL",
            )
        })?;
        let password = password.map(CString::new).transpose().map_err(|_| {
            ShimError::new(
                raw::pdbg_status::PDBG_ERROR_GENERIC,
                "password contains interior NUL",
            )
        })?;
        let options = config.to_raw_open_options();
        let _open_guard = self.open_lock.lock().expect("pdbg context mutex poisoned");

        unsafe {
            let mut doc = ptr::null_mut();
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_document_open_fd(
                self.raw.as_ptr(),
                fd.as_raw_fd(),
                display_path.as_ptr(),
                password
                    .as_ref()
                    .map_or(ptr::null(), |password| password.as_ptr()),
                &options,
                &mut doc,
                &mut err,
            );
            check_open_status(status, &err)?;
            let raw = NonNull::new(doc).ok_or_else(|| {
                ShimError::new(
                    raw::pdbg_status::PDBG_ERROR_GENERIC,
                    "pdbg_document_open_fd returned null",
                )
            })?;
            Ok(raw)
        }
    }
}

#[cfg(feature = "real-mupdf")]
impl Drop for PdbgContext {
    fn drop(&mut self) {
        unsafe { raw::pdbg_context_drop(self.raw.as_ptr()) }
    }
}

#[cfg(feature = "real-mupdf")]
struct PdbgDoc {
    raw: NonNull<raw::pdbg_doc>,
    _ctx: Arc<PdbgContext>,
}

#[cfg(feature = "real-mupdf")]
impl PdbgDoc {
    fn open_path(
        ctx: Arc<PdbgContext>,
        path: &str,
        password: Option<&str>,
        config: &SafeModeConfig,
    ) -> Result<Self, ShimError> {
        let raw = ctx.open_raw_document_handle(path, password, config)?;
        Ok(Self { raw, _ctx: ctx })
    }

    #[cfg(unix)]
    fn open_fd(
        ctx: Arc<PdbgContext>,
        fd: BorrowedFd<'_>,
        display_path: &str,
        password: Option<&str>,
        config: &SafeModeConfig,
    ) -> Result<Self, ShimError> {
        let raw = ctx.open_raw_document_fd_handle(fd, display_path, password, config)?;
        Ok(Self { raw, _ctx: ctx })
    }
}

// Safety: `PdbgDoc` may be moved to a worker thread, but it is not `Sync`.
// The C shim contract requires document handles to remain valid after open
// without borrowing unsynchronized root-context state. Concurrent access must
// go through `DocumentSession`, which serializes mutable document operations.
#[cfg(feature = "real-mupdf")]
unsafe impl Send for PdbgDoc {}

#[cfg(feature = "real-mupdf")]
impl PdbgDoc {
    fn summary(&self, registry: &NodeTokenRegistry) -> Result<DocumentSummary, ShimError> {
        unsafe {
            let mut out = std::mem::zeroed::<raw::pdbg_document_summary_out>();
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_document_summary(self.raw.as_ptr(), &mut out, &mut err);
            check_status(status, &err)?;
            let summary = convert_document_summary(&out, registry);
            raw::pdbg_document_summary_out_drop(&mut out);
            Ok(summary)
        }
    }

    fn children(
        &self,
        registry: &mut NodeTokenRegistry,
        raw_parent: &raw::pdbg_node_id,
        parent: &NodeId,
        range: ChildRange,
        container: ChildContainer,
    ) -> Result<ChildPage, ShimError> {
        unsafe {
            let mut list = ptr::null_mut();
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_node_children(
                self.raw.as_ptr(),
                raw_parent,
                range.offset,
                range.limit,
                &mut list,
                &mut err,
            );
            check_status(status, &err)?;
            let list = PdbgNodeList::new(list)?;
            Ok(list.to_child_page(registry, parent, range, container))
        }
    }

    fn object_detail(
        &self,
        registry: &mut NodeTokenRegistry,
        raw_node: &raw::pdbg_node_id,
        public_node: &NodeId,
        range: ChildRange,
    ) -> Result<ObjectDetail, ShimError> {
        unsafe {
            let mut out = std::mem::zeroed::<raw::pdbg_object_detail_out>();
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_object_detail(self.raw.as_ptr(), raw_node, &mut out, &mut err);
            check_status(status, &err)?;
            let detail = convert_object_detail(registry, public_node, range, &out);
            raw::pdbg_object_detail_out_drop(&mut out);
            Ok(detail)
        }
    }

    fn xref_table(&self, range: ChildRange) -> Result<XrefTableSlice, ShimError> {
        unsafe {
            let mut table = ptr::null_mut();
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_xref_table_load(
                self.raw.as_ptr(),
                range.offset,
                range.limit,
                &mut table,
                &mut err,
            );
            check_status(status, &err)?;
            let len = raw::pdbg_xref_table_len(table);
            let total = raw::pdbg_xref_table_total(table);
            let start = raw::pdbg_xref_table_start(table);
            let sections = raw::pdbg_xref_table_sections(table);
            let raw_items = raw::pdbg_xref_table_items(table);
            let mut items = Vec::with_capacity(len);
            for index in 0..len {
                let info = *raw_items.add(index);
                let kind = match info.kind {
                    raw::PDBG_XREF_ENTRY_NORMAL => XrefEntryKind::Normal,
                    raw::PDBG_XREF_ENTRY_COMPRESSED => XrefEntryKind::Compressed,
                    _ => XrefEntryKind::Free,
                };
                // The shim reports MuPDF's section index (newest = 0); flip it
                // so 0 is the original document and higher means later update.
                let section = (info.section >= 0 && (info.section as usize) < sections)
                    .then(|| (sections - 1 - info.section as usize) as u32);
                items.push(XrefEntryInfo {
                    object: ObjectId {
                        num: info.num,
                        gen: info.gen,
                    },
                    kind,
                    offset: info.offset,
                    objstm_index: (kind == XrefEntryKind::Compressed && info.objstm_index >= 0)
                        .then_some(info.objstm_index as u32),
                    section,
                });
            }
            raw::pdbg_xref_table_drop(table);
            Ok(XrefTableSlice {
                items,
                offset: start,
                total,
                sections,
            })
        }
    }

    fn image_preview(
        &self,
        object: ObjectId,
        max_dimension: u32,
        cancel: *mut raw::pdbg_cancel_token,
    ) -> Result<ImagePreview, ShimError> {
        unsafe {
            let mut image = ptr::null_mut();
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_image_object_load(
                self.raw.as_ptr(),
                wire::raw_object_id(object),
                max_dimension,
                0,
                cancel,
                &mut image,
                &mut err,
            );
            check_status(status, &err)?;
            let image = PdbgImage::new(image)?;
            image.to_image_preview()
        }
    }

    fn stream_save(
        &self,
        object: ObjectId,
        mode: StreamMode,
        path: &str,
        max_bytes: u64,
        cancel: *mut raw::pdbg_cancel_token,
    ) -> Result<StreamSaveOutcome, ShimError> {
        let path = CString::new(path).map_err(|_| {
            ShimError::new(
                raw::pdbg_status::PDBG_ERROR_GENERIC,
                "export path contains a NUL byte",
            )
        })?;
        unsafe {
            let mut bytes_written = 0u64;
            let mut capped = 0i32;
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_stream_save(
                self.raw.as_ptr(),
                wire::raw_object_id(object),
                matches!(mode, StreamMode::Decoded) as i32,
                path.as_ptr(),
                max_bytes,
                cancel,
                &mut bytes_written,
                &mut capped,
                &mut err,
            );
            check_status(status, &err)?;
            Ok(StreamSaveOutcome {
                bytes_written,
                capped: capped != 0,
            })
        }
    }

    fn stream_load(
        &self,
        registry: &NodeTokenRegistry,
        object: ObjectId,
        mode: StreamMode,
        offset: u64,
        limit: usize,
    ) -> Result<StreamChunk, ShimError> {
        self.stream_load_with_cancel(registry, object, mode, offset, limit, ptr::null_mut())
    }

    fn stream_load_with_cancel(
        &self,
        registry: &NodeTokenRegistry,
        object: ObjectId,
        mode: StreamMode,
        offset: u64,
        limit: usize,
        cancel: *mut raw::pdbg_cancel_token,
    ) -> Result<StreamChunk, ShimError> {
        unsafe {
            let mut buffer = ptr::null_mut();
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_stream_load(
                self.raw.as_ptr(),
                wire::raw_object_id(object),
                matches!(mode, StreamMode::Decoded) as i32,
                offset,
                limit,
                cancel,
                &mut buffer,
                &mut err,
            );
            check_status(status, &err)?;
            let buffer = PdbgBuffer::new(buffer)?;
            Ok(buffer.to_stream_chunk(registry, mode, offset))
        }
    }

    fn render_page(
        &self,
        registry: &NodeTokenRegistry,
        request: &RenderRequest,
    ) -> Result<RenderResult, ShimError> {
        self.render_page_with_cancel(registry, request, ptr::null_mut())
    }

    fn render_page_with_cancel(
        &self,
        registry: &NodeTokenRegistry,
        request: &RenderRequest,
        cancel: *mut raw::pdbg_cancel_token,
    ) -> Result<RenderResult, ShimError> {
        unsafe {
            let options = raw_render_options(request);
            let mut image = ptr::null_mut();
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_page_render(
                self.raw.as_ptr(),
                page_index_to_u32(request.page_index)?,
                &options,
                cancel,
                &mut image,
                &mut err,
            );
            check_status(status, &err)?;
            let image = PdbgImage::new(image)?;
            image.to_render_result(registry, request.page_index)
        }
    }

    fn extract_text(&self, request: &TextRequest) -> Result<TextPage, ShimError> {
        self.extract_text_with_cancel(request, ptr::null_mut())
    }

    fn extract_text_with_cancel(
        &self,
        request: &TextRequest,
        cancel: *mut raw::pdbg_cancel_token,
    ) -> Result<TextPage, ShimError> {
        unsafe {
            let options = raw::pdbg_text_options {
                sort_by_position: request.sort_by_position as i32,
                include_coordinates: request.include_coordinates as i32,
                max_chars: request.max_chars,
                max_blocks: request.max_blocks,
            };
            let mut text = ptr::null_mut();
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_page_extract_text(
                self.raw.as_ptr(),
                page_index_to_u32(request.page_index)?,
                &options,
                cancel,
                &mut text,
                &mut err,
            );
            check_status(status, &err)?;
            let text = PdbgTextPage::new(text)?;
            Ok(text.to_text_page(request.page_index))
        }
    }

    fn extract_visuals(&self, request: &VisualRequest) -> Result<VisualPage, ShimError> {
        self.extract_visuals_with_cancel(request, ptr::null_mut())
    }

    fn extract_visuals_with_cancel(
        &self,
        request: &VisualRequest,
        cancel: *mut raw::pdbg_cancel_token,
    ) -> Result<VisualPage, ShimError> {
        unsafe {
            let options = raw::pdbg_visual_options {
                include_text: request.include_text as i32,
                include_images: request.include_images as i32,
                include_vectors: request.include_vectors as i32,
                max_elements: request.max_elements,
            };
            let mut visuals = ptr::null_mut();
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_page_extract_visuals(
                self.raw.as_ptr(),
                page_index_to_u32(request.page_index)?,
                &options,
                cancel,
                &mut visuals,
                &mut err,
            );
            check_status(status, &err)?;
            let visuals = PdbgVisualPage::new(visuals)?;
            Ok(visuals.to_visual_page(request.page_index))
        }
    }
}

#[cfg(feature = "real-mupdf")]
impl Drop for PdbgDoc {
    fn drop(&mut self) {
        unsafe { raw::pdbg_document_drop(self.raw.as_ptr()) }
    }
}

#[cfg(feature = "real-mupdf")]
struct PdbgNodeList {
    raw: NonNull<raw::pdbg_node_list>,
}

#[cfg(feature = "real-mupdf")]
impl PdbgNodeList {
    fn new(raw: *mut raw::pdbg_node_list) -> Result<Self, ShimError> {
        let raw = NonNull::new(raw).ok_or_else(|| {
            ShimError::new(
                raw::pdbg_status::PDBG_ERROR_GENERIC,
                "node list accessor returned null",
            )
        })?;
        Ok(Self { raw })
    }

    unsafe fn to_child_page(
        &self,
        registry: &mut NodeTokenRegistry,
        parent: &NodeId,
        range: ChildRange,
        container: ChildContainer,
    ) -> ChildPage {
        borrowed_node_list_to_child_page(self.raw.as_ptr(), registry, parent, range, container)
    }
}

#[cfg(feature = "real-mupdf")]
impl Drop for PdbgNodeList {
    fn drop(&mut self) {
        unsafe { raw::pdbg_node_list_drop(self.raw.as_ptr()) }
    }
}

#[cfg(feature = "real-mupdf")]
struct PdbgBuffer {
    raw: NonNull<raw::pdbg_buffer>,
}

#[cfg(feature = "real-mupdf")]
impl PdbgBuffer {
    fn new(raw: *mut raw::pdbg_buffer) -> Result<Self, ShimError> {
        let raw = NonNull::new(raw).ok_or_else(|| {
            ShimError::new(
                raw::pdbg_status::PDBG_ERROR_GENERIC,
                "buffer accessor returned null",
            )
        })?;
        Ok(Self { raw })
    }

    unsafe fn to_stream_chunk(
        &self,
        registry: &NodeTokenRegistry,
        mode: StreamMode,
        offset: u64,
    ) -> StreamChunk {
        let len = raw::pdbg_buffer_len(self.raw.as_ptr());
        let bytes = wire::copy_bytes(raw::pdbg_buffer_data(self.raw.as_ptr()), len);
        let diagnostic_count = raw::pdbg_buffer_diagnostic_count(self.raw.as_ptr());
        let mut decode_diagnostics = Vec::with_capacity(diagnostic_count);
        for index in 0..diagnostic_count {
            let mut diagnostic = std::mem::zeroed::<raw::pdbg_diagnostic>();
            let mut err = raw::pdbg_error::default();
            if raw::pdbg_buffer_diagnostic_get(self.raw.as_ptr(), index, &mut diagnostic, &mut err)
                == raw::pdbg_status::PDBG_OK
            {
                decode_diagnostics.push(wire::diagnostic(&diagnostic, &|node| {
                    registry.resolve_node(node)
                }));
            }
        }

        StreamChunk {
            mode,
            offset,
            bytes,
            total_size: Some(raw::pdbg_buffer_total_size_hint(self.raw.as_ptr())),
            truncated: raw::pdbg_buffer_truncated(self.raw.as_ptr()) != 0,
            decode_diagnostics,
        }
    }
}

#[cfg(feature = "real-mupdf")]
impl Drop for PdbgBuffer {
    fn drop(&mut self) {
        unsafe { raw::pdbg_buffer_drop(self.raw.as_ptr()) }
    }
}

#[cfg(feature = "real-mupdf")]
struct PdbgImage {
    raw: NonNull<raw::pdbg_image>,
}

#[cfg(feature = "real-mupdf")]
impl PdbgImage {
    fn new(raw: *mut raw::pdbg_image) -> Result<Self, ShimError> {
        let raw = NonNull::new(raw).ok_or_else(|| {
            ShimError::new(
                raw::pdbg_status::PDBG_ERROR_GENERIC,
                "image accessor returned null",
            )
        })?;
        Ok(Self { raw })
    }

    unsafe fn to_render_result(
        &self,
        registry: &NodeTokenRegistry,
        page_index: usize,
    ) -> Result<RenderResult, ShimError> {
        let width = raw::pdbg_image_width(self.raw.as_ptr());
        let height = raw::pdbg_image_height(self.raw.as_ptr());
        let stride = raw::pdbg_image_stride(self.raw.as_ptr());
        let byte_len = stride.checked_mul(height as usize).ok_or_else(|| {
            ShimError::new(
                raw::pdbg_status::PDBG_ERROR_LIMIT,
                "render output byte size overflow",
            )
        })?;
        let pixels_rgba =
            wire::copy_bytes(raw::pdbg_image_rgba_pixels(self.raw.as_ptr()), byte_len);
        let diagnostic_count = raw::pdbg_image_diagnostic_count(self.raw.as_ptr());
        let mut diagnostics = Vec::with_capacity(diagnostic_count);
        for index in 0..diagnostic_count {
            let mut diagnostic = std::mem::zeroed::<raw::pdbg_diagnostic>();
            let mut err = raw::pdbg_error::default();
            if raw::pdbg_image_diagnostic_get(self.raw.as_ptr(), index, &mut diagnostic, &mut err)
                == raw::pdbg_status::PDBG_OK
            {
                diagnostics.push(wire::diagnostic(&diagnostic, &|node| {
                    registry.resolve_node(node)
                }));
            }
        }

        Ok(RenderResult {
            page_index,
            width,
            height,
            stride,
            pixels_rgba,
            duration_ms: 0,
            diagnostics,
        })
    }

    unsafe fn to_image_preview(&self) -> Result<ImagePreview, ShimError> {
        let width = raw::pdbg_image_width(self.raw.as_ptr());
        let height = raw::pdbg_image_height(self.raw.as_ptr());
        let stride = raw::pdbg_image_stride(self.raw.as_ptr());
        let byte_len = stride.checked_mul(height as usize).ok_or_else(|| {
            ShimError::new(
                raw::pdbg_status::PDBG_ERROR_LIMIT,
                "image output byte size overflow",
            )
        })?;
        let pixels_rgba =
            wire::copy_bytes(raw::pdbg_image_rgba_pixels(self.raw.as_ptr()), byte_len);
        let diagnostic_count = raw::pdbg_image_diagnostic_count(self.raw.as_ptr());
        let mut diagnostics = Vec::with_capacity(diagnostic_count);
        for index in 0..diagnostic_count {
            let mut diagnostic = std::mem::zeroed::<raw::pdbg_diagnostic>();
            let mut err = raw::pdbg_error::default();
            if raw::pdbg_image_diagnostic_get(self.raw.as_ptr(), index, &mut diagnostic, &mut err)
                == raw::pdbg_status::PDBG_OK
            {
                diagnostics.push(wire::diagnostic(&diagnostic, &|_| None));
            }
        }
        Ok(ImagePreview {
            width,
            height,
            stride,
            pixels_rgba,
            diagnostics,
        })
    }
}

#[cfg(feature = "real-mupdf")]
impl Drop for PdbgImage {
    fn drop(&mut self) {
        unsafe { raw::pdbg_image_drop(self.raw.as_ptr()) }
    }
}

#[cfg(feature = "real-mupdf")]
struct PdbgTextPage {
    raw: NonNull<raw::pdbg_text_page>,
}

#[cfg(feature = "real-mupdf")]
impl PdbgTextPage {
    fn new(raw: *mut raw::pdbg_text_page) -> Result<Self, ShimError> {
        let raw = NonNull::new(raw).ok_or_else(|| {
            ShimError::new(
                raw::pdbg_status::PDBG_ERROR_GENERIC,
                "text page accessor returned null",
            )
        })?;
        Ok(Self { raw })
    }

    unsafe fn to_text_page(&self, page_index: usize) -> TextPage {
        let len = raw::pdbg_text_page_span_count(self.raw.as_ptr());
        let mut spans = Vec::with_capacity(len);
        for index in 0..len {
            let mut span = std::mem::zeroed::<raw::pdbg_text_span>();
            let mut err = raw::pdbg_error::default();
            if raw::pdbg_text_page_span_get(self.raw.as_ptr(), index, &mut span, &mut err)
                == raw::pdbg_status::PDBG_OK
            {
                spans.push(wire::text_span(&span));
            }
        }
        TextPage { page_index, spans }
    }
}

#[cfg(feature = "real-mupdf")]
impl Drop for PdbgTextPage {
    fn drop(&mut self) {
        unsafe { raw::pdbg_text_page_drop(self.raw.as_ptr()) }
    }
}

#[cfg(feature = "real-mupdf")]
struct PdbgVisualPage {
    raw: NonNull<raw::pdbg_visual_page>,
}

#[cfg(feature = "real-mupdf")]
impl PdbgVisualPage {
    fn new(raw: *mut raw::pdbg_visual_page) -> Result<Self, ShimError> {
        let raw = NonNull::new(raw).ok_or_else(|| {
            ShimError::new(
                raw::pdbg_status::PDBG_ERROR_GENERIC,
                "visual page accessor returned null",
            )
        })?;
        Ok(Self { raw })
    }

    unsafe fn to_visual_page(&self, page_index: usize) -> VisualPage {
        let len = raw::pdbg_visual_page_element_count(self.raw.as_ptr());
        let mut elements = Vec::with_capacity(len);
        for index in 0..len {
            let mut element = std::mem::zeroed::<raw::pdbg_visual_element>();
            let mut err = raw::pdbg_error::default();
            if raw::pdbg_visual_page_element_get(self.raw.as_ptr(), index, &mut element, &mut err)
                == raw::pdbg_status::PDBG_OK
            {
                elements.push(wire::visual_element(&element));
            }
        }
        VisualPage {
            page_index,
            elements,
        }
    }
}

#[cfg(feature = "real-mupdf")]
impl Drop for PdbgVisualPage {
    fn drop(&mut self) {
        unsafe { raw::pdbg_visual_page_drop(self.raw.as_ptr()) }
    }
}

fn check_status(status: raw::pdbg_status, err: &raw::pdbg_error) -> Result<(), ShimError> {
    if status == raw::pdbg_status::PDBG_OK {
        return Ok(());
    }
    Err(ShimError::new(status, c_char_array_to_string(&err.message)))
}

#[cfg(feature = "real-mupdf")]
fn check_open_status(status: raw::pdbg_status, err: &raw::pdbg_error) -> Result<(), ShimError> {
    if status == raw::pdbg_status::PDBG_OK {
        return Ok(());
    }

    let message = c_char_array_to_string(&err.message);
    let diagnostics = if status == raw::pdbg_status::PDBG_ERROR_PASSWORD {
        vec![DiagnosticSummary {
            severity: DiagnosticSeverity::Error,
            code: DiagnosticCode::EncryptionPasswordFailure,
            message: message.clone(),
            node: None,
            page_index: None,
            object: None,
        }]
    } else {
        Vec::new()
    };

    Err(ShimError::with_diagnostics(status, message, diagnostics))
}

fn c_char_array_to_string(bytes: &[std::os::raw::c_char]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    let bounded = bytes[..end]
        .iter()
        .map(|byte| *byte as u8)
        .collect::<Vec<_>>();
    String::from_utf8_lossy(&bounded).into_owned()
}

#[cfg(feature = "real-mupdf")]
unsafe fn convert_document_summary(
    out: &raw::pdbg_document_summary_out,
    registry: &NodeTokenRegistry,
) -> DocumentSummary {
    let mut diagnostics =
        wire::diagnostic_list(out.diagnostics, &|node| registry.resolve_node(node));
    if out.javascript_disabled != 0
        && !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::JavaScriptDisabled)
    {
        diagnostics.push(DiagnosticSummary {
            severity: DiagnosticSeverity::Info,
            code: DiagnosticCode::JavaScriptDisabled,
            message: "JavaScript execution is disabled".to_string(),
            node: None,
            page_index: None,
            object: None,
        });
    }

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
        diagnostics,
    }
}

#[cfg(feature = "real-mupdf")]
unsafe fn convert_object_detail(
    registry: &mut NodeTokenRegistry,
    public_node: &NodeId,
    range: ChildRange,
    out: &raw::pdbg_object_detail_out,
) -> ObjectDetail {
    let array_entries = (!out.children.is_null()).then(|| {
        borrowed_node_list_to_child_page(
            out.children,
            registry,
            public_node,
            range,
            ChildContainer::Array,
        )
    });
    let dictionary_entries = (!out.dictionary_entries.is_null()).then(|| {
        borrowed_node_list_to_dict_entry_page(out.dictionary_entries, registry, public_node, range)
    });

    ObjectDetail {
        id: public_node.clone(),
        kind: wire::object_kind(out.kind),
        object: wire::optional_object_id(out.object, out.has_object),
        label: wire::copy_c_string(out.label),
        preview: wire::copy_c_string(out.preview),
        value: wire::object_value(&out.value),
        dictionary_entries,
        array_entries,
        stream: (out.has_stream != 0).then(|| wire::stream_summary(&out.stream)),
        diagnostics: wire::diagnostic_list(out.diagnostics, &|node| registry.resolve_node(node)),
    }
}

#[cfg(feature = "real-mupdf")]
unsafe fn borrowed_node_list_to_child_page(
    list: *const raw::pdbg_node_list,
    registry: &mut NodeTokenRegistry,
    parent: &NodeId,
    range: ChildRange,
    container: ChildContainer,
) -> ChildPage {
    let len = raw::pdbg_node_list_len(list);
    let mut items = Vec::with_capacity(len);
    for index in 0..len {
        let mut entry = std::mem::zeroed::<raw::pdbg_dict_entry>();
        let mut err = raw::pdbg_error::default();
        if raw::pdbg_node_list_get(list, index, &mut entry, &mut err) == raw::pdbg_status::PDBG_OK {
            items.push(registry.convert_child_entry(parent, &entry, container, range, index));
        }
    }

    ChildPage {
        total: (raw::pdbg_node_list_has_total_count(list) != 0)
            .then_some(raw::pdbg_node_list_total_count(list)),
        items,
    }
}

#[cfg(feature = "real-mupdf")]
unsafe fn borrowed_node_list_to_dict_entry_page(
    list: *const raw::pdbg_node_list,
    registry: &mut NodeTokenRegistry,
    parent: &NodeId,
    range: ChildRange,
) -> ChildPage<DictEntryDetail> {
    let page =
        borrowed_node_list_to_child_page(list, registry, parent, range, ChildContainer::Dictionary);
    ChildPage {
        total: page.total,
        items: page
            .items
            .into_iter()
            .map(|value| {
                let key = match &value.id {
                    NodeId::DictEntry { key, .. } => key.clone(),
                    _ => String::new(),
                };
                DictEntryDetail { key, value }
            })
            .collect(),
    }
}

#[cfg(feature = "real-mupdf")]
fn raw_node_from_public(node: &NodeId) -> Option<raw::pdbg_node_id> {
    let mut raw_node = raw::pdbg_node_id {
        document_id: node.document_id().0,
        kind: raw::pdbg_node_kind::PDBG_NODE_DOCUMENT_ROOT,
        object: raw::pdbg_object_id { num: 0, gen: 0 },
        has_object: 0,
        page_index: 0,
        path_token: 0,
        decoded: 0,
        resource_group: raw::pdbg_resource_group::PDBG_RESOURCE_FONTS,
    };

    match node {
        NodeId::DocumentRoot { .. } => {}
        NodeId::Trailer { .. } => raw_node.kind = raw::pdbg_node_kind::PDBG_NODE_TRAILER,
        NodeId::Catalog { .. } => raw_node.kind = raw::pdbg_node_kind::PDBG_NODE_CATALOG,
        NodeId::XrefRoot { .. } => raw_node.kind = raw::pdbg_node_kind::PDBG_NODE_XREF_ROOT,
        NodeId::XrefObject { object, .. } => {
            raw_node.kind = raw::pdbg_node_kind::PDBG_NODE_XREF_OBJECT;
            raw_node.object = wire::raw_object_id(*object);
            raw_node.has_object = 1;
        }
        NodeId::PageRoot { .. } => raw_node.kind = raw::pdbg_node_kind::PDBG_NODE_PAGE_ROOT,
        NodeId::Page { index, .. } => {
            raw_node.kind = raw::pdbg_node_kind::PDBG_NODE_PAGE;
            raw_node.page_index = (*index).try_into().ok()?;
        }
        NodeId::IndirectRef { object, .. } => {
            raw_node.kind = raw::pdbg_node_kind::PDBG_NODE_INDIRECT_REF;
            raw_node.object = wire::raw_object_id(*object);
            raw_node.has_object = 1;
        }
        NodeId::Stream {
            object, decoded, ..
        } => {
            raw_node.kind = raw::pdbg_node_kind::PDBG_NODE_STREAM;
            raw_node.object = wire::raw_object_id(*object);
            raw_node.has_object = 1;
            raw_node.decoded = *decoded as i32;
        }
        NodeId::ResourceGroup {
            page_index, group, ..
        } => {
            raw_node.kind = raw::pdbg_node_kind::PDBG_NODE_RESOURCE_GROUP;
            raw_node.page_index = (*page_index).try_into().ok()?;
            raw_node.resource_group = raw_resource_group(group);
        }
        NodeId::DictEntry { .. } | NodeId::ArrayEntry { .. } => return None,
    }

    Some(raw_node)
}

#[cfg(feature = "real-mupdf")]
fn raw_resource_group(group: &ResourceGroup) -> raw::pdbg_resource_group {
    match group {
        ResourceGroup::Fonts => raw::pdbg_resource_group::PDBG_RESOURCE_FONTS,
        ResourceGroup::Images => raw::pdbg_resource_group::PDBG_RESOURCE_IMAGES,
        ResourceGroup::XObjects => raw::pdbg_resource_group::PDBG_RESOURCE_XOBJECTS,
        ResourceGroup::ColorSpaces => raw::pdbg_resource_group::PDBG_RESOURCE_COLOR_SPACES,
        ResourceGroup::Patterns => raw::pdbg_resource_group::PDBG_RESOURCE_PATTERNS,
        ResourceGroup::Shadings => raw::pdbg_resource_group::PDBG_RESOURCE_SHADINGS,
        ResourceGroup::Annotations => raw::pdbg_resource_group::PDBG_RESOURCE_ANNOTATIONS,
        ResourceGroup::Widgets => raw::pdbg_resource_group::PDBG_RESOURCE_WIDGETS,
    }
}

#[cfg(feature = "real-mupdf")]
fn raw_render_options(request: &RenderRequest) -> raw::pdbg_render_options {
    raw::pdbg_render_options {
        zoom: request.zoom,
        rotation_degrees: request.rotation_degrees,
        max_width: request.max_width,
        max_height: request.max_height,
        max_pixels: request.max_pixels,
        max_output_bytes: request.max_output_bytes,
        color_mode: match request.color_mode {
            RenderColorMode::Rgba => raw::pdbg_color_mode::PDBG_COLOR_RGBA,
            RenderColorMode::Grayscale => raw::pdbg_color_mode::PDBG_COLOR_GRAYSCALE,
            RenderColorMode::Inverted => raw::pdbg_color_mode::PDBG_COLOR_INVERTED,
        },
        layer_config_token: request.layer_config_token.unwrap_or(0),
    }
}

#[cfg(feature = "real-mupdf")]
fn page_index_to_u32(page_index: usize) -> Result<u32, ShimError> {
    page_index.try_into().map_err(|_| {
        ShimError::new(
            raw::pdbg_status::PDBG_ERROR_LIMIT,
            "page index exceeds C ABI range",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_char_array_to_string_handles_unterminated_fixed_buffer() {
        let bytes = [b'A' as std::os::raw::c_char; 4];

        assert_eq!(c_char_array_to_string(&bytes), "AAAA");
    }

    #[cfg(feature = "fake")]
    #[test]
    fn fake_shim_returns_document_summary() {
        let shim = FakeShim::new().unwrap();
        let summary = shim.open_document_summary("fake.pdf").unwrap();
        assert_eq!(summary.file_path, "fake.pdf");
        assert_eq!(summary.file_hash.as_deref(), Some("fake-hash"));
        assert_eq!(summary.pdf_version.as_deref(), Some("1.7"));
        assert_eq!(summary.page_count, 1);
        assert_eq!(summary.xref_size, 3);
        assert_eq!(summary.parsed_object_count, Some(3));
        assert!(!summary.encrypted);
        assert!(!summary.needs_password);
        assert!(summary.permissions.print);
        assert!(summary.permissions.copy);
        assert!(!summary.permissions.modify);
        assert!(summary.metadata_summary.is_empty());
        assert!(summary.safety.safe_mode);
        assert!(summary.safety.javascript_disabled);
        assert!(!summary.safety.ocr_enabled);
        assert_eq!(
            summary.diagnostics[0].code.as_public_str(),
            "repair_warning"
        );
        assert!(summary.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == DiagnosticCode::JavaScriptDisabled
                && diagnostic.severity == DiagnosticSeverity::Info
        }));
    }

    #[cfg(feature = "fake")]
    #[test]
    fn fake_document_exposes_children_detail_stream_render_and_text() {
        let shim = FakeShim::new().unwrap();
        let mut doc = shim.open_document("fake.pdf").unwrap();
        let summary = doc.summary().unwrap();
        let root = NodeId::DocumentRoot {
            doc: summary.doc.clone(),
        };
        let range = ChildRange {
            offset: 0,
            limit: 2,
        };

        let children = doc
            .children(&root, range, ChildContainer::Dictionary)
            .unwrap();
        assert_eq!(children.total, Some(3));
        assert_eq!(children.items.len(), 2);
        assert!(matches!(children.items[0].id, NodeId::DictEntry { .. }));

        let detail = doc.object_detail(&children.items[0].id, range).unwrap();
        let stream_summary = detail.stream.as_ref().unwrap();
        assert_eq!(stream_summary.object, ObjectId { num: 1, gen: 0 });
        assert_eq!(stream_summary.filters, vec!["FlateDecode"]);
        assert_eq!(stream_summary.raw_size_hint, Some(32));
        assert_eq!(stream_summary.decoded_size_hint, Some(64));
        assert!(stream_summary.can_decode);
        assert!(!stream_summary.image_preview_available);
        assert!(!detail.diagnostics.is_empty());
        assert!(detail.dictionary_entries.unwrap().total.is_some());

        let stream = doc
            .stream_load(ObjectId { num: 1, gen: 0 }, StreamMode::Raw, 0, 4)
            .unwrap();
        assert_eq!(stream.bytes, b"fake");
        assert!(stream.truncated);

        let render = doc.render_page(&RenderRequest::page(0)).unwrap();
        assert_eq!((render.width, render.height, render.stride), (1, 1, 4));
        assert_eq!(render.pixels_rgba, vec![255, 255, 255, 255]);

        let text = doc.extract_text(&TextRequest::page(0)).unwrap();
        assert_eq!(text.spans[0].text.as_bytes(), b"A\0B");
        assert!(text.spans[0].untrusted);

        let visuals = doc.extract_visuals(&VisualRequest::page(0)).unwrap();
        assert_eq!(visuals.page_index, 0);
        assert!(visuals
            .elements
            .iter()
            .any(|element| element.kind == VisualElementKind::Text));
        assert!(visuals
            .elements
            .iter()
            .any(|element| element.kind == VisualElementKind::Image));
    }

    #[cfg(all(feature = "fake", not(feature = "real-mupdf")))]
    #[test]
    fn fake_cancel_token_maps_to_cancelled_status_and_keeps_document_usable() {
        let shim = FakeShim::new().unwrap();
        let mut doc = shim.open_document("fake.pdf").unwrap();
        let cancel = CancelToken::new().unwrap();
        cancel.cancel();

        let stream_err = doc
            .stream_load_with_cancel_token(
                ObjectId { num: 1, gen: 0 },
                StreamMode::Raw,
                0,
                4,
                &cancel,
            )
            .unwrap_err();
        assert_eq!(stream_err.status, raw::pdbg_status::PDBG_ERROR_CANCELLED);

        let render_err = doc
            .render_page_with_cancel_token(&RenderRequest::page(0), &cancel)
            .unwrap_err();
        assert_eq!(render_err.status, raw::pdbg_status::PDBG_ERROR_CANCELLED);

        let text_err = doc
            .extract_text_with_cancel_token(&TextRequest::page(0), &cancel)
            .unwrap_err();
        assert_eq!(text_err.status, raw::pdbg_status::PDBG_ERROR_CANCELLED);

        let visual_err = doc
            .extract_visuals_with_cancel_token(&VisualRequest::page(0), &cancel)
            .unwrap_err();
        assert_eq!(visual_err.status, raw::pdbg_status::PDBG_ERROR_CANCELLED);

        let summary = doc.summary().unwrap();
        assert_eq!(summary.file_path, "fake.pdf");
        let render = doc.render_page(&RenderRequest::page(0)).unwrap();
        assert_eq!(render.pixels_rgba, vec![255, 255, 255, 255]);
    }

    #[cfg(all(feature = "fake", not(feature = "real-mupdf")))]
    #[test]
    fn fake_text_options_enforce_character_and_block_limits() {
        let shim = FakeShim::new().unwrap();
        let mut doc = shim.open_document("fake.pdf").unwrap();

        let mut char_limited = TextRequest::page(0);
        char_limited.max_chars = 2;
        let err = doc.extract_text(&char_limited).unwrap_err();
        assert_eq!(err.status, raw::pdbg_status::PDBG_ERROR_LIMIT);
        assert!(err.message.contains("character limit"));

        let mut block_limited = TextRequest::page(0);
        block_limited.max_blocks = 1;
        let err = doc.extract_text(&block_limited).unwrap_err();
        assert_eq!(err.status, raw::pdbg_status::PDBG_ERROR_LIMIT);
        assert!(err.message.contains("block limit"));

        let mut default_block_limit = TextRequest::page(0);
        default_block_limit.max_blocks = 0;
        let text = doc.extract_text(&default_block_limit).unwrap();
        assert_eq!(text.spans.len(), 2);
    }

    #[cfg(all(feature = "fake", not(feature = "real-mupdf")))]
    #[test]
    fn fake_visual_options_enforce_element_limit() {
        let shim = FakeShim::new().unwrap();
        let mut doc = shim.open_document("fake.pdf").unwrap();

        let mut limited = VisualRequest::page(0);
        limited.max_elements = 2;
        let err = doc.extract_visuals(&limited).unwrap_err();
        assert_eq!(err.status, raw::pdbg_status::PDBG_ERROR_LIMIT);
        assert!(err.message.contains("element limit"));

        let mut default_limit = VisualRequest::page(0);
        default_limit.max_elements = 0;
        let visuals = doc.extract_visuals(&default_limit).unwrap();
        assert_eq!(visuals.elements.len(), 3);
    }

    #[cfg(feature = "fake")]
    #[test]
    fn opaque_accessor_outputs_are_owned_after_handles_drop() {
        let (children, detail, stream, render, text) = {
            let shim = FakeShim::new().unwrap();
            let mut doc = shim.open_document("fake.pdf").unwrap();
            let summary = doc.summary().unwrap();
            let root = NodeId::DocumentRoot {
                doc: summary.doc.clone(),
            };
            let range = ChildRange {
                offset: 0,
                limit: 2,
            };
            let children = doc
                .children(&root, range, ChildContainer::Dictionary)
                .unwrap();
            let first_child = children.items[0].id.clone();
            let detail = doc.object_detail(&first_child, range).unwrap();
            let stream = doc
                .stream_load(ObjectId { num: 1, gen: 0 }, StreamMode::Raw, 0, 32)
                .unwrap();
            let render = doc.render_page(&RenderRequest::page(0)).unwrap();
            let text = doc.extract_text(&TextRequest::page(0)).unwrap();
            (children, detail, stream, render, text)
        };

        assert_eq!(children.items[0].label, "Object 0");
        assert_eq!(detail.preview, "<< /Type /Fake >>");
        assert_eq!(stream.bytes, b"fake stream bytes");
        assert_eq!(render.pixels_rgba, vec![255, 255, 255, 255]);
        assert_eq!(text.spans[0].text.as_bytes(), b"A\0B");
    }

    #[cfg(feature = "fake")]
    #[test]
    fn decoded_stream_limit_returns_limit_error_before_buffer_materialization() {
        let shim = FakeShim::new().unwrap();
        let config = SafeModeConfig {
            max_decoded_stream_bytes: 8,
            ..SafeModeConfig::default()
        };
        let mut doc = shim.open_document_with_config("fake.pdf", &config).unwrap();
        let requested_output_limit = 4;
        assert!(requested_output_limit <= config.max_decoded_stream_bytes as usize);

        let err = doc
            .stream_load(
                ObjectId { num: 1, gen: 0 },
                StreamMode::Decoded,
                0,
                requested_output_limit,
            )
            .unwrap_err();

        assert_eq!(err.status, raw::pdbg_status::PDBG_ERROR_LIMIT);
        assert!(err.message.contains("during decode"));
    }

    #[cfg(all(unix, feature = "fake"))]
    #[test]
    fn open_fd_keeps_caller_fd_usable_after_document_drop() {
        use std::fs;
        use std::io::{Read, Seek, SeekFrom, Write};
        use std::os::fd::{AsFd, AsRawFd};

        let (path, mut file) = temp_pdf_file();

        let shim = FakeShim::new().unwrap();
        let owned_fd;
        let owned_fd_identity;
        {
            let mut doc = shim
                .open_document_fd(file.as_fd(), "fd-backed.pdf", &SafeModeConfig::default())
                .unwrap();
            owned_fd = doc
                .fake_owned_fd_raw()
                .expect("fake document owns fd clone");
            assert!(owned_fd >= 0);
            assert_ne!(owned_fd, file.as_raw_fd());
            owned_fd_identity = fd_file_identity(owned_fd).expect("owned fd identity before drop");
            assert_eq!(doc.summary().unwrap().file_path, "fake.pdf");
        }
        assert_ne!(fd_file_identity(owned_fd), Some(owned_fd_identity));

        file.write_all(b"\ncaller fd still open").unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        assert!(contents.contains("caller fd still open"));
        fs::remove_file(path).unwrap();
    }

    #[cfg(all(unix, feature = "fake"))]
    #[test]
    fn open_fd_failure_keeps_caller_fd_usable() {
        use std::fs;
        use std::io::{Read, Seek, SeekFrom, Write};
        use std::os::fd::AsFd;

        let (path, mut file) = temp_pdf_file();

        let shim = FakeShim::new().unwrap();
        let err = match shim.open_document_fd(file.as_fd(), "fail-open", &SafeModeConfig::default())
        {
            Ok(_) => panic!("expected fake open failure"),
            Err(err) => err,
        };
        assert_eq!(err.status, raw::pdbg_status::PDBG_ERROR_GENERIC);
        assert!(err.message.contains("fake open failure"));

        file.write_all(b"\ncaller fd survived failed open").unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        assert!(contents.contains("caller fd survived failed open"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stable_public_strings_are_pinned() {
        assert_eq!(
            DiagnosticCode::JavaScriptDisabled.as_public_str(),
            "javascript_disabled"
        );
        assert_eq!(ResourceGroup::XObjects.as_public_str(), "xobjects");
    }

    #[cfg(feature = "real-mupdf")]
    fn write_temp_real_pdf(prefix: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "pdbg-core-{}-{}-{}.pdf",
            prefix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[cfg(feature = "real-mupdf")]
    fn mutool_path() -> std::path::PathBuf {
        if let Some(path) = std::env::var_os("PDBG_MUTOOL_PATH") {
            return std::path::PathBuf::from(path);
        }
        let source_dir = std::env::var_os("PDBG_MUPDF_SOURCE_DIR")
            .expect("real encrypted test requires PDBG_MUPDF_SOURCE_DIR or PDBG_MUTOOL_PATH");
        let path = std::path::PathBuf::from(source_dir)
            .join("build")
            .join("release")
            .join("mutool");
        assert!(
            path.is_file(),
            "real encrypted test requires mutool at {}; build it with `make build=release build/release/mutool` or set PDBG_MUTOOL_PATH",
            path.display()
        );
        path
    }

    #[cfg(feature = "real-mupdf")]
    fn encrypted_minimal_pdf_path() -> std::path::PathBuf {
        let input = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/synthetic/minimal.pdf"
        );
        let output = std::env::temp_dir().join(format!(
            "pdbg-core-encrypted-{}-{}.pdf",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let status = std::process::Command::new(mutool_path())
            .args([
                "clean", "-E", "aes-128", "-O", "owner", "-U", "user", "-P", "0", input,
            ])
            .arg(&output)
            .status()
            .expect("failed to run mutool");
        assert!(
            status.success(),
            "mutool failed to create encrypted fixture"
        );
        output
    }

    #[cfg(feature = "real-mupdf")]
    fn repairable_minimal_pdf_bytes() -> Vec<u8> {
        let mut bytes = include_bytes!("../../../fixtures/synthetic/minimal.pdf").to_vec();
        let needle = b"startxref\n184\n";
        let pos = bytes
            .windows(needle.len())
            .position(|window| window == needle)
            .unwrap();
        bytes.splice(pos..pos + needle.len(), b"startxref\n0\n".iter().copied());
        bytes
    }

    #[cfg(feature = "real-mupdf")]
    fn synthetic_external_reference_pdf() -> Vec<u8> {
        fn push_obj(pdf: &mut String, offsets: &mut Vec<usize>, body: &str) {
            offsets.push(pdf.len());
            pdf.push_str(body);
        }

        let mut pdf = String::from("%PDF-1.1\n");
        let mut offsets = Vec::new();
        push_obj(
            &mut pdf,
            &mut offsets,
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R /OpenAction 4 0 R >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "2 0 obj\n<< /Type /Pages /Count 1 /Kids [3 0 R] >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "4 0 obj\n<< /S /URI /URI (https://example.invalid/payload) >>\nendobj\n",
        );

        let xref_offset = pdf.len();
        pdf.push_str("xref\n0 5\n0000000000 65535 f \n");
        for offset in offsets {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Root 1 0 R /Size 5 >>\nstartxref\n{xref_offset}\n%%EOF\n"
        ));
        pdf.into_bytes()
    }

    #[cfg(feature = "real-mupdf")]
    fn synthetic_embedded_file_pdf() -> Vec<u8> {
        fn push_obj(pdf: &mut String, offsets: &mut Vec<usize>, body: &str) {
            offsets.push(pdf.len());
            pdf.push_str(body);
        }

        let mut pdf = String::from("%PDF-1.1\n");
        let mut offsets = Vec::new();
        push_obj(
            &mut pdf,
            &mut offsets,
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R /Names << /EmbeddedFiles << /Names [(payload.txt) 4 0 R] >> >> >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "2 0 obj\n<< /Type /Pages /Count 1 /Kids [3 0 R] >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "4 0 obj\n<< /Type /Filespec /F (payload.txt) /EF << /F 5 0 R >> >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "5 0 obj\n<< /Type /EmbeddedFile /Length 5 >>\nstream\nhello\nendstream\nendobj\n",
        );

        let xref_offset = pdf.len();
        pdf.push_str("xref\n0 6\n0000000000 65535 f \n");
        for offset in offsets {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Root 1 0 R /Size 6 >>\nstartxref\n{xref_offset}\n%%EOF\n"
        ));
        pdf.into_bytes()
    }

    #[cfg(feature = "real-mupdf")]
    fn synthetic_text_pdf() -> Vec<u8> {
        fn push_obj(pdf: &mut String, offsets: &mut Vec<usize>, body: &str) {
            offsets.push(pdf.len());
            pdf.push_str(body);
        }

        let content = "BT\n/F1 12 Tf\n10 60 Td\n(Hello M3) Tj\nET\n";
        let mut pdf = String::from("%PDF-1.1\n");
        let mut offsets = Vec::new();
        push_obj(
            &mut pdf,
            &mut offsets,
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "2 0 obj\n<< /Type /Pages /Count 1 /Kids [3 0 R] >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            &format!(
                "5 0 obj\n<< /Length {} >>\nstream\n{}endstream\nendobj\n",
                content.len(),
                content
            ),
        );

        let xref_offset = pdf.len();
        pdf.push_str("xref\n0 6\n0000000000 65535 f \n");
        for offset in offsets {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Root 1 0 R /Size 6 >>\nstartxref\n{xref_offset}\n%%EOF\n"
        ));
        pdf.into_bytes()
    }

    #[cfg(feature = "real-mupdf")]
    fn synthetic_cropped_text_pdf() -> Vec<u8> {
        fn push_obj(pdf: &mut String, offsets: &mut Vec<usize>, body: &str) {
            offsets.push(pdf.len());
            pdf.push_str(body);
        }

        let content = "BT\n/F1 12 Tf\n25 135 Td\n(Crop M3) Tj\nET\n";
        let mut pdf = String::from("%PDF-1.1\n");
        let mut offsets = Vec::new();
        push_obj(
            &mut pdf,
            &mut offsets,
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "2 0 obj\n<< /Type /Pages /Count 1 /Kids [3 0 R] >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /CropBox [20 50 120 150] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            &format!(
                "5 0 obj\n<< /Length {} >>\nstream\n{}endstream\nendobj\n",
                content.len(),
                content
            ),
        );

        let xref_offset = pdf.len();
        pdf.push_str("xref\n0 6\n0000000000 65535 f \n");
        for offset in offsets {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Root 1 0 R /Size 6 >>\nstartxref\n{xref_offset}\n%%EOF\n"
        ));
        pdf.into_bytes()
    }

    #[cfg(feature = "real-mupdf")]
    fn synthetic_stream_pdf() -> Vec<u8> {
        fn push_obj(pdf: &mut String, offsets: &mut Vec<usize>, body: &str) {
            offsets.push(pdf.len());
            pdf.push_str(body);
        }

        let mut pdf = String::from("%PDF-1.1\n");
        let mut offsets = Vec::new();
        push_obj(
            &mut pdf,
            &mut offsets,
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "2 0 obj\n<< /Type /Pages /Count 1 /Kids [3 0 R] >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] /Contents 4 0 R >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "4 0 obj\n<< /Length 6 /Filter /FlateDecode >>\nstream\nABCDEF\nendstream\nendobj\n",
        );

        let xref_offset = pdf.len();
        pdf.push_str("xref\n0 5\n0000000000 65535 f \n");
        for offset in offsets {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Root 1 0 R /Size 5 >>\nstartxref\n{xref_offset}\n%%EOF\n"
        ));
        pdf.into_bytes()
    }

    #[cfg(feature = "real-mupdf")]
    fn synthetic_ascii_hex_stream_pdf() -> Vec<u8> {
        fn push_obj(pdf: &mut String, offsets: &mut Vec<usize>, body: &str) {
            offsets.push(pdf.len());
            pdf.push_str(body);
        }

        let mut pdf = String::from("%PDF-1.1\n");
        let mut offsets = Vec::new();
        push_obj(
            &mut pdf,
            &mut offsets,
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "2 0 obj\n<< /Type /Pages /Count 1 /Kids [3 0 R] >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] /Contents 4 0 R >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "4 0 obj\n<< /Length 11 /DL 5 /Filter /ASCIIHexDecode >>\nstream\n48656c6c6f>\nendstream\nendobj\n",
        );

        let xref_offset = pdf.len();
        pdf.push_str("xref\n0 5\n0000000000 65535 f \n");
        for offset in offsets {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Root 1 0 R /Size 5 >>\nstartxref\n{xref_offset}\n%%EOF\n"
        ));
        pdf.into_bytes()
    }

    #[cfg(feature = "real-mupdf")]
    fn synthetic_generation_pdf() -> Vec<u8> {
        fn push_obj(pdf: &mut String, offsets: &mut Vec<(usize, u16)>, gen: u16, body: &str) {
            offsets.push((pdf.len(), gen));
            pdf.push_str(body);
        }

        let mut pdf = String::from("%PDF-1.1\n");
        let mut offsets = Vec::new();
        push_obj(
            &mut pdf,
            &mut offsets,
            0,
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            0,
            "2 0 obj\n<< /Type /Pages /Count 1 /Kids [3 0 R] >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            0,
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            7,
            "4 7 obj\n<< /GenerationMarker true >>\nendobj\n",
        );

        let xref_offset = pdf.len();
        pdf.push_str("xref\n0 5\n0000000000 65535 f \n");
        for (offset, gen) in offsets {
            pdf.push_str(&format!("{offset:010} {gen:05} n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Root 1 0 R /Size 5 >>\nstartxref\n{xref_offset}\n%%EOF\n"
        ));
        pdf.into_bytes()
    }

    #[cfg(feature = "real-mupdf")]
    fn synthetic_large_ascii_hex_stream_pdf(decoded_len: usize) -> Vec<u8> {
        fn push_obj(pdf: &mut String, offsets: &mut Vec<usize>, body: &str) {
            offsets.push(pdf.len());
            pdf.push_str(body);
        }

        let mut hex = String::with_capacity(decoded_len * 2 + 1);
        for _ in 0..decoded_len {
            hex.push_str("41");
        }
        hex.push('>');

        let mut pdf = String::from("%PDF-1.1\n");
        let mut offsets = Vec::new();
        push_obj(
            &mut pdf,
            &mut offsets,
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "2 0 obj\n<< /Type /Pages /Count 1 /Kids [3 0 R] >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] /Contents 4 0 R >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            &format!(
                "4 0 obj\n<< /Length {} /Filter /ASCIIHexDecode >>\nstream\n{}\nendstream\nendobj\n",
                hex.len(),
                hex
            ),
        );

        let xref_offset = pdf.len();
        pdf.push_str("xref\n0 5\n0000000000 65535 f \n");
        for offset in offsets {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Root 1 0 R /Size 5 >>\nstartxref\n{xref_offset}\n%%EOF\n"
        ));
        pdf.into_bytes()
    }

    #[cfg(feature = "real-mupdf")]
    fn synthetic_flate_expansion_pdf() -> Vec<u8> {
        fn push_obj(pdf: &mut Vec<u8>, offsets: &mut Vec<usize>, body: &[u8]) {
            offsets.push(pdf.len());
            pdf.extend_from_slice(body);
        }

        let flate = [
            0x78, 0x9c, 0x73, 0x74, 0x1c, 0x05, 0xa3, 0x60, 0x14, 0x8c, 0x54, 0x00, 0x00, 0xa4,
            0x78, 0x04, 0x10,
        ];
        let mut pdf = Vec::from(&b"%PDF-1.1\n"[..]);
        let mut offsets = Vec::new();
        push_obj(
            &mut pdf,
            &mut offsets,
            b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            b"2 0 obj\n<< /Type /Pages /Count 1 /Kids [3 0 R] >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] /Contents 4 0 R >>\nendobj\n",
        );
        offsets.push(pdf.len());
        pdf.extend_from_slice(
            format!(
                "4 0 obj\n<< /Length {} /Filter /FlateDecode >>\nstream\n",
                flate.len()
            )
            .as_bytes(),
        );
        pdf.extend_from_slice(&flate);
        pdf.extend_from_slice(b"\nendstream\nendobj\n");

        let xref_offset = pdf.len();
        pdf.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \n");
        for offset in offsets {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!("trailer\n<< /Root 1 0 R /Size 5 >>\nstartxref\n{xref_offset}\n%%EOF\n")
                .as_bytes(),
        );
        pdf
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_opens_minimal_pdf_and_traverses_inspect_roots() {
        let fixture = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/synthetic/minimal.pdf"
        );
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim.open_document(fixture).unwrap();
        let summary = doc.summary().unwrap();
        assert_eq!(summary.page_count, 1);
        assert!(summary.xref_size > 0);
        assert!(summary.safety.safe_mode);
        assert!(summary.safety.javascript_disabled);

        let root = NodeId::DocumentRoot {
            doc: summary.doc.clone(),
        };
        let range = ChildRange {
            offset: 0,
            limit: 16,
        };
        let children = doc
            .children(&root, range, ChildContainer::Dictionary)
            .unwrap();
        assert_eq!(children.total, Some(4));
        assert_eq!(children.items.len(), 4);
        assert_eq!(children.items[0].label, "Trailer");
        assert_eq!(children.items[2].label, "Pages");
        assert_eq!(children.items[3].label, "Xref");

        let trailer = doc.object_detail(&children.items[0].id, range).unwrap();
        assert_eq!(trailer.kind, ObjectKind::Trailer);
        assert!(trailer.dictionary_entries.unwrap().total.is_some());

        let pages = doc
            .children(&children.items[2].id, range, ChildContainer::Array)
            .unwrap();
        assert_eq!(pages.total, Some(1));
        assert!(matches!(pages.items[0].id, NodeId::ArrayEntry { .. }));

        let unsupported = doc
            .stream_load(ObjectId { num: 1, gen: 0 }, StreamMode::Raw, 0, 16)
            .unwrap_err();
        assert_eq!(unsupported.status, raw::pdbg_status::PDBG_ERROR_UNSUPPORTED);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_reports_encrypted_summary_before_authentication() {
        let path = encrypted_minimal_pdf_path();
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim.open_document(path.to_string_lossy().as_ref()).unwrap();
        let summary = doc.summary().unwrap();
        assert!(summary.encrypted);
        assert!(summary.needs_password);
        assert_eq!(summary.page_count, 0);
        assert_eq!(summary.xref_size, 0);

        let root = NodeId::DocumentRoot {
            doc: summary.doc.clone(),
        };
        let children_err = doc
            .children(
                &root,
                ChildRange {
                    offset: 0,
                    limit: 16,
                },
                ChildContainer::Dictionary,
            )
            .unwrap_err();
        assert_eq!(children_err.status, raw::pdbg_status::PDBG_ERROR_PASSWORD);

        let detail_err = doc
            .object_detail(
                &root,
                ChildRange {
                    offset: 0,
                    limit: 16,
                },
            )
            .unwrap_err();
        assert_eq!(detail_err.status, raw::pdbg_status::PDBG_ERROR_PASSWORD);

        let stream_err = doc
            .stream_load(ObjectId { num: 1, gen: 0 }, StreamMode::Raw, 0, 16)
            .unwrap_err();
        assert_eq!(stream_err.status, raw::pdbg_status::PDBG_ERROR_PASSWORD);

        drop(doc);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_reports_wrong_password_diagnostic_on_open_error() {
        let path = encrypted_minimal_pdf_path();
        let shim = RealMuPdfShim::new().unwrap();
        let err = match shim.open_document_with_password_and_config(
            path.to_string_lossy().as_ref(),
            "wrong-password",
            &SafeModeConfig::default(),
        ) {
            Ok(_) => panic!("expected wrong-password open to fail"),
            Err(err) => err,
        };

        assert_eq!(err.status, raw::pdbg_status::PDBG_ERROR_PASSWORD);
        assert!(err.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == DiagnosticCode::EncryptionPasswordFailure
                && diagnostic.severity == DiagnosticSeverity::Error
        }));

        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_reports_repair_warning_on_summary() {
        let path = write_temp_real_pdf("repairable", &repairable_minimal_pdf_bytes());
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim.open_document(path.to_string_lossy().as_ref()).unwrap();
        let summary = doc.summary().unwrap();

        assert!(summary.safety.repaired_or_damaged);
        assert!(summary.parsed_object_count.is_some_and(|count| count > 0));
        assert!(summary.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == DiagnosticCode::RepairWarning
                && diagnostic.severity == DiagnosticSeverity::Warning
        }));

        drop(doc);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn fake_shim_xref_table_is_bounded_and_deterministic() {
        let shim = FakeShim::new().unwrap();
        let mut doc = shim.open_document("fake.pdf").unwrap();

        let all = doc
            .xref_table(ChildRange {
                offset: 0,
                limit: 16,
            })
            .unwrap();
        assert_eq!(all.total, 3);
        assert_eq!(all.offset, 0);
        assert_eq!(all.items.len(), 3);
        assert_eq!(all.items[0].kind, XrefEntryKind::Free);
        assert_eq!(all.items[0].object.gen, 65535);
        assert_eq!(all.items[1].kind, XrefEntryKind::Normal);
        assert_eq!(all.items[1].offset, 100);

        let tail = doc
            .xref_table(ChildRange {
                offset: 2,
                limit: 16,
            })
            .unwrap();
        assert_eq!(tail.offset, 2);
        assert_eq!(tail.items.len(), 1);

        let beyond = doc
            .xref_table(ChildRange {
                offset: 10,
                limit: 4,
            })
            .unwrap();
        assert!(beyond.items.is_empty());
        assert_eq!(beyond.total, 3);
        assert_eq!(all.sections, 1);
        assert!(all.items.iter().all(|entry| entry.section == Some(0)));
    }

    #[cfg(feature = "real-mupdf")]
    fn synthetic_incremental_update_pdf() -> Vec<u8> {
        fn push_obj(pdf: &mut String, offsets: &mut Vec<usize>, body: &str) {
            offsets.push(pdf.len());
            pdf.push_str(body);
        }

        let mut pdf = String::from("%PDF-1.1\n");
        let mut offsets = Vec::new();
        push_obj(
            &mut pdf,
            &mut offsets,
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "2 0 obj\n<< /Type /Pages /Count 1 /Kids [3 0 R] >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] >>\nendobj\n",
        );

        let first_xref = pdf.len();
        pdf.push_str("xref\n0 4\n0000000000 65535 f \n");
        for offset in &offsets {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Root 1 0 R /Size 4 >>\nstartxref\n{first_xref}\n%%EOF\n"
        ));

        // Incremental update rewriting the page object.
        let updated_obj_offset = pdf.len();
        pdf.push_str("3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 144 144] >>\nendobj\n");
        let second_xref = pdf.len();
        pdf.push_str("xref\n3 1\n");
        pdf.push_str(&format!("{updated_obj_offset:010} 00000 n \n"));
        pdf.push_str(&format!(
            "trailer\n<< /Root 1 0 R /Size 4 /Prev {first_xref} >>\nstartxref\n{second_xref}\n%%EOF\n"
        ));
        pdf.into_bytes()
    }

    #[cfg(feature = "real-mupdf")]
    fn synthetic_image_pdf() -> Vec<u8> {
        fn push_obj(pdf: &mut String, offsets: &mut Vec<usize>, body: &str) {
            offsets.push(pdf.len());
            pdf.push_str(body);
        }

        let mut pdf = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        push_obj(
            &mut pdf,
            &mut offsets,
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "2 0 obj\n<< /Type /Pages /Count 1 /Kids [3 0 R] >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] \
             /Resources << /XObject << /Im0 4 0 R >> >> >>\nendobj\n",
        );
        // 2x2 uncompressed RGB image; pixel bytes chosen ASCII-safe.
        push_obj(
            &mut pdf,
            &mut offsets,
            "4 0 obj\n<< /Type /XObject /Subtype /Image /Width 2 /Height 2 \
             /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length 12 >>\n\
             stream\nAAABBBCCCDDD\nendstream\nendobj\n",
        );

        let xref_offset = pdf.len();
        pdf.push_str("xref\n0 5\n0000000000 65535 f \n");
        for offset in offsets {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Root 1 0 R /Size 5 >>\nstartxref\n{xref_offset}\n%%EOF\n"
        ));
        pdf.into_bytes()
    }

    #[test]
    fn fake_shim_stream_save_writes_and_caps() {
        let shim = FakeShim::new().unwrap();
        let mut doc = shim.open_document("fake.pdf").unwrap();
        let object = ObjectId { num: 2, gen: 0 };
        let path = std::env::temp_dir().join(format!(
            "pdbg-fake-save-{}-{}.bin",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path_str = path.to_string_lossy().into_owned();

        let outcome = doc
            .stream_save(object, StreamMode::Raw, &path_str, 0)
            .unwrap();
        assert_eq!(outcome.bytes_written, 17);
        assert!(!outcome.capped);
        assert_eq!(std::fs::read(&path).unwrap(), b"fake stream bytes");

        let capped = doc
            .stream_save(object, StreamMode::Raw, &path_str, 5)
            .unwrap();
        assert_eq!(capped.bytes_written, 5);
        assert!(capped.capped);
        assert_eq!(std::fs::read(&path).unwrap(), b"fake ");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn fake_shim_image_preview_is_unsupported() {
        let shim = FakeShim::new().unwrap();
        let mut doc = shim.open_document("fake.pdf").unwrap();
        let err = doc
            .image_preview(ObjectId { num: 2, gen: 0 }, 64)
            .unwrap_err();
        assert_eq!(err.status, raw::pdbg_status::PDBG_ERROR_UNSUPPORTED);
    }

    #[cfg(feature = "real-mupdf")]
    fn synthetic_large_stream_pdf(stream_len: usize) -> Vec<u8> {
        fn push_obj(pdf: &mut String, offsets: &mut Vec<usize>, body: &str) {
            offsets.push(pdf.len());
            pdf.push_str(body);
        }

        let mut pdf = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        push_obj(
            &mut pdf,
            &mut offsets,
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "2 0 obj\n<< /Type /Pages /Count 1 /Kids [3 0 R] >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] >>\nendobj\n",
        );
        offsets.push(pdf.len());
        pdf.push_str(&format!("4 0 obj\n<< /Length {stream_len} >>\nstream\n"));
        pdf.push_str(&"A".repeat(stream_len));
        pdf.push_str("\nendstream\nendobj\n");

        let xref_offset = pdf.len();
        pdf.push_str("xref\n0 5\n0000000000 65535 f \n");
        for offset in offsets {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Root 1 0 R /Size 5 >>\nstartxref\n{xref_offset}\n%%EOF\n"
        ));
        pdf.into_bytes()
    }

    #[cfg(feature = "real-mupdf")]
    fn temp_save_path(prefix: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "pdbg-save-{}-{}-{}.bin",
            prefix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_stream_save_streams_large_objects_in_one_pass() {
        // 200 KB forces several 64 KB read iterations inside pdbg_stream_save.
        let stream_len = 200_000usize;
        let pdf_path =
            write_temp_real_pdf("stream-save-large", &synthetic_large_stream_pdf(stream_len));
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim
            .open_document(pdf_path.to_string_lossy().as_ref())
            .unwrap();
        let object = ObjectId { num: 4, gen: 0 };

        let raw_path = temp_save_path("raw");
        let outcome = doc
            .stream_save(object, StreamMode::Raw, &raw_path.to_string_lossy(), 0)
            .unwrap();
        assert_eq!(outcome.bytes_written, stream_len as u64);
        assert!(!outcome.capped);
        let written = std::fs::read(&raw_path).unwrap();
        assert_eq!(written.len(), stream_len);
        assert!(written.iter().all(|byte| *byte == b'A'));

        // No filter: the decoded branch must produce identical bytes.
        let decoded_path = temp_save_path("decoded");
        let decoded = doc
            .stream_save(
                object,
                StreamMode::Decoded,
                &decoded_path.to_string_lossy(),
                0,
            )
            .unwrap();
        assert_eq!(decoded.bytes_written, stream_len as u64);
        assert_eq!(std::fs::read(&decoded_path).unwrap(), written);

        let _ = std::fs::remove_file(&raw_path);
        let _ = std::fs::remove_file(&decoded_path);
        let _ = std::fs::remove_file(&pdf_path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_stream_save_honors_cap_cancel_and_rejects_non_streams() {
        let stream_len = 100_000usize;
        let pdf_path =
            write_temp_real_pdf("stream-save-edges", &synthetic_large_stream_pdf(stream_len));
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim
            .open_document(pdf_path.to_string_lossy().as_ref())
            .unwrap();
        let object = ObjectId { num: 4, gen: 0 };

        // Size cap: stops exactly at max_bytes and reports it.
        let capped_path = temp_save_path("capped");
        let capped = doc
            .stream_save(
                object,
                StreamMode::Raw,
                &capped_path.to_string_lossy(),
                1000,
            )
            .unwrap();
        assert_eq!(capped.bytes_written, 1000);
        assert!(capped.capped);
        assert_eq!(std::fs::read(&capped_path).unwrap().len(), 1000);

        // Cancellation: fails with CANCELLED and removes the partial file.
        let cancelled_path = temp_save_path("cancelled");
        let token = CancelToken::new().unwrap();
        token.cancel();
        let err = doc
            .stream_save_with_cancel_token(
                object,
                StreamMode::Raw,
                &cancelled_path.to_string_lossy(),
                0,
                &token,
            )
            .unwrap_err();
        assert_eq!(err.status, raw::pdbg_status::PDBG_ERROR_CANCELLED);
        assert!(!cancelled_path.exists());

        // Non-stream object: rejected without creating a file.
        let rejected_path = temp_save_path("rejected");
        let err = doc
            .stream_save(
                ObjectId { num: 1, gen: 0 },
                StreamMode::Raw,
                &rejected_path.to_string_lossy(),
                0,
            )
            .unwrap_err();
        assert_eq!(err.status, raw::pdbg_status::PDBG_ERROR_UNSUPPORTED);
        assert!(!rejected_path.exists());

        let _ = std::fs::remove_file(&capped_path);
        let _ = std::fs::remove_file(&pdf_path);
    }

    #[cfg(all(feature = "real-mupdf", unix))]
    #[test]
    fn real_mupdf_stream_save_replaces_path_without_following_symlink() {
        let pdf_path = write_temp_real_pdf("stream-save-symlink", &synthetic_large_stream_pdf(6));
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim
            .open_document(pdf_path.to_string_lossy().as_ref())
            .unwrap();
        let protected_path = temp_save_path("protected");
        let target_path = temp_save_path("symlink-target");
        std::fs::write(&protected_path, b"keep me").unwrap();
        let _ = std::fs::remove_file(&target_path);
        std::os::unix::fs::symlink(&protected_path, &target_path).unwrap();

        // A direct fopen(path, "wb") follows the symlink and corrupts the
        // protected file. The export path must write a sibling temp file and
        // rename it over the selected path instead.
        let object = ObjectId { num: 4, gen: 0 };
        let outcome = doc
            .stream_save(object, StreamMode::Raw, &target_path.to_string_lossy(), 0)
            .unwrap();
        assert_eq!(outcome.bytes_written, 6);
        assert_eq!(std::fs::read(&protected_path).unwrap(), b"keep me");
        assert!(!std::fs::symlink_metadata(&target_path)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(std::fs::read(&target_path).unwrap(), b"AAAAAA");

        let _ = std::fs::remove_file(&protected_path);
        let _ = std::fs::remove_file(&target_path);
        let _ = std::fs::remove_file(&pdf_path);
    }

    #[cfg(feature = "real-mupdf")]
    fn synthetic_gray_image_pdf() -> Vec<u8> {
        fn push_obj(pdf: &mut String, offsets: &mut Vec<usize>, body: &str) {
            offsets.push(pdf.len());
            pdf.push_str(body);
        }

        let mut pdf = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        push_obj(
            &mut pdf,
            &mut offsets,
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "2 0 obj\n<< /Type /Pages /Count 1 /Kids [3 0 R] >>\nendobj\n",
        );
        push_obj(
            &mut pdf,
            &mut offsets,
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] \
             /Resources << /XObject << /Im0 4 0 R >> >> >>\nendobj\n",
        );
        // 2x2 grayscale: two dark pixels, two light pixels.
        push_obj(
            &mut pdf,
            &mut offsets,
            "4 0 obj\n<< /Type /XObject /Subtype /Image /Width 2 /Height 2 \
             /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 4 >>\n\
             stream\n  zz\nendstream\nendobj\n",
        );

        let xref_offset = pdf.len();
        pdf.push_str("xref\n0 5\n0000000000 65535 f \n");
        for offset in offsets {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Root 1 0 R /Size 5 >>\nstartxref\n{xref_offset}\n%%EOF\n"
        ));
        pdf.into_bytes()
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_image_preview_converts_gray_colorspace() {
        let pdf_path = write_temp_real_pdf("gray-image", &synthetic_gray_image_pdf());
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim
            .open_document(pdf_path.to_string_lossy().as_ref())
            .unwrap();

        let preview = doc.image_preview(ObjectId { num: 4, gen: 0 }, 64).unwrap();
        assert_eq!((preview.width, preview.height), (2, 2));
        assert_eq!(preview.pixels_rgba.len(), 16);
        for pixel in preview.pixels_rgba.chunks(4) {
            // Gray converts to R == G == B with synthesized opaque alpha.
            assert_eq!(pixel[0], pixel[1]);
            assert_eq!(pixel[1], pixel[2]);
            assert_eq!(pixel[3], 0xFF);
        }
        // Dark pixels (0x20) and light pixels (0x7A) stay distinguishable.
        assert_ne!(preview.pixels_rgba[0], preview.pixels_rgba[8]);

        drop(doc);
        let _ = std::fs::remove_file(&pdf_path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_decodes_image_object_preview() {
        let path = write_temp_real_pdf("image-preview", &synthetic_image_pdf());
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim.open_document(path.to_string_lossy().as_ref()).unwrap();

        let summary = doc.summary().unwrap();
        let image_object = ObjectId { num: 4, gen: 0 };
        let node = NodeId::XrefObject {
            doc: summary.doc.clone(),
            object: image_object,
        };
        let detail = doc
            .object_detail(
                &node,
                ChildRange {
                    offset: 0,
                    limit: 8,
                },
            )
            .unwrap();
        let stream = detail.stream.expect("image object has a stream");
        assert!(stream.image_preview_available);

        let preview = doc.image_preview(image_object, 64).unwrap();
        assert_eq!((preview.width, preview.height), (2, 2));
        assert_eq!(preview.stride, 8);
        assert_eq!(preview.pixels_rgba.len(), 16);
        // First pixel: bytes 'AAA' with synthesized opaque alpha.
        assert_eq!(&preview.pixels_rgba[0..4], &[0x41, 0x41, 0x41, 0xFF]);

        // Non-image objects are rejected rather than decoded.
        let err = doc
            .image_preview(ObjectId { num: 1, gen: 0 }, 64)
            .unwrap_err();
        assert_eq!(err.status, raw::pdbg_status::PDBG_ERROR_UNSUPPORTED);

        drop(doc);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_reports_xref_sections_for_incremental_updates() {
        let path = write_temp_real_pdf("xref-incremental", &synthetic_incremental_update_pdf());
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim.open_document(path.to_string_lossy().as_ref()).unwrap();

        let summary = doc.summary().unwrap();
        assert!(
            !summary.safety.repaired_or_damaged,
            "fixture should parse without repair"
        );

        let slice = doc
            .xref_table(ChildRange {
                offset: 0,
                limit: 64,
            })
            .unwrap();
        assert_eq!(slice.sections, 2);
        assert_eq!(slice.total, 4);
        assert_eq!(slice.items[1].section, Some(0));
        assert_eq!(slice.items[2].section, Some(0));
        assert_eq!(slice.items[3].section, Some(1));
        assert_eq!(slice.items[3].kind, XrefEntryKind::Normal);
        assert!(slice.items[3].offset > slice.items[2].offset);

        drop(doc);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_exposes_xref_entries_with_offsets() {
        let path = write_temp_real_pdf("xref-table", &synthetic_external_reference_pdf());
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim.open_document(path.to_string_lossy().as_ref()).unwrap();

        let slice = doc
            .xref_table(ChildRange {
                offset: 0,
                limit: 64,
            })
            .unwrap();
        assert_eq!(slice.total, 5);
        assert_eq!(slice.items.len(), 5);
        assert_eq!(slice.items[0].kind, XrefEntryKind::Free);
        for entry in &slice.items[1..] {
            assert_eq!(entry.kind, XrefEntryKind::Normal);
            assert!(entry.offset > 0);
        }
        assert!(slice.items[1].offset < slice.items[2].offset);

        let page = doc
            .xref_table(ChildRange {
                offset: 3,
                limit: 1,
            })
            .unwrap();
        assert_eq!(page.offset, 3);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].object.num, 3);
        assert_eq!(page.total, 5);
        assert_eq!(slice.sections, 1);
        assert!(slice.items.iter().all(|entry| entry.section == Some(0)));

        drop(doc);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_reports_external_reference_on_summary() {
        let path = write_temp_real_pdf("external-reference", &synthetic_external_reference_pdf());
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim.open_document(path.to_string_lossy().as_ref()).unwrap();
        let summary = doc.summary().unwrap();

        assert!(summary.safety.external_references_detected);
        assert!(summary.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == DiagnosticCode::ExternalReferenceDetected
                && diagnostic.severity == DiagnosticSeverity::Warning
        }));

        drop(doc);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_reports_embedded_file_on_summary() {
        let path = write_temp_real_pdf("embedded-file", &synthetic_embedded_file_pdf());
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim.open_document(path.to_string_lossy().as_ref()).unwrap();
        let summary = doc.summary().unwrap();

        assert!(summary.safety.embedded_files_detected);
        assert!(summary.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == DiagnosticCode::EmbeddedFileDetected
                && diagnostic.severity == DiagnosticSeverity::Warning
        }));

        drop(doc);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_rejects_repaired_pdf_when_repair_policy_is_never() {
        let path = write_temp_real_pdf("repair-policy-never", &repairable_minimal_pdf_bytes());
        let shim = RealMuPdfShim::new().unwrap();
        let config = SafeModeConfig {
            repair_policy: crate::RepairPolicy::Never,
            ..SafeModeConfig::default()
        };
        let err = match shim.open_document_with_config(path.to_string_lossy().as_ref(), &config) {
            Ok(_) => panic!("expected repair-policy rejection"),
            Err(err) => err,
        };

        assert_eq!(err.status, raw::pdbg_status::PDBG_ERROR_FORMAT);
        assert!(err.message.contains("repair policy"));

        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_reports_stream_summary_metadata() {
        let path = write_temp_real_pdf("stream-summary", &synthetic_stream_pdf());
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim.open_document(path.to_string_lossy().as_ref()).unwrap();
        let summary = doc.summary().unwrap();
        let xref = NodeId::XrefRoot {
            doc: summary.doc.clone(),
        };
        let range = ChildRange {
            offset: 3,
            limit: 1,
        };

        let xref_entries = doc.children(&xref, range, ChildContainer::Array).unwrap();
        assert_eq!(xref_entries.items.len(), 1);
        assert_eq!(
            xref_entries.items[0].object,
            Some(ObjectId { num: 4, gen: 0 })
        );

        let detail = doc.object_detail(&xref_entries.items[0].id, range).unwrap();
        let stream = detail.stream.unwrap();
        assert_eq!(stream.object, ObjectId { num: 4, gen: 0 });
        assert_eq!(stream.filters, vec!["FlateDecode"]);
        assert_eq!(stream.raw_size_hint, Some(6));
        assert_eq!(stream.decoded_size_hint, None);
        assert!(stream.can_decode);
        assert!(!stream.image_preview_available);

        drop(doc);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_preserves_xref_entry_generation() {
        let path = write_temp_real_pdf("xref-generation", &synthetic_generation_pdf());
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim.open_document(path.to_string_lossy().as_ref()).unwrap();
        let summary = doc.summary().unwrap();
        let xref = NodeId::XrefRoot {
            doc: summary.doc.clone(),
        };
        let range = ChildRange {
            offset: 3,
            limit: 1,
        };

        let xref_entries = doc.children(&xref, range, ChildContainer::Array).unwrap();
        assert_eq!(xref_entries.items.len(), 1);
        let entry = &xref_entries.items[0];
        assert_eq!(entry.object, Some(ObjectId { num: 4, gen: 7 }));
        assert!(entry.label.contains("4 7 R"));
        assert!(entry.preview.contains("4 7 R"));

        let detail = doc.object_detail(&entry.id, range).unwrap();
        assert_eq!(detail.object, Some(ObjectId { num: 4, gen: 7 }));

        drop(doc);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_reports_stream_decode_failure_diagnostic() {
        let path = write_temp_real_pdf("stream-decode-failure", &synthetic_stream_pdf());
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim.open_document(path.to_string_lossy().as_ref()).unwrap();

        let chunk = doc
            .stream_load(ObjectId { num: 4, gen: 0 }, StreamMode::Decoded, 0, 64)
            .unwrap();

        assert!(chunk.truncated);
        assert!(chunk.decode_diagnostics.iter().any(|diagnostic| {
            diagnostic.code == DiagnosticCode::StreamDecodeFailure
                && diagnostic.severity == DiagnosticSeverity::Warning
                && diagnostic.object == Some(ObjectId { num: 4, gen: 0 })
        }));

        drop(doc);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_loads_raw_and_decoded_stream_chunks() {
        let path = write_temp_real_pdf("stream-load", &synthetic_ascii_hex_stream_pdf());
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim.open_document(path.to_string_lossy().as_ref()).unwrap();
        let summary = doc.summary().unwrap();
        let detail = doc
            .object_detail(
                &NodeId::XrefObject {
                    doc: summary.doc,
                    object: ObjectId { num: 4, gen: 0 },
                },
                ChildRange {
                    offset: 0,
                    limit: 16,
                },
            )
            .unwrap();
        assert_eq!(detail.stream.as_ref().unwrap().decoded_size_hint, Some(5));

        let raw = doc
            .stream_load(ObjectId { num: 4, gen: 0 }, StreamMode::Raw, 0, 64)
            .unwrap();
        assert_eq!(raw.bytes, b"48656c6c6f>");
        assert_eq!(raw.total_size, Some(11));
        assert!(!raw.truncated);

        let raw_chunk = doc
            .stream_load(ObjectId { num: 4, gen: 0 }, StreamMode::Raw, 2, 4)
            .unwrap();
        assert_eq!(raw_chunk.bytes, b"656c");
        assert_eq!(raw_chunk.total_size, Some(11));
        assert!(raw_chunk.truncated);

        let decoded = doc
            .stream_load(ObjectId { num: 4, gen: 0 }, StreamMode::Decoded, 0, 64)
            .unwrap();
        assert_eq!(decoded.bytes, b"Hello");
        assert_eq!(decoded.total_size, Some(5));
        assert!(!decoded.truncated);

        let decoded_chunk = doc
            .stream_load(ObjectId { num: 4, gen: 0 }, StreamMode::Decoded, 1, 3)
            .unwrap();
        assert_eq!(decoded_chunk.bytes, b"ell");
        assert_eq!(decoded_chunk.total_size, Some(5));
        assert!(decoded_chunk.truncated);

        drop(doc);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_cancelled_stream_returns_clean_error_and_keeps_doc_usable() {
        let path = write_temp_real_pdf("stream-cancel", &synthetic_ascii_hex_stream_pdf());
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim.open_document(path.to_string_lossy().as_ref()).unwrap();
        let cancel = CancelToken::new().unwrap();
        cancel.cancel();

        let err = doc
            .stream_load_with_cancel_token(
                ObjectId { num: 4, gen: 0 },
                StreamMode::Decoded,
                0,
                64,
                &cancel,
            )
            .unwrap_err();
        assert_eq!(err.status, raw::pdbg_status::PDBG_ERROR_CANCELLED);
        assert!(err.message.contains("cancelled"));

        let summary = doc.summary().unwrap();
        assert_eq!(summary.page_count, 1);
        let decoded = doc
            .stream_load(ObjectId { num: 4, gen: 0 }, StreamMode::Decoded, 0, 64)
            .unwrap();
        assert_eq!(decoded.bytes, b"Hello");

        drop(doc);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_cross_thread_cancel_stops_large_decoded_stream() {
        const DECODED_LEN: usize = 8 * 1024 * 1024;
        let path = write_temp_real_pdf(
            "stream-mid-cancel",
            &synthetic_large_ascii_hex_stream_pdf(DECODED_LEN),
        );
        let shim = RealMuPdfShim::new().unwrap();
        let config = SafeModeConfig {
            max_decoded_stream_bytes: (DECODED_LEN as u64) + 1024,
            ..SafeModeConfig::default()
        };
        let mut doc = shim
            .open_document_with_config(path.to_string_lossy().as_ref(), &config)
            .unwrap();

        let cancel = std::sync::Arc::new(CancelToken::new().unwrap());
        let canceller = {
            let cancel = std::sync::Arc::clone(&cancel);
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(1));
                cancel.cancel();
            })
        };

        let err = doc
            .stream_load_with_cancel_token(
                ObjectId { num: 4, gen: 0 },
                StreamMode::Decoded,
                0,
                0,
                &cancel,
            )
            .unwrap_err();
        canceller.join().unwrap();
        assert_eq!(err.status, raw::pdbg_status::PDBG_ERROR_CANCELLED);

        let summary = doc.summary().unwrap();
        assert_eq!(summary.page_count, 1);

        drop(doc);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_enforces_decoded_stream_limit_during_read() {
        let path = write_temp_real_pdf("stream-limit", &synthetic_ascii_hex_stream_pdf());
        let shim = RealMuPdfShim::new().unwrap();
        let config = SafeModeConfig {
            max_decoded_stream_bytes: 4,
            ..SafeModeConfig::default()
        };
        let mut doc = shim
            .open_document_with_config(path.to_string_lossy().as_ref(), &config)
            .unwrap();

        let err = doc
            .stream_load(ObjectId { num: 4, gen: 0 }, StreamMode::Decoded, 0, 2)
            .unwrap_err();
        assert_eq!(err.status, raw::pdbg_status::PDBG_ERROR_LIMIT);
        assert!(err.message.contains("during decode"));

        drop(doc);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_enforces_filter_expansion_ratio_during_read() {
        let path = write_temp_real_pdf("stream-ratio-limit", &synthetic_flate_expansion_pdf());
        let shim = RealMuPdfShim::new().unwrap();
        let config = SafeModeConfig {
            max_decoded_stream_bytes: 4096,
            max_filter_expansion_ratio: 4,
            ..SafeModeConfig::default()
        };
        let mut doc = shim
            .open_document_with_config(path.to_string_lossy().as_ref(), &config)
            .unwrap();

        let err = doc
            .stream_load(ObjectId { num: 4, gen: 0 }, StreamMode::Decoded, 0, 16)
            .unwrap_err();
        assert_eq!(err.status, raw::pdbg_status::PDBG_ERROR_LIMIT);
        assert!(err.message.contains("expansion ratio"));

        drop(doc);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_renders_first_page_to_owned_rgba_pixels() {
        let fixture = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/synthetic/minimal.pdf"
        );
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim.open_document(fixture).unwrap();

        let render = doc.render_page(&RenderRequest::page(0)).unwrap();
        assert_eq!(render.page_index, 0);
        assert_eq!((render.width, render.height), (72, 72));
        assert!(render.stride >= render.width as usize * 4);
        assert_eq!(
            render.pixels_rgba.len(),
            render.stride * render.height as usize
        );
        assert_eq!(&render.pixels_rgba[..4], &[255, 255, 255, 255]);

        let mut inverted_request = RenderRequest::page(0);
        inverted_request.color_mode = RenderColorMode::Inverted;
        let inverted = doc.render_page(&inverted_request).unwrap();
        assert_eq!(&inverted.pixels_rgba[..4], &[0, 0, 0, 255]);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_enforces_render_pixel_limit() {
        let fixture = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/synthetic/minimal.pdf"
        );
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim.open_document(fixture).unwrap();
        let mut request = RenderRequest::page(0);
        request.max_pixels = 1;

        let err = doc.render_page(&request).unwrap_err();
        assert_eq!(err.status, raw::pdbg_status::PDBG_ERROR_LIMIT);
        assert!(err.message.contains("pixel"));
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_extracts_positioned_text() {
        let path = write_temp_real_pdf("text", &synthetic_text_pdf());
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim.open_document(path.to_string_lossy().as_ref()).unwrap();

        let text = doc.extract_text(&TextRequest::page(0)).unwrap();
        assert_eq!(text.page_index, 0);
        assert!(text.spans.iter().any(|span| span.text.contains("Hello M3")));
        let span = text
            .spans
            .iter()
            .find(|span| span.text.contains("Hello M3"))
            .unwrap();
        assert!(span.untrusted);
        assert!(span.bbox.x >= 0.0);
        assert!(span.bbox.y >= 0.0);
        assert!(span.bbox.width > 0.0);
        assert!(span.bbox.height > 0.0);

        drop(doc);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_normalizes_text_bbox_to_cropbox_top_left_space() {
        let path = write_temp_real_pdf("text-cropbox", &synthetic_cropped_text_pdf());
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim.open_document(path.to_string_lossy().as_ref()).unwrap();

        let text = doc.extract_text(&TextRequest::page(0)).unwrap();
        let span = text
            .spans
            .iter()
            .find(|span| span.text.contains("Crop M3"))
            .unwrap();

        assert!(
            (span.bbox.x - 5.0).abs() < 2.0,
            "expected CropBox-relative x near 5, got {}",
            span.bbox.x
        );
        assert!(
            (0.0..30.0).contains(&span.bbox.y),
            "expected top-left y near the page top, got {}",
            span.bbox.y
        );
        assert!(span.bbox.width > 0.0);
        assert!(span.bbox.height > 0.0);

        drop(doc);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_enforces_text_character_limit() {
        let path = write_temp_real_pdf("text-limit", &synthetic_text_pdf());
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim.open_document(path.to_string_lossy().as_ref()).unwrap();
        let mut request = TextRequest::page(0);
        request.max_chars = 5;

        let err = doc.extract_text(&request).unwrap_err();
        assert_eq!(err.status, raw::pdbg_status::PDBG_ERROR_LIMIT);
        assert!(err.message.contains("character limit"));

        drop(doc);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_cancelled_text_returns_clean_error_and_keeps_doc_usable() {
        let path = write_temp_real_pdf("text-cancel", &synthetic_text_pdf());
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim.open_document(path.to_string_lossy().as_ref()).unwrap();
        let cancel = CancelToken::new().unwrap();
        cancel.cancel();

        let err = doc
            .extract_text_with_cancel_token(&TextRequest::page(0), &cancel)
            .unwrap_err();
        assert_eq!(err.status, raw::pdbg_status::PDBG_ERROR_CANCELLED);

        let summary = doc.summary().unwrap();
        assert_eq!(summary.page_count, 1);
        let text = doc.extract_text(&TextRequest::page(0)).unwrap();
        assert!(text.spans.iter().any(|span| span.text.contains("Hello M3")));

        drop(doc);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_cancelled_render_returns_clean_error_and_keeps_doc_usable() {
        let fixture = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/synthetic/minimal.pdf"
        );
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim.open_document(fixture).unwrap();
        let cancel = CancelToken::new().unwrap();
        cancel.cancel();

        let err = doc
            .render_page_with_cancel_token(&RenderRequest::page(0), &cancel)
            .unwrap_err();
        assert_eq!(err.status, raw::pdbg_status::PDBG_ERROR_CANCELLED);
        assert!(err.message.contains("cancelled"));

        let summary = doc.summary().unwrap();
        assert_eq!(summary.page_count, 1);
        let render = doc.render_page(&RenderRequest::page(0)).unwrap();
        assert_eq!((render.width, render.height), (72, 72));
    }

    #[cfg(feature = "real-mupdf")]
    #[test]
    fn real_mupdf_shim_traverses_cloned_contexts_concurrently() {
        let fixture = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/synthetic/minimal.pdf"
        );
        let shim = RealMuPdfShim::new().unwrap();
        let mut docs = Vec::new();
        for _ in 0..8 {
            docs.push(shim.open_document(fixture).unwrap());
        }

        let barrier = Arc::new(std::sync::Barrier::new(docs.len()));
        let workers = docs
            .into_iter()
            .map(|mut doc| {
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    let summary = doc.summary().unwrap();
                    assert_eq!(summary.page_count, 1);

                    let root = NodeId::DocumentRoot {
                        doc: summary.doc.clone(),
                    };
                    let range = ChildRange {
                        offset: 0,
                        limit: 16,
                    };
                    let children = doc
                        .children(&root, range, ChildContainer::Dictionary)
                        .unwrap();
                    assert_eq!(children.total, Some(4));

                    let trailer = doc.object_detail(&children.items[0].id, range).unwrap();
                    assert_eq!(trailer.kind, ObjectKind::Trailer);
                })
            })
            .collect::<Vec<_>>();

        for worker in workers {
            worker.join().unwrap();
        }
    }

    #[cfg(all(feature = "real-mupdf", unix))]
    #[test]
    fn real_mupdf_shim_opens_fd_without_consuming_caller_fd() {
        use std::os::fd::AsFd;

        let fixture = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/synthetic/minimal.pdf"
        );
        let file = std::fs::File::open(fixture).unwrap();
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim
            .open_document_fd(file.as_fd(), "minimal-fd.pdf", &SafeModeConfig::default())
            .unwrap();
        let summary = doc.summary().unwrap();
        assert_eq!(summary.file_path, "minimal-fd.pdf");
        assert_eq!(summary.page_count, 1);
        drop(doc);
        assert!(file.metadata().is_ok());
    }

    #[cfg(all(unix, feature = "fake"))]
    fn temp_pdf_file() -> (std::path::PathBuf, std::fs::File) {
        use std::fs::OpenOptions;
        use std::io::{Seek, SeekFrom, Write};
        use std::sync::atomic::{AtomicU64, Ordering};

        static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);
        let sequence = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "pdbg-open-fd-{}-{}-{}.pdf",
            std::process::id(),
            sequence,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();

        file.write_all(b"%PDF fake").unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        (path, file)
    }

    #[cfg(all(unix, feature = "fake"))]
    fn fd_file_identity(fd: i32) -> Option<(u64, u64)> {
        use std::os::unix::fs::MetadataExt;

        let candidates = [
            std::path::PathBuf::from(format!("/proc/self/fd/{fd}")),
            std::path::PathBuf::from(format!("/dev/fd/{fd}")),
        ];
        candidates
            .iter()
            .find_map(|path| std::fs::metadata(path).ok())
            .map(|metadata| (metadata.dev(), metadata.ino()))
    }
}
