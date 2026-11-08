//! The asynchronous loading seam.
//!
//! The host owns a [`Completions`] queue and a [`Loader`]. Each
//! [`Completions::request`] makes a [`Request`] and the [`Completion`] that
//! answers it. The loader starts the request and completes it whenever and
//! from whichever thread it likes, and the host drains the finished loads on
//! its own thread. A completion dropped unanswered reports a failed load, so
//! nothing waits forever on a loader that gave up.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::{LoadError, load};

/// Identifies one request among a [`Completions`] queue's.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct RequestId(u64);

/// A resource to load: an absolute URL (see [`crate::resolve`]).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Request {
    /// Which request this is.
    pub id: RequestId,
    /// What to load.
    pub url: String,
}

/// A finished load.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Loaded {
    /// The request it answers.
    pub id: RequestId,
    /// The URL that was requested.
    pub url: String,
    /// The bytes, or why there are none.
    pub result: Result<Vec<u8>, LoadError>,
}

/// Starts loads. A loader may answer at once (as [`FileLoader`] does) or
/// later, from another thread.
pub trait Loader {
    /// Start loading `request`; answer through `completion`.
    fn start(&self, request: Request, completion: Completion);
}

type Waker = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
struct Shared {
    finished: Mutex<Vec<Loaded>>,
    waker: Mutex<Option<Waker>>,
    next: AtomicU64,
    pending: AtomicUsize,
}

/// The host's queue of finished loads.
#[derive(Clone, Default)]
pub struct Completions {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for Completions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Completions")
            .field("pending", &self.pending())
            .finish_non_exhaustive()
    }
}

impl Completions {
    /// An empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// A request for `url` and the completion that answers it.
    pub fn request(&self, url: String) -> (Request, Completion) {
        let id = RequestId(self.shared.next.fetch_add(1, Ordering::Relaxed));
        self.shared.pending.fetch_add(1, Ordering::SeqCst);
        let completion = Completion {
            id,
            url: url.clone(),
            shared: Some(self.shared.clone()),
        };
        (Request { id, url }, completion)
    }

    /// Call `waker` after each load finishes, from the thread that finished
    /// it: a host wakes its event loop here, then drains on its own thread.
    pub fn set_waker(&self, waker: impl Fn() + Send + Sync + 'static) {
        *lock(&self.shared.waker) = Some(Arc::new(waker));
    }

    /// The loads finished since the last drain, in the order they finished.
    pub fn drain(&self) -> Vec<Loaded> {
        std::mem::take(&mut *lock(&self.shared.finished))
    }

    /// How many requests are still unanswered.
    pub fn pending(&self) -> usize {
        self.shared.pending.load(Ordering::SeqCst)
    }
}

/// Answers one [`Request`]. It can be sent to another thread; dropping it
/// unanswered reports the load as failed.
pub struct Completion {
    id: RequestId,
    url: String,
    shared: Option<Arc<Shared>>,
}

impl std::fmt::Debug for Completion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Completion")
            .field("id", &self.id)
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

impl Completion {
    /// Answer the request with `result`.
    pub fn complete(mut self, result: Result<Vec<u8>, LoadError>) {
        self.finish(result);
    }

    fn finish(&mut self, result: Result<Vec<u8>, LoadError>) {
        let Some(shared) = self.shared.take() else {
            return;
        };
        lock(&shared.finished).push(Loaded {
            id: self.id,
            url: std::mem::take(&mut self.url),
            result,
        });
        shared.pending.fetch_sub(1, Ordering::SeqCst);
        let waker = lock(&shared.waker).clone();
        if let Some(waker) = waker {
            waker();
        }
    }
}

impl Drop for Completion {
    fn drop(&mut self) {
        self.finish(Err(LoadError::Io(
            "the loader dropped the request".to_owned(),
        )));
    }
}

/// A poisoned lock still holds a usable queue: a panicking waker must not
/// stop every later load.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Loads `file:` and `data:` URLs (and bare paths) synchronously, answering
/// before [`Loader::start`] returns; anything else fails as unsupported.
#[derive(Clone, Copy, Default, Debug)]
pub struct FileLoader;

impl Loader for FileLoader {
    fn start(&self, request: Request, completion: Completion) {
        completion.complete(load(&request.url));
    }
}
