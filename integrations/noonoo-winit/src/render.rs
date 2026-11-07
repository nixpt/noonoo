//! The GPU surface: a wgpu surface driven by Vello.
//!
//! One [`Renderer`] owns the surface, the device, and Vello's renderer. Each
//! [`Renderer::draw`] composes the host's layers — one `noonoo_vello` scene per
//! layer, offset by the layer's rectangle and clipped to it — and presents.
//!
//! The surface is created from the window's raw handles
//! ([`SurfaceTargetUnsafe::from_window`]) so it can live for `'static` while the
//! window keeps ownership. The caller must drop the renderer before the window
//! (the `App` struct orders its fields so, and drops it on suspend).

use noonoo_shell::Host;
use vello::kurbo::{Affine, Rect as KRect};
use vello::peniko::{BlendMode, Color, Fill};
use vello::{AaConfig, AaSupport, RenderParams, Renderer as VelloRenderer, RendererOptions, Scene};
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
        // Prefer an sRGB format so Vello's linear output is gamma-corrected.
        let format = capabilities
            .formats
            .iter()
            .find(|format| {
                matches!(
                    format,
                    wgpu::TextureFormat::Bgra8UnormSrgb | wgpu::TextureFormat::Rgba8UnormSrgb
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

        Ok(Self {
            device,
            queue,
            surface,
            config,
            vello,
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
    }

    /// The configured surface size, in physical pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// Compose the host's layers and present a frame.
    pub fn draw(&mut self, host: &Host) -> Result<(), Error> {
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
            scene.push_layer(
                Fill::NonZero,
                BlendMode::default(),
                1.0,
                Affine::IDENTITY,
                &clip,
            );
            scene.append(&noonoo_vello::scene(layer.commands()), Some(offset));
            scene.pop_layer();
        }

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
            .render_to_texture(&self.device, &self.queue, &scene, &view, &params)?;
        frame.present();
        Ok(())
    }
}
