//! The GPU surface: a wgpu surface driven by Vello.
//!
//! One [`Renderer`] owns the surface, the device, and Vello's renderer. Each
//! [`Renderer::draw`] composes the host's layers — one `noonoo_vello` scene per
//! layer, offset by the layer's rectangle and clipped to it — plus the
//! embedder's [`crate::frame`] hooks, and presents.
//!
//! The surface is created from the window's raw handles
//! ([`SurfaceTargetUnsafe::from_window`]) so it can live for `'static` while the
//! window keeps ownership. The caller must drop the renderer before the window
//! (the `App` struct orders its fields so, and drops it on suspend).

use noonoo_shell::Host;

use crate::frame::{self, FrameHooks, TextureOverlay, TexturePaint};
use vello::peniko::{Color, ImageData};
use vello::{AaConfig, AaSupport, RenderParams, Renderer as VelloRenderer, RendererOptions};
use wgpu::{
    DeviceDescriptor, Instance, InstanceDescriptor, PowerPreference, PresentMode,
    RequestAdapterOptions, SurfaceConfiguration, SurfaceTargetUnsafe, TextureUsages,
};
use winit::window::Window;

/// A surface, device, or Vello failure.
#[derive(Debug)]
pub enum Error {
    /// A window handle could not be read, or wgpu refused an adapter, device,
    /// or surface.
    Wgpu(String),
    /// Vello failed to build or to render a frame.
    Vello(vello::Error),
    /// The swap chain errored; the frame was skipped.
    Surface(wgpu::SurfaceError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wgpu(message) => write!(f, "wgpu: {message}"),
            Self::Vello(error) => write!(f, "vello: {error}"),
            Self::Surface(error) => write!(f, "surface: {error}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<wgpu::CreateSurfaceError> for Error {
    fn from(error: wgpu::CreateSurfaceError) -> Self {
        Self::Wgpu(error.to_string())
    }
}

impl From<wgpu::RequestAdapterError> for Error {
    fn from(error: wgpu::RequestAdapterError) -> Self {
        Self::Wgpu(error.to_string())
    }
}

impl From<wgpu::RequestDeviceError> for Error {
    fn from(error: wgpu::RequestDeviceError) -> Self {
        Self::Wgpu(error.to_string())
    }
}

impl From<vello::Error> for Error {
    fn from(error: vello::Error) -> Self {
        Self::Vello(error)
    }
}

impl From<wgpu::SurfaceError> for Error {
    fn from(error: wgpu::SurfaceError) -> Self {
        Self::Surface(error)
    }
}

/// A wgpu surface driven by Vello.
///
/// Field order matters for drop: the surface must drop before the window it
/// was created from, so `App` keeps the renderer *before* the window.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: SurfaceConfiguration,
    vello: VelloRenderer,
    /// Vello renders into this `Rgba8Unorm` storage texture: it can't write
    /// a surface texture directly (wrong format, and no storage usage).
    target: wgpu::TextureView,
    /// Copies `target` onto the surface each frame.
    blitter: wgpu::util::TextureBlitter,
    /// The texture overlay last registered with `vello`, and its image.
    registered: Option<(wgpu::Texture, ImageData)>,
}

/// The texture Vello renders into, sized to the surface.
fn render_target(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("noonoo-winit render target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING,
            format: wgpu::TextureFormat::Rgba8Unorm,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

impl Renderer {
    /// Create the surface, adapter, device, and Vello renderer for `window`.
    ///
    /// # Safety (in the one `unsafe` below)
    ///
    /// `SurfaceTargetUnsafe::from_window` copies the window's raw handles so
    /// the surface can live for `'static` while the window keeps ownership.
    /// The caller must keep `window` alive for as long as the renderer — the
    /// `App` struct orders its fields so the renderer drops first.
    #[allow(unsafe_code)]
    pub async fn new(window: &dyn Window) -> Result<Self, Error> {
        let instance = Instance::new(&InstanceDescriptor::default());
        let target = unsafe { SurfaceTargetUnsafe::from_window(&window) }
            .map_err(|error| Error::Wgpu(error.to_string()))?;
        let surface = unsafe { instance.create_surface_unsafe(target) }?;

        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await?;
        let (device, queue) = adapter.request_device(&DeviceDescriptor::default()).await?;

        let capabilities = surface.get_capabilities(&adapter);
        // Vello's output is already sRGB-encoded, so the surface must not be
        // an `*Srgb` format, or the blit would encode it a second time and
        // wash the colours out. This is the format vello's own util picks.
        let format = capabilities
            .formats
            .iter()
            .find(|format| {
                matches!(
                    format,
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
                )
            })
            .copied()
            .unwrap_or_else(|| capabilities.formats[0]);
        let size = window.surface_size();
        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: PresentMode::Fifo,
            alpha_mode: capabilities.alpha_modes[0],
            view_formats: Vec::new(),
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let vello = VelloRenderer::new(
            &device,
            RendererOptions {
                use_cpu: false,
                antialiasing_support: AaSupport::all(),
                ..RendererOptions::default()
            },
        )?;

        let target = render_target(&device, config.width, config.height);
        let blitter = wgpu::util::TextureBlitter::new(&device, config.format);

        Ok(Self {
            device,
            queue,
            surface,
            config,
            vello,
            target,
            blitter,
            registered: None,
        })
    }

    /// Reconfigure the surface for a new (physical) size.
    pub fn resize(&mut self, width: u32, height: u32) {
        let (width, height) = (width.max(1), height.max(1));
        if self.config.width == width && self.config.height == height {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.target = render_target(&self.device, width, height);
    }

    /// The configured surface size, in physical pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// The device the renderer draws with.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// The queue the renderer submits to.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// The image Vello knows `overlay`'s texture by, registering it when it
    /// is new. `register_texture` allocates a fresh slot on every call, so
    /// re-registering an unchanged texture each frame would leak one.
    fn register(&mut self, overlay: &TextureOverlay) -> ImageData {
        if let Some((texture, image)) = &self.registered
            && *texture == overlay.texture
        {
            return image.clone();
        }
        if let Some((_, old)) = self.registered.take() {
            self.vello.unregister_texture(old);
        }
        let image = self.vello.register_texture(overlay.texture.clone());
        self.registered = Some((overlay.texture.clone(), image.clone()));
        image
    }

    /// Compose a frame — the host's layers, then `hooks`' overlays and
    /// effects (see [`crate::frame`]) — and present it.
    ///
    /// The host lays out in logical pixels; `scale` (the window's scale
    /// factor) maps them onto the physical surface.
    pub fn draw(&mut self, host: &Host, scale: f64, hooks: &FrameHooks) -> Result<(), Error> {
        let texture = match hooks.texture() {
            Some(overlay) => Some(TexturePaint {
                image: self.register(&overlay),
                transform: overlay.transform,
                clip: overlay.clip,
            }),
            None => {
                if let Some((_, old)) = self.registered.take() {
                    self.vello.unregister_texture(old);
                }
                None
            }
        };
        let scene = frame::compose(host, scale, self.size(), hooks, texture.as_ref());

        let frame = self.surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let params = RenderParams {
            base_color: Color::from_rgba8(255, 255, 255, 255),
            width: self.config.width,
            height: self.config.height,
            antialiasing_method: AaConfig::Area,
        };
        self.vello
            .render_to_texture(&self.device, &self.queue, &scene, &self.target, &params)?;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("noonoo-winit blit"),
            });
        self.blitter
            .copy(&self.device, &mut encoder, &self.target, &view);
        self.queue.submit([encoder.finish()]);
        frame.present();
        Ok(())
    }
}
