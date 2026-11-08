//! Frame hooks: what an embedder adds to each frame on top of the host's
//! layers.
//!
//! A frame is composed in a fixed order:
//!
//! 1. the host's layers, each offset and clipped to its rectangle;
//! 2. the texture overlay — a texture already rendered elsewhere (e.g. page
//!    content on a worker thread), drawn as one image fill;
//! 3. scene overlays — another `vello::Scene` merged into the frame, under a
//!    transform and an optional clip (e.g. a devtools highlight, which must
//!    sit over the page it marks);
//! 4. the scene-effects hook, which may add anything to the finished scene
//!    (e.g. blur layers) just before it goes to the GPU.
//!
//! [`FrameHooks`] is a shared handle: clone it before [`crate::App::run`] and
//! set overlays from anywhere. Each setter wakes the window for a new frame.
//! It also hands out the renderer's device and queue, so content can be
//! rendered on the same device the window composites with.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use noonoo_shell::Host;
use vello::Scene;
use vello::kurbo::{Affine, Rect as KRect};
use vello::peniko::{BlendMode, Fill, ImageBrush, ImageData};

/// A scene merged into the frame over the texture overlay.
pub struct SceneOverlay {
    /// The scene, shared without copying.
    pub scene: Arc<Scene>,
    /// Where the scene goes, in physical surface pixels.
    pub transform: Affine,
    /// An optional clip, in physical surface pixels.
    pub clip: Option<KRect>,
}

/// A texture drawn as one image fill under the scene overlays.
///
/// The texture must be `Rgba8Unorm` with `COPY_SRC` usage, as
/// `vello::Renderer::register_texture` requires.
#[derive(Clone)]
pub struct TextureOverlay {
    /// The rendered texture (a cheap handle clone).
    pub texture: wgpu::Texture,
    /// Where the texture goes, in physical surface pixels.
    pub transform: Affine,
    /// An optional clip, in physical surface pixels.
    pub clip: Option<KRect>,
}

/// A texture overlay once registered with Vello: what [`compose`] draws.
pub struct TexturePaint {
    /// The image Vello registered for the texture.
    pub image: ImageData,
    /// Where the image goes, in physical surface pixels.
    pub transform: Affine,
    /// An optional clip, in physical surface pixels.
    pub clip: Option<KRect>,
}

/// The scene-effects hook: given the composed scene and the surface size.
pub type SceneEffects = Box<dyn FnMut(&mut Scene, u32, u32) + Send>;

type Waker = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
struct Inner {
    scenes: Vec<SceneOverlay>,
    texture: Option<TextureOverlay>,
    effects: Option<SceneEffects>,
    /// Set by `clear_scene_effects`, so a hook cleared while it runs stays
    /// cleared.
    effects_cleared: bool,
    gpu: Option<(wgpu::Device, wgpu::Queue)>,
    waker: Option<Waker>,
}

/// A shared handle to the overlays and effects added to every frame.
#[derive(Clone, Default)]
pub struct FrameHooks {
    inner: Arc<Mutex<Inner>>,
}

impl FrameHooks {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A panicking effects hook mustn't take the window down with it.
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Change the hooks, then wake the window for a frame that shows it.
    fn change(&self, f: impl FnOnce(&mut Inner)) {
        let waker = {
            let mut inner = self.lock();
            f(&mut inner);
            inner.waker.clone()
        };
        if let Some(wake) = waker {
            wake();
        }
    }

    /// Show `scene` over the host's layers and the texture overlay. Replaces
    /// any scene overlay.
    pub fn set_overlay_scene(&self, scene: Arc<Scene>, transform: Affine, clip: Option<KRect>) {
        self.change(|inner| {
            inner.scenes.clear();
            inner.scenes.push(SceneOverlay {
                scene,
                transform,
                clip,
            });
        });
    }

    /// Remove the scene overlays.
    pub fn clear_overlay_scenes(&self) {
        self.change(|inner| inner.scenes.clear());
    }

    /// Show `texture` over the host's layers, under the scene overlays.
    /// Replaces any texture overlay;
    /// independent of the scene overlays.
    pub fn set_overlay_texture(
        &self,
        texture: wgpu::Texture,
        transform: Affine,
        clip: Option<KRect>,
    ) {
        self.change(|inner| {
            inner.texture = Some(TextureOverlay {
                texture,
                transform,
                clip,
            });
        });
    }

    /// Remove the texture overlay.
    pub fn clear_overlay_textures(&self) {
        self.change(|inner| inner.texture = None);
    }

    /// Run `effects` on every composed frame, last. Replaces any hook.
    pub fn set_scene_effects(&self, effects: impl FnMut(&mut Scene, u32, u32) + Send + 'static) {
        self.change(|inner| {
            inner.effects = Some(Box::new(effects));
            inner.effects_cleared = false;
        });
    }

    /// Remove the scene-effects hook.
    pub fn clear_scene_effects(&self) {
        self.change(|inner| {
            inner.effects = None;
            inner.effects_cleared = true;
        });
    }

    /// The window's device, while it has a renderer.
    pub fn device(&self) -> Option<wgpu::Device> {
        self.lock().gpu.as_ref().map(|(device, _)| device.clone())
    }

    /// The window's queue, while it has a renderer.
    pub fn queue(&self) -> Option<wgpu::Queue> {
        self.lock().gpu.as_ref().map(|(_, queue)| queue.clone())
    }

    pub(crate) fn texture(&self) -> Option<TextureOverlay> {
        self.lock().texture.clone()
    }

    pub(crate) fn attach_gpu(&self, device: wgpu::Device, queue: wgpu::Queue) {
        self.lock().gpu = Some((device, queue));
    }

    pub(crate) fn detach_gpu(&self) {
        self.lock().gpu = None;
    }

    pub(crate) fn set_waker(&self, wake: impl Fn() + Send + Sync + 'static) {
        self.lock().waker = Some(Arc::new(wake));
    }
}

/// Draw `paint` into `scene`, inside a clip layer when there is one.
fn clipped(scene: &mut Scene, clip: Option<&KRect>, paint: impl FnOnce(&mut Scene)) {
    match clip {
        Some(clip) => {
            scene.push_layer(
                Fill::NonZero,
                BlendMode::default(),
                1.0,
                Affine::IDENTITY,
                clip,
            );
            paint(scene);
            scene.pop_layer();
        }
        None => paint(scene),
    }
}

/// Compose a frame: the host's layers at `scale`, then `texture`, then
/// `hooks`' scene overlays, then `hooks`' effects with `size`.
///
/// [`crate::render::Renderer::draw`] calls this with the texture overlay
/// already registered with Vello.
pub fn compose(
    host: &Host,
    scale: f64,
    size: (u32, u32),
    hooks: &FrameHooks,
    texture: Option<&TexturePaint>,
) -> Scene {
    let to_physical = Affine::scale(scale);
    let mut scene = Scene::new();
    for layer in host.layers() {
        let rect = layer.rect;
        // Each layer paints into its own rectangle: a layer's content must
        // not bleed over its neighbours.
        let clip = KRect::new(
            f64::from(rect.x),
            f64::from(rect.y),
            f64::from(rect.x + rect.width),
            f64::from(rect.y + rect.height),
        );
        let offset = Affine::translate((f64::from(rect.x), f64::from(rect.y)));
        scene.push_layer(Fill::NonZero, BlendMode::default(), 1.0, to_physical, &clip);
        scene.append(
            &noonoo_vello::scene(layer.commands()),
            Some(to_physical * offset),
        );
        scene.pop_layer();
    }

    if let Some(texture) = texture {
        let bounds = KRect::new(
            0.0,
            0.0,
            f64::from(texture.image.width),
            f64::from(texture.image.height),
        );
        let brush = ImageBrush::new(texture.image.clone());
        clipped(&mut scene, texture.clip.as_ref(), |scene| {
            scene.fill(Fill::NonZero, texture.transform, &brush, None, &bounds);
        });
    }
    let mut inner = hooks.lock();
    for overlay in &inner.scenes {
        clipped(&mut scene, overlay.clip.as_ref(), |scene| {
            scene.append(&overlay.scene, Some(overlay.transform));
        });
    }
    // The hook runs unlocked, so it may set overlays or replace itself.
    let effects = inner.effects.take();
    drop(inner);
    if let Some(mut effects) = effects {
        effects(&mut scene, size.0, size.1);
        let mut inner = hooks.lock();
        if inner.effects.is_none() && !inner.effects_cleared {
            inner.effects = Some(effects);
        }
        inner.effects_cleared = false;
    }
    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn every_setter_wakes_the_window() {
        let hooks = FrameHooks::default();
        let wakes = Arc::new(AtomicUsize::new(0));
        let count = wakes.clone();
        hooks.set_waker(move || {
            count.fetch_add(1, Ordering::Relaxed);
        });
        hooks.set_overlay_scene(Arc::new(Scene::new()), Affine::IDENTITY, None);
        hooks.clear_overlay_scenes();
        hooks.clear_overlay_textures();
        hooks.set_scene_effects(|_, _, _| {});
        hooks.clear_scene_effects();
        assert_eq!(wakes.load(Ordering::Relaxed), 5);
    }
}
