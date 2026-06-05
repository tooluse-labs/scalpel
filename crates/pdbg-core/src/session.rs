use crate::{DocumentSummary, ShimDocument, ShimError};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub struct FakeSharedStore {
    inner: Arc<Mutex<FakeSharedStoreState>>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FakeSharedStoreSnapshot {
    pub root_lock_contexts: u64,
    pub documents_opened: u64,
    pub tasks_entered: u64,
    pub tasks_completed: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TaskQueueStats {
    pub submitted: u64,
    pub completed: u64,
}

#[derive(Clone, Copy, Debug, Default)]
struct FakeSharedStoreState {
    root_lock_contexts: u64,
    documents_opened: u64,
    tasks_entered: u64,
    tasks_completed: u64,
}

impl FakeSharedStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn record_root_lock_context(&self) {
        self.with_state(|state| state.root_lock_contexts += 1);
    }

    pub(crate) fn record_document_open(&self) {
        self.with_state(|state| state.documents_opened += 1);
    }

    pub(crate) fn record_task_entered(&self) {
        self.with_state(|state| state.tasks_entered += 1);
    }

    pub(crate) fn record_task_completed(&self) {
        self.with_state(|state| state.tasks_completed += 1);
    }

    pub fn snapshot(&self) -> FakeSharedStoreSnapshot {
        let state = self.inner.lock().expect("fake shared store mutex poisoned");
        FakeSharedStoreSnapshot {
            root_lock_contexts: state.root_lock_contexts,
            documents_opened: state.documents_opened,
            tasks_entered: state.tasks_entered,
            tasks_completed: state.tasks_completed,
        }
    }

    fn with_state(&self, update: impl FnOnce(&mut FakeSharedStoreState)) {
        let mut state = self.inner.lock().expect("fake shared store mutex poisoned");
        update(&mut state);
    }
}

pub struct DocumentSession<D>
where
    D: ShimDocument + Send + 'static,
{
    inner: Arc<DocumentSessionInner<D>>,
}

struct DocumentSessionInner<D>
where
    D: ShimDocument + Send + 'static,
{
    document: Mutex<D>,
    summary_cache: Mutex<Option<DocumentSummary>>,
    shared_store: FakeSharedStore,
    submitted_tasks: AtomicU64,
    completed_tasks: AtomicU64,
}

impl<D> Clone for DocumentSession<D>
where
    D: ShimDocument + Send + 'static,
{
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<D> DocumentSession<D>
where
    D: ShimDocument + Send + 'static,
{
    pub fn new(document: D) -> Self {
        Self::with_shared_store(document, FakeSharedStore::new())
    }

    pub fn with_shared_store(document: D, shared_store: FakeSharedStore) -> Self {
        Self {
            inner: Arc::new(DocumentSessionInner {
                document: Mutex::new(document),
                summary_cache: Mutex::new(None),
                shared_store,
                submitted_tasks: AtomicU64::new(0),
                completed_tasks: AtomicU64::new(0),
            }),
        }
    }

    pub fn run_task<T>(
        &self,
        task: impl FnOnce(&mut D) -> Result<T, ShimError>,
    ) -> Result<T, ShimError> {
        self.inner.submitted_tasks.fetch_add(1, Ordering::SeqCst);
        self.inner.shared_store.record_task_entered();

        let result = {
            let mut document = self
                .inner
                .document
                .lock()
                .expect("document session mutex poisoned");
            task(&mut document)
        };

        self.inner.completed_tasks.fetch_add(1, Ordering::SeqCst);
        self.inner.shared_store.record_task_completed();
        result
    }

    pub fn summary(&self) -> Result<DocumentSummary, ShimError> {
        if let Some(summary) = self
            .inner
            .summary_cache
            .lock()
            .expect("summary cache mutex poisoned")
            .clone()
        {
            return Ok(summary);
        }

        let summary = self.run_task(|document| document.summary())?;
        *self
            .inner
            .summary_cache
            .lock()
            .expect("summary cache mutex poisoned") = Some(summary.clone());
        Ok(summary)
    }

    pub fn task_queue_stats(&self) -> TaskQueueStats {
        TaskQueueStats {
            submitted: self.inner.submitted_tasks.load(Ordering::SeqCst),
            completed: self.inner.completed_tasks.load(Ordering::SeqCst),
        }
    }

    pub fn shared_store_snapshot(&self) -> FakeSharedStoreSnapshot {
        self.inner.shared_store.snapshot()
    }
}

#[cfg(all(test, feature = "fake"))]
mod tests {
    use super::*;
    use crate::{FakeShim, Shim};

    #[test]
    fn fake_shared_store_records_root_context_before_open() {
        let shim = FakeShim::new().unwrap();
        assert_eq!(
            shim.shared_store_snapshot(),
            FakeSharedStoreSnapshot {
                root_lock_contexts: 1,
                documents_opened: 0,
                tasks_entered: 0,
                tasks_completed: 0,
            }
        );

        let _doc = shim.open_document("fake.pdf").unwrap();
        assert_eq!(shim.shared_store_snapshot().documents_opened, 1);
    }

    #[test]
    fn document_session_serializes_tasks_and_caches_owned_summary() {
        let shim = FakeShim::new().unwrap();
        let doc = shim.open_document("fake.pdf").unwrap();
        let session = DocumentSession::with_shared_store(doc, shim.shared_store());

        let first = session.summary().unwrap();
        let second = session.summary().unwrap();

        assert_eq!(first.doc, second.doc);
        assert_eq!(
            session.task_queue_stats(),
            TaskQueueStats {
                submitted: 1,
                completed: 1,
            }
        );
        assert_eq!(session.shared_store_snapshot().tasks_entered, 1);
        assert_eq!(session.shared_store_snapshot().tasks_completed, 1);
    }

    #[test]
    fn multiple_sessions_account_shared_store_from_worker_threads() {
        let shim = FakeShim::new().unwrap();
        let shared_store = shim.shared_store();
        let sessions = [
            DocumentSession::with_shared_store(
                shim.open_document("fake-a.pdf").unwrap(),
                shared_store.clone(),
            ),
            DocumentSession::with_shared_store(
                shim.open_document("fake-b.pdf").unwrap(),
                shared_store.clone(),
            ),
        ];
        let worker_count = 6;
        let tasks_per_worker = 8;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(worker_count));
        let mut workers = Vec::new();

        for worker_index in 0..worker_count {
            let session = sessions[worker_index % sessions.len()].clone();
            let barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                for _ in 0..tasks_per_worker {
                    let summary = session.run_task(|document| document.summary()).unwrap();
                    assert_eq!(summary.file_path, "fake.pdf");
                }
            }));
        }

        for worker in workers {
            worker.join().unwrap();
        }

        let expected_tasks = (worker_count * tasks_per_worker) as u64;
        let snapshot = shared_store.snapshot();
        assert_eq!(snapshot.root_lock_contexts, 1);
        assert_eq!(snapshot.documents_opened, 2);
        assert_eq!(snapshot.tasks_entered, expected_tasks);
        assert_eq!(snapshot.tasks_completed, expected_tasks);
        assert_eq!(
            sessions
                .iter()
                .map(|session| session.task_queue_stats().submitted)
                .sum::<u64>(),
            expected_tasks
        );
    }

    #[test]
    fn cloned_document_session_serializes_concurrent_tasks() {
        let shim = FakeShim::new().unwrap();
        let session = DocumentSession::with_shared_store(
            shim.open_document("fake.pdf").unwrap(),
            shim.shared_store(),
        );
        let worker_count = 8;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(worker_count));
        let active_tasks = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let max_active_tasks = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut workers = Vec::new();

        for _ in 0..worker_count {
            let session = session.clone();
            let barrier = barrier.clone();
            let active_tasks = active_tasks.clone();
            let max_active_tasks = max_active_tasks.clone();
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                session
                    .run_task(|document| {
                        let active =
                            active_tasks.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                        max_active_tasks.fetch_max(active, std::sync::atomic::Ordering::SeqCst);
                        std::thread::sleep(std::time::Duration::from_millis(2));
                        let summary = document.summary();
                        active_tasks.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                        summary
                    })
                    .unwrap();
            }));
        }

        for worker in workers {
            worker.join().unwrap();
        }

        assert_eq!(
            max_active_tasks.load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        assert_eq!(
            session.task_queue_stats(),
            TaskQueueStats {
                submitted: worker_count as u64,
                completed: worker_count as u64,
            }
        );
    }
}
