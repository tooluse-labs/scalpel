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

/// A keyed background task, optionally backed by a cancel token.
///
/// `key` identifies the request the job answers; poll sites compare it to the
/// currently desired key to discard stale results. Dropping the job cancels
/// its token when present, so replacing or clearing a cancellable job slot
/// asks the worker to stop.
pub(crate) struct BackgroundJob<K, T> {
    key: K,
    /// `None` for uncancellable jobs and for jobs whose token could not be
    /// created (those deliver their failure through the channel instead).
    cancel: Option<Arc<CancelToken>>,
    receiver: mpsc::Receiver<JobOutput<K, T>>,
}

pub(crate) struct JobOutput<K, T> {
    pub(crate) key: K,
    pub(crate) result: Result<T, String>,
}

pub(crate) enum JobPoll<K, T> {
    Pending,
    Finished(JobOutput<K, T>),
    Disconnected(K),
}

impl<K, T> BackgroundJob<K, T>
where
    K: Clone + Send + 'static,
    T: Send + 'static,
{
    pub(crate) fn spawn<F>(key: K, worker: F) -> Self
    where
        F: FnOnce(&CancelToken) -> Result<T, String> + Send + 'static,
    {
        let (sender, receiver) = mpsc::channel();
        let cancel = match CancelToken::new() {
            Ok(cancel) => Arc::new(cancel),
            Err(err) => {
                // Token allocation failed; report it like a worker error so
                // the poll site's normal error handling (and logging) runs.
                let _ = sender.send(JobOutput {
                    key: key.clone(),
                    result: Err(err.message),
                });
                return Self {
                    key,
                    cancel: None,
                    receiver,
                };
            }
        };
        let worker_cancel = Arc::clone(&cancel);
        let worker_key = key.clone();
        thread::spawn(move || {
            let result = worker(worker_cancel.as_ref());
            let _ = sender.send(JobOutput {
                key: worker_key,
                result,
            });
        });
        Self {
            key,
            cancel: Some(cancel),
            receiver,
        }
    }

    /// For work that cannot be interrupted mid-flight (e.g. opening a
    /// document); no cancel token is allocated.
    pub(crate) fn spawn_uncancellable<F>(key: K, worker: F) -> Self
    where
        F: FnOnce() -> Result<T, String> + Send + 'static,
    {
        let (sender, receiver) = mpsc::channel();
        let worker_key = key.clone();
        thread::spawn(move || {
            let _ = sender.send(JobOutput {
                key: worker_key,
                result: worker(),
            });
        });
        Self {
            key,
            cancel: None,
            receiver,
        }
    }

    pub(crate) fn key(&self) -> &K {
        &self.key
    }

    pub(crate) fn poll(&self) -> JobPoll<K, T> {
        match self.receiver.try_recv() {
            Ok(output) => JobPoll::Finished(output),
            Err(mpsc::TryRecvError::Empty) => JobPoll::Pending,
            Err(mpsc::TryRecvError::Disconnected) => JobPoll::Disconnected(self.key.clone()),
        }
    }
}

impl<K, T> Drop for BackgroundJob<K, T> {
    fn drop(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
        }
    }
}

pub(crate) type RealRenderJob = BackgroundJob<RealRenderKey, RenderResult>;
pub(crate) type RealStreamJob = BackgroundJob<RealStreamKey, StreamChunk>;
pub(crate) type RealTextSearchJob = BackgroundJob<String, (TextSearchResult, TextPageCache)>;
pub(crate) type RealObjectSearchJob = BackgroundJob<String, ObjectSearchResult>;
pub(crate) type OpenPdfJob = BackgroundJob<String, OpenPdfJobResult>;
pub(crate) type ImagePreviewJob = BackgroundJob<ObjectId, ImagePreview>;
pub(crate) type StreamExportJob = BackgroundJob<StreamExportKey, StreamSaveOutcome>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StreamExportKey {
    pub(crate) object: ObjectId,
    pub(crate) mode: StreamMode,
    pub(crate) path: String,
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
