use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RealRenderKey {
    pub(crate) page_index: usize,
    pub(crate) zoom_bits: u32,
    pub(crate) rotation_degrees: i32,
    pub(crate) max_dimension: u32,
}

impl RealRenderKey {
    pub(crate) fn new(
        page_index: usize,
        zoom: f32,
        rotation_degrees: i32,
        max_dimension: u32,
    ) -> Self {
        Self {
            page_index,
            zoom_bits: zoom.to_bits(),
            rotation_degrees,
            max_dimension,
        }
    }

    pub(crate) fn zoom(self) -> f32 {
        f32::from_bits(self.zoom_bits)
    }

    pub(crate) fn request(self) -> RenderRequest {
        let mut request = RenderRequest::page(self.page_index);
        request.zoom = self.zoom();
        request.rotation_degrees = self.rotation_degrees;
        request.max_width = self.max_dimension;
        request.max_height = self.max_dimension;
        request.max_pixels = render_max_pixels(self.max_dimension);
        request.max_output_bytes = render_max_output_bytes(self.max_dimension);
        request
    }
}

pub(crate) struct RealRenderJob {
    pub(crate) key: RealRenderKey,
    pub(crate) cancel: Arc<CancelToken>,
    pub(crate) receiver: mpsc::Receiver<RealRenderJobOutput>,
}

impl Drop for RealRenderJob {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

pub(crate) struct RealRenderJobOutput {
    pub(crate) key: RealRenderKey,
    pub(crate) result: Result<RenderResult, String>,
}

pub(crate) struct RealStreamJob {
    pub(crate) key: RealStreamKey,
    pub(crate) cancel: Arc<CancelToken>,
    pub(crate) receiver: mpsc::Receiver<RealStreamJobOutput>,
}

impl Drop for RealStreamJob {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

pub(crate) struct RealStreamJobOutput {
    pub(crate) key: RealStreamKey,
    pub(crate) result: Result<StreamChunk, String>,
}

pub(crate) struct RealTextSearchJob {
    pub(crate) query: String,
    pub(crate) cancel: Arc<CancelToken>,
    pub(crate) receiver: mpsc::Receiver<RealTextSearchJobOutput>,
}

impl Drop for RealTextSearchJob {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

pub(crate) struct RealTextSearchJobOutput {
    pub(crate) query: String,
    pub(crate) result: Result<(TextSearchResult, TextPageCache), String>,
}

pub(crate) struct RealObjectSearchJob {
    pub(crate) query: String,
    pub(crate) cancel: Arc<CancelToken>,
    pub(crate) receiver: mpsc::Receiver<RealObjectSearchJobOutput>,
}

impl Drop for RealObjectSearchJob {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

pub(crate) struct RealObjectSearchJobOutput {
    pub(crate) query: String,
    pub(crate) result: Result<ObjectSearchResult, String>,
}

pub(crate) struct OpenPdfJob {
    pub(crate) path: String,
    pub(crate) receiver: mpsc::Receiver<OpenPdfJobOutput>,
}

pub(crate) struct OpenPdfJobOutput {
    pub(crate) path: String,
    pub(crate) result: Result<OpenPdfJobResult, String>,
}

pub(crate) enum OpenPdfJobResult {
    Opened(Box<OpenedPdfModel>),
    NeedsPassword,
}

pub(crate) struct OpenedPdfModel {
    pub(crate) state: AppState,
    pub(crate) tree: TreeModel,
    pub(crate) real_detail: Option<ObjectDetail>,
    pub(crate) real_detail_error: Option<String>,
    pub(crate) real_pages: Option<ChildPage<ObjectSummary>>,
    pub(crate) real_pages_error: Option<String>,
    pub(crate) status_log: Vec<String>,
}
