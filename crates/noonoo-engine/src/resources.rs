//! Subresources: `<img>` sources, loaded through the host's
//! [`Loader`](noonoo_net::Loader) and kept for layout and paint.

use std::collections::{HashMap, HashSet};

use noonoo_layout::{Image, Images};
use noonoo_net::{Completions, Loader, RequestId};

use crate::Engine;

/// The loader, what was asked of it, and what arrived.
#[derive(Default)]
pub(crate) struct Resources {
    loader: Option<Box<dyn Loader + Send + Sync>>,
    base_url: Option<String>,
    completions: Completions,
    /// Each source as written, once requested (loaded, failed or pending).
    requested: HashSet<String>,
    /// Pending requests → the source as written.
    pending: HashMap<RequestId, String>,
    images: Images,
}

impl Engine {
    /// The URL the document's references resolve against (its own URL).
    pub fn set_base_url(&mut self, url: impl Into<String>) {
        self.resources.base_url = Some(url.into());
    }

    /// The URL references resolve against, if one was set.
    pub fn base_url(&self) -> Option<&str> {
        self.resources.base_url.as_deref()
    }

    /// Load subresources through `loader` from now on. Without one, nothing
    /// is loaded: an engine reaches the filesystem or the network only
    /// when its host says so.
    pub fn set_loader(&mut self, loader: impl Loader + Send + Sync + 'static) {
        self.resources.loader = Some(Box::new(loader));
    }

    /// The queue loads finish into: set a waker on it to hear when to call
    /// [`Engine::poll_resources`].
    pub fn completions(&self) -> &Completions {
        &self.resources.completions
    }

    /// Start loading each `<img src>` not yet requested, resolved against
    /// the base URL; returns how many loads started. Call it after parsing
    /// and after DOM edits that may add images.
    pub fn load_resources(&mut self) -> usize {
        let Some(loader) = &self.resources.loader else {
            return 0;
        };
        let mut started = 0;
        for element in self.dom.get_elements_by_tag_name("img") {
            let Some(src) = self.dom.element(element).and_then(|e| e.attr("src")) else {
                continue;
            };
            let src = src.trim();
            if src.is_empty() || !self.resources.requested.insert(src.to_owned()) {
                continue;
            }
            let url = match &self.resources.base_url {
                Some(base) => noonoo_net::resolve(base, src),
                None => src.to_owned(),
            };
            let (request, completion) = self.resources.completions.request(url);
            self.resources.pending.insert(request.id, src.to_owned());
            loader.start(request, completion);
            started += 1;
        }
        started
    }

    /// Take in the loads that finished; true when an image arrived, so
    /// layout changed. A load that failed or isn't an image keeps its
    /// element's `alt` text, and isn't retried.
    pub fn poll_resources(&mut self) -> bool {
        let mut changed = false;
        for loaded in self.resources.completions.drain() {
            let Some(src) = self.resources.pending.remove(&loaded.id) else {
                continue;
            };
            if let Some(image) = loaded.result.ok().and_then(Image::from_bytes) {
                self.resources.images.insert(src, image);
                changed = true;
            }
        }
        changed
    }

    /// The images loaded so far, by `src` as written.
    pub fn images(&self) -> &Images {
        &self.resources.images
    }
}
