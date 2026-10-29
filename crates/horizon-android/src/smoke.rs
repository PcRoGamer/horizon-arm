//! The smoke screen: a window, a wgpu adapter, a cleared frame that breathes
//! between two colours, and in logcat what the device actually offers.
//!
//! It exists so a phone can be judged before any Mojang data is on it: if this
//! draws, the port's window, surface, driver and render thread work. If it
//! does not, the message says which of those failed.
use std::sync::Arc;

use crate::say;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    platform::android::EventLoopBuilderExtAndroid,
    window::{Window, WindowId},
};

/// Draws until the activity goes away.
pub fn run(app: android_activity::AndroidApp) -> Result<(), String> {
    let event_loop = EventLoop::builder()
        .with_android_app(app)
        .build()
        .map_err(|e| format!("event loop: {e}"))?;
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop
        .run_app(&mut Screen::default())
        .map_err(|e| format!("event loop: {e}"))
}

#[derive(Default)]
struct Screen {
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    frames: u64,
}

struct Gpu {
    surface: wgpu::Surface<'static>,
    window: Arc<Window>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
}

impl ApplicationHandler for Screen {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            // A resumed activity keeps its surface.
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Horizon")
            .with_inner_size(LogicalSize::new(960.0, 540.0));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(e) => {
                say(&format!("smoke: no window: {e}"));
                event_loop.exit();
                return;
            }
        };
        say(&format!(
            "smoke: window {}x{}",
            window.inner_size().width,
            window.inner_size().height
        ));
        match Gpu::new(window.clone()) {
            Ok(gpu) => self.gpu = Some(gpu),
            Err(e) => {
                say(&format!("smoke: {e}"));
                event_loop.exit();
                return;
            }
        }
        window.request_redraw();
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                self.frames += 1;
                if let Some(gpu) = self.gpu.as_mut()
                    && let Err(e) = gpu.draw(self.frames)
                {
                    say(&format!("smoke: {e}"));
                    event_loop.exit();
                    return;
                }
                if self.frames % 120 == 0 {
                    say(&format!("smoke: {} frames drawn", self.frames));
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

impl Gpu {
    fn new(window: Arc<Window>) -> Result<Self, String> {
        // Vulkan first (wgpu's best on Android), then GLES: Waydroid's Vulkan
        // passthrough is the piece most likely to be missing, and a device with
        // only GLES should still draw. Each backend's failure is reported.
        let mut failures = Vec::new();
        for backends in [wgpu::Backends::VULKAN, wgpu::Backends::GL] {
            match Self::open(window.clone(), backends) {
                Ok(gpu) => {
                    if !failures.is_empty() {
                        say(&format!("smoke: earlier attempts: {}", failures.join("; ")));
                    }
                    return Ok(gpu);
                }
                Err(e) => failures.push(format!("{backends:?}: {e}")),
            }
        }
        Err(format!("no backend drew a frame: {}", failures.join("; ")))
    }

    fn open(window: Arc<Window>, backends: wgpu::Backends) -> Result<Self, String> {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = backends;
        let instance = wgpu::Instance::new(descriptor);
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| format!("no surface: {e}"))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            ..Default::default()
        }))
        .map_err(|e| format!("no adapter: {e}"))?;
        let info = adapter.get_info();
        let limits = adapter.limits();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("Horizon smoke"),
            required_features: wgpu::Features::empty(),
            required_limits: limits,
            ..Default::default()
        }))
        .map_err(|e| format!("no device: {e}"))?;
        let size = window.inner_size();
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .or_else(|| caps.formats.first().copied())
            .unwrap_or(wgpu::TextureFormat::Bgra8Unorm);
        let present_mode = if caps.present_modes.contains(&wgpu::PresentMode::Fifo) {
            wgpu::PresentMode::Fifo
        } else {
            caps.present_modes
                .first()
                .copied()
                .unwrap_or(wgpu::PresentMode::Fifo)
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps
                .alpha_modes
                .first()
                .copied()
                .unwrap_or(wgpu::CompositeAlphaMode::Opaque),
            view_formats: vec![format],
            color_space: wgpu::SurfaceColorSpace::default(),
        };
        surface.configure(&device, &config);
        say(&format!(
            "smoke: adapter {:?} ({:?}), backend {:?}, driver {} {}",
            info.name, info.device_type, info.backend, info.driver, info.driver_info
        ));
        say(&format!(
            "smoke: surface {}x{} {format:?} {present_mode:?}, present modes {:?}",
            config.width, config.height, caps.present_modes
        ));
        Ok(Self {
            surface,
            window,
            device,
            queue,
            config,
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if self.config.width == width && self.config.height == height {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        say(&format!("smoke: resized to {width}x{height}"));
    }

    fn draw(&mut self, frame: u64) -> Result<(), String> {
        use wgpu::CurrentSurfaceTexture as Acquired;
        let texture = match self.surface.get_current_texture() {
            Acquired::Success(texture) | Acquired::Suboptimal(texture) => texture,
            Acquired::Lost | Acquired::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            Acquired::Timeout | Acquired::Occluded => return Ok(()),
            Acquired::Validation => return Err("surface acquisition failed validation".into()),
        };
        let view = texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        // Two colours breathing at 60 frames a second: a frozen screen and a
        // running one look the same in a screenshot otherwise.
        let t = ((frame % 240) as f32 / 240.0 * std::f32::consts::TAU).sin() * 0.5 + 0.5;
        let clear = wgpu::Color {
            r: 0.04 + 0.08 * t as f64,
            g: 0.06 + 0.10 * (1.0 - t) as f64,
            b: 0.18 + 0.20 * t as f64,
            a: 1.0,
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Smoke"),
            });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Smoke"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        self.queue.submit(Some(encoder.finish()));
        self.window.pre_present_notify();
        self.queue.present(texture);
        Ok(())
    }
}


