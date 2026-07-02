//! Tracked background work (BE-0011): spawn fire-and-forget tasks that are **awaited on shutdown**,
//! so async work isn't silently dropped on deploy. Use this instead of a bare `tokio::spawn`. Lives
//! in `AppState` (clone freely); `main` drains it after HTTP, before exit.

use std::future::Future;
use tokio_util::task::TaskTracker;

/// Cheap-to-clone handle to the process-wide task tracker.
#[derive(Clone, Default)]
pub struct Tasks {
    tracker: TaskTracker,
}

impl Tasks {
    pub fn new() -> Self {
        Tasks {
            tracker: TaskTracker::new(),
        }
    }

    /// Spawn tracked background work. It outlives the request but is awaited at shutdown.
    pub fn spawn<F>(&self, fut: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.tracker.spawn(fut);
    }

    /// Stop accepting new tasks and wait for in-flight ones. Call once, at shutdown (bound it with a
    /// `tokio::time::timeout` so a stuck task can't hang the process).
    pub async fn shutdown(&self) {
        self.tracker.close();
        self.tracker.wait().await;
    }
}
