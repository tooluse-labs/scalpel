use crate::dto::*;
use crate::session::FakeSharedStore;
use crate::{wire, ChildContainer, NodeTokenRegistry, SafeModeConfig};
use pdbg_shim::raw;
use std::ffi::{CStr, CString};
#[cfg(unix)]
use std::os::fd::{AsRawFd, BorrowedFd};
use std::ptr::{self, NonNull};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct ShimError {
    pub status: raw::pdbg_status,
    pub message: String,
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
    fn stream_load(
        &mut self,
        object: ObjectId,
        mode: StreamMode,
        offset: u64,
        limit: usize,
    ) -> Result<StreamChunk, ShimError>;
    fn render_page(&mut self, request: &RenderRequest) -> Result<RenderResult, ShimError>;
    fn extract_text(&mut self, request: &TextRequest) -> Result<TextPage, ShimError>;
}

pub struct FakeShim {
    ctx: Arc<PdbgContext>,
    shared_store: FakeSharedStore,
}

impl FakeShim {
    pub fn new() -> Result<Self, ShimError> {
        let shared_store = FakeSharedStore::new();
        shared_store.record_root_lock_context();
        Ok(Self {
            ctx: Arc::new(PdbgContext::new()?),
            shared_store,
        })
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
        let doc = PdbgDoc::open_path(Arc::clone(&self.ctx), path, config)?;
        self.shared_store.record_document_open();
        Ok(OpenDocument {
            doc,
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
        let doc = PdbgDoc::open_fd(Arc::clone(&self.ctx), fd, display_path, config)?;
        self.shared_store.record_document_open();
        Ok(OpenDocument {
            doc,
            registry: NodeTokenRegistry::default(),
        })
    }
}

impl Shim for FakeShim {
    type Document = OpenDocument;

    fn open_document(&self, path: &str) -> Result<Self::Document, ShimError> {
        let doc = PdbgDoc::open_path(Arc::clone(&self.ctx), path, &SafeModeConfig::default())?;
        self.shared_store.record_document_open();
        Ok(OpenDocument {
            doc,
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
            doc: PdbgDoc::open_path(Arc::clone(&self.ctx), path, config)?,
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
            doc: PdbgDoc::open_fd(Arc::clone(&self.ctx), fd, display_path, config)?,
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
    doc: PdbgDoc,
    registry: NodeTokenRegistry,
}

impl ShimDocument for OpenDocument {
    fn summary(&mut self) -> Result<DocumentSummary, ShimError> {
        self.doc.summary(&self.registry)
    }

    fn children(
        &mut self,
        parent: &NodeId,
        range: ChildRange,
        container: ChildContainer,
    ) -> Result<ChildPage, ShimError> {
        let raw_parent = self.raw_node_for(parent)?;
        self.doc
            .children(&mut self.registry, &raw_parent, parent, range, container)
    }

    fn object_detail(
        &mut self,
        node: &NodeId,
        range: ChildRange,
    ) -> Result<ObjectDetail, ShimError> {
        let raw_node = self.raw_node_for(node)?;
        self.doc
            .object_detail(&mut self.registry, &raw_node, node, range)
    }

    fn stream_load(
        &mut self,
        object: ObjectId,
        mode: StreamMode,
        offset: u64,
        limit: usize,
    ) -> Result<StreamChunk, ShimError> {
        self.doc
            .stream_load(&self.registry, object, mode, offset, limit)
    }

    fn render_page(&mut self, request: &RenderRequest) -> Result<RenderResult, ShimError> {
        self.doc.render_page(&self.registry, request)
    }

    fn extract_text(&mut self, request: &TextRequest) -> Result<TextPage, ShimError> {
        self.doc.extract_text(request)
    }
}

impl OpenDocument {
    fn raw_node_for(&self, node: &NodeId) -> Result<raw::pdbg_node_id, ShimError> {
        if let Some(raw_node) = self.registry.raw_for(node) {
            return Ok(raw_node);
        }
        raw_node_from_public(node).ok_or_else(|| ShimError {
            status: raw::pdbg_status::PDBG_ERROR_GENERIC,
            message: "node is not registered in this document session".to_string(),
        })
    }
}

struct PdbgContext {
    raw: NonNull<raw::pdbg_context>,
    open_lock: Mutex<()>,
}

// Safety: the C `pdbg_context` installs MuPDF lock callbacks before any cloned
// document context is created. The Rust root context is shared to keep that
// lock table alive; root-context open/clone entry points are serialized by
// `open_lock`, while document operations use per-document C handles.
unsafe impl Send for PdbgContext {}
unsafe impl Sync for PdbgContext {}

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
            Ok(Self {
                raw,
                open_lock: Mutex::new(()),
            })
        }
    }

    fn open_raw_document_handle(
        &self,
        path: &str,
        config: &SafeModeConfig,
    ) -> Result<NonNull<raw::pdbg_doc>, ShimError> {
        let path = CString::new(path).map_err(|_| ShimError {
            status: raw::pdbg_status::PDBG_ERROR_GENERIC,
            message: "path contains interior NUL".to_string(),
        })?;
        let options = config.to_raw_open_options();
        let _open_guard = self.open_lock.lock().expect("pdbg context mutex poisoned");

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
            Ok(raw)
        }
    }

    #[cfg(unix)]
    fn open_raw_document_fd_handle(
        &self,
        fd: BorrowedFd<'_>,
        display_path: &str,
        config: &SafeModeConfig,
    ) -> Result<NonNull<raw::pdbg_doc>, ShimError> {
        let display_path = CString::new(display_path).map_err(|_| ShimError {
            status: raw::pdbg_status::PDBG_ERROR_GENERIC,
            message: "display path contains interior NUL".to_string(),
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
                ptr::null(),
                &options,
                &mut doc,
                &mut err,
            );
            check_status(status, &err)?;
            let raw = NonNull::new(doc).ok_or_else(|| ShimError {
                status: raw::pdbg_status::PDBG_ERROR_GENERIC,
                message: "pdbg_document_open_fd returned null".to_string(),
            })?;
            Ok(raw)
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
    _ctx: Arc<PdbgContext>,
}

impl PdbgDoc {
    fn open_path(
        ctx: Arc<PdbgContext>,
        path: &str,
        config: &SafeModeConfig,
    ) -> Result<Self, ShimError> {
        let raw = ctx.open_raw_document_handle(path, config)?;
        Ok(Self { raw, _ctx: ctx })
    }

    #[cfg(unix)]
    fn open_fd(
        ctx: Arc<PdbgContext>,
        fd: BorrowedFd<'_>,
        display_path: &str,
        config: &SafeModeConfig,
    ) -> Result<Self, ShimError> {
        let raw = ctx.open_raw_document_fd_handle(fd, display_path, config)?;
        Ok(Self { raw, _ctx: ctx })
    }
}

// Safety: `PdbgDoc` may be moved to a worker thread, but it is not `Sync`.
// The C shim contract requires document handles to remain valid after open
// without borrowing unsynchronized root-context state. Concurrent access must
// go through `DocumentSession`, which serializes mutable document operations.
unsafe impl Send for PdbgDoc {}

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

    fn stream_load(
        &self,
        registry: &NodeTokenRegistry,
        object: ObjectId,
        mode: StreamMode,
        offset: u64,
        limit: usize,
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
                ptr::null_mut(),
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
        unsafe {
            let options = raw_render_options(request);
            let mut image = ptr::null_mut();
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_page_render(
                self.raw.as_ptr(),
                page_index_to_u32(request.page_index)?,
                &options,
                ptr::null_mut(),
                &mut image,
                &mut err,
            );
            check_status(status, &err)?;
            let image = PdbgImage::new(image)?;
            image.to_render_result(registry, request.page_index)
        }
    }

    fn extract_text(&self, request: &TextRequest) -> Result<TextPage, ShimError> {
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
                ptr::null_mut(),
                &mut text,
                &mut err,
            );
            check_status(status, &err)?;
            let text = PdbgTextPage::new(text)?;
            Ok(text.to_text_page(request.page_index))
        }
    }
}

impl Drop for PdbgDoc {
    fn drop(&mut self) {
        unsafe { raw::pdbg_document_drop(self.raw.as_ptr()) }
    }
}

struct PdbgNodeList {
    raw: NonNull<raw::pdbg_node_list>,
}

impl PdbgNodeList {
    fn new(raw: *mut raw::pdbg_node_list) -> Result<Self, ShimError> {
        let raw = NonNull::new(raw).ok_or_else(|| ShimError {
            status: raw::pdbg_status::PDBG_ERROR_GENERIC,
            message: "node list accessor returned null".to_string(),
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

impl Drop for PdbgNodeList {
    fn drop(&mut self) {
        unsafe { raw::pdbg_node_list_drop(self.raw.as_ptr()) }
    }
}

struct PdbgBuffer {
    raw: NonNull<raw::pdbg_buffer>,
}

impl PdbgBuffer {
    fn new(raw: *mut raw::pdbg_buffer) -> Result<Self, ShimError> {
        let raw = NonNull::new(raw).ok_or_else(|| ShimError {
            status: raw::pdbg_status::PDBG_ERROR_GENERIC,
            message: "buffer accessor returned null".to_string(),
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

impl Drop for PdbgBuffer {
    fn drop(&mut self) {
        unsafe { raw::pdbg_buffer_drop(self.raw.as_ptr()) }
    }
}

struct PdbgImage {
    raw: NonNull<raw::pdbg_image>,
}

impl PdbgImage {
    fn new(raw: *mut raw::pdbg_image) -> Result<Self, ShimError> {
        let raw = NonNull::new(raw).ok_or_else(|| ShimError {
            status: raw::pdbg_status::PDBG_ERROR_GENERIC,
            message: "image accessor returned null".to_string(),
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
        let byte_len = stride
            .checked_mul(height as usize)
            .ok_or_else(|| ShimError {
                status: raw::pdbg_status::PDBG_ERROR_LIMIT,
                message: "render output byte size overflow".to_string(),
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
}

impl Drop for PdbgImage {
    fn drop(&mut self) {
        unsafe { raw::pdbg_image_drop(self.raw.as_ptr()) }
    }
}

struct PdbgTextPage {
    raw: NonNull<raw::pdbg_text_page>,
}

impl PdbgTextPage {
    fn new(raw: *mut raw::pdbg_text_page) -> Result<Self, ShimError> {
        let raw = NonNull::new(raw).ok_or_else(|| ShimError {
            status: raw::pdbg_status::PDBG_ERROR_GENERIC,
            message: "text page accessor returned null".to_string(),
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

impl Drop for PdbgTextPage {
    fn drop(&mut self) {
        unsafe { raw::pdbg_text_page_drop(self.raw.as_ptr()) }
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

unsafe fn convert_document_summary(
    out: &raw::pdbg_document_summary_out,
    registry: &NodeTokenRegistry,
) -> DocumentSummary {
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
        diagnostics: wire::diagnostic_list(out.diagnostics, &|node| registry.resolve_node(node)),
    }
}

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

fn page_index_to_u32(page_index: usize) -> Result<u32, ShimError> {
    page_index.try_into().map_err(|_| ShimError {
        status: raw::pdbg_status::PDBG_ERROR_LIMIT,
        message: "page index exceeds C ABI range".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
            owned_fd = unsafe { raw::pdbg_test_document_owned_fd(doc.doc.raw.as_ptr()) };
            assert!(owned_fd >= 0);
            assert_ne!(owned_fd, file.as_raw_fd());
            assert_eq!(unsafe { raw::pdbg_test_fd_is_open(owned_fd) }, 1);
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
            "4 0 obj\n<< /Length 11 /Filter /ASCIIHexDecode >>\nstream\n48656c6c6f>\nendstream\nendobj\n",
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
    fn real_mupdf_shim_reports_repair_warning_on_summary() {
        let mut bytes = include_bytes!("../../../fixtures/synthetic/minimal.pdf").to_vec();
        let needle = b"startxref\n184\n";
        let pos = bytes
            .windows(needle.len())
            .position(|window| window == needle)
            .unwrap();
        bytes.splice(pos..pos + needle.len(), b"startxref\n0\n".iter().copied());

        let path = write_temp_real_pdf("repairable", &bytes);
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
    fn real_mupdf_shim_loads_raw_and_decoded_stream_chunks() {
        let path = write_temp_real_pdf("stream-load", &synthetic_ascii_hex_stream_pdf());
        let shim = RealMuPdfShim::new().unwrap();
        let mut doc = shim.open_document(path.to_string_lossy().as_ref()).unwrap();

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
