//! The loading screen: while the APK's data is unpacked, a window that says so
//! and a bar that fills.
//!
//! It draws on the activity's own `ANativeWindow` — not through winit — because
//! winit's Android event loop is the process's one: the game builds it a moment
//! later, when the data is in place, and that has to be the only one. A raw
//! wgpu surface needs no event loop at all: a loop here, `present`, repeat.
//!
//! Everything is quads: the breathing purple background, the bar, and text from
//! a 3x5 pixel font (the game's own font is inside the data being unpacked).
use std::{
    ffi::c_void,
    sync::{
        Arc,
        atomic::Ordering,
    },
    time::{Duration, Instant},
};

use android_activity::AndroidApp;
use raw_window_handle::{AndroidNdkWindowHandle, RawWindowHandle};

use crate::{
    bundled::Progress,
    say,
};

/// Draws until the unpack is done (or failed), then returns.
pub(super) fn run(app: &AndroidApp, progress: Arc<Progress>) -> Result<(), String> {
    // `android_main` runs from `ANativeActivity_onCreate`, before the framework
    // hands the activity its surface; the window shows up a moment later, while
    // the unpack is already running.
    let mut window = None;
    for _ in 0..120 {
        if progress.done.load(Ordering::Acquire) {
            return Ok(());
        }
        if let Some(found) = app.native_window() {
            window = Some(found);
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let Some(window) = window else {
        return Err("the activity has no native window".into());
    };
    let (width, height) = (window.width().max(1) as u32, window.height().max(1) as u32);

    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::VULKAN | wgpu::Backends::GL;
    let instance = wgpu::Instance::new(descriptor);
    // Safety: the activity's window outlives this surface (the guard is held
    // until the end of the function, the surface is dropped before it).
    let surface = unsafe {
        instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: None,
            raw_window_handle: RawWindowHandle::AndroidNdk(AndroidNdkWindowHandle::new(
                window.ptr().cast::<c_void>(),
            )),
        })
    }
    .map_err(|e| format!("no surface: {e}"))?;

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .map_err(|e| format!("no adapter: {e}"))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Horizon loading"),
        required_features: wgpu::Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .map_err(|e| format!("no device: {e}"))?;

    let caps = surface.get_capabilities(&adapter);
    let format = caps
        .formats
        .iter()
        .copied()
        .find(|f| !f.is_srgb())
        .or_else(|| caps.formats.first().copied())
        .unwrap_or(wgpu::TextureFormat::Bgra8Unorm);
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width,
        height,
        present_mode: wgpu::PresentMode::Fifo,
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

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Loading"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let attributes = wgpu::vertex_attr_array![0 => Float32x2, 1 => Unorm8x4];
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Loading quads"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attributes,
            })],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });

    // Room for the bar, the labels and a few thousand glyph pixels.
    let mut vertices: Vec<Vertex> = Vec::with_capacity(16_384);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Loading quads"),
        size: (16_384 * std::mem::size_of::<Vertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let started = Instant::now();
    let mut frames = 0u64;
    loop {
        let done = progress.done.load(Ordering::Acquire);
        let fraction = progress.fraction();
        let t = started.elapsed().as_secs_f32();
        let seconds = progress.bytes.load(Ordering::Relaxed) as f32
            / 1048576.0
            / t.max(0.001);
        vertices.clear();
        scene(
            &mut vertices,
            (width, height),
            fraction,
            done,
            frames,
            seconds,
            progress
                .failed
                .lock()
                .ok()
                .and_then(|e| e.clone())
                .as_deref(),
        );
        if let Err(e) = paint(&surface, &device, &queue, &config, &pipeline, &buffer, &vertices) {
            // A loading screen that cannot draw must not cost the install: the
            // unpack is already running and the game may still start.
            say(&format!("loading: {e}"));
            return Ok(());
        }
        frames += 1;
        if done {
            // The last frame shows the whole bar.
            std::thread::sleep(Duration::from_millis(80));
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(16));
    }
}

const SHADER: &str = r#"
struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs(@location(0) at: vec2<f32>, @location(1) color: vec4<f32>) -> Out {
    var out: Out;
    out.pos = vec4<f32>(at, 0.0, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

#[repr(C)]
#[derive(Clone, Copy)]
struct Vertex {
    pos: [f32; 2],
    color: [u8; 4],
}

/// Pixel coordinates, top left, to the clip space wgpu wants.
fn rect(vertices: &mut Vec<Vertex>, (w, h): (u32, u32), x: f32, y: f32, rw: f32, rh: f32, color: [u8; 4]) {
    let (w, h) = (w as f32, h as f32);
    let x0 = x / w * 2.0 - 1.0;
    let x1 = (x + rw) / w * 2.0 - 1.0;
    let y0 = 1.0 - y / h * 2.0;
    let y1 = 1.0 - (y + rh) / h * 2.0;
    for (px, py) in [(x0, y0), (x1, y0), (x1, y1), (x0, y0), (x1, y1), (x0, y1)] {
        vertices.push(Vertex { pos: [px, py], color });
    }
}

/// The 3x5 font: five rows of three bits, most significant bit on the left.
fn glyph(c: char) -> [u8; 5] {
    match c.to_ascii_uppercase() {
        'A' => [0b010, 0b101, 0b111, 0b101, 0b101],
        'B' => [0b110, 0b101, 0b110, 0b101, 0b110],
        'C' => [0b011, 0b100, 0b100, 0b100, 0b011],
        'D' => [0b110, 0b101, 0b101, 0b101, 0b110],
        'E' => [0b111, 0b100, 0b110, 0b100, 0b111],
        'F' => [0b111, 0b100, 0b110, 0b100, 0b100],
        'G' => [0b011, 0b100, 0b101, 0b101, 0b011],
        'H' => [0b101, 0b101, 0b111, 0b101, 0b101],
        'I' => [0b111, 0b010, 0b010, 0b010, 0b111],
        'J' => [0b001, 0b001, 0b001, 0b101, 0b010],
        'K' => [0b101, 0b101, 0b110, 0b101, 0b101],
        'L' => [0b100, 0b100, 0b100, 0b100, 0b111],
        'M' => [0b101, 0b111, 0b111, 0b101, 0b101],
        'N' => [0b110, 0b101, 0b101, 0b101, 0b101],
        'O' => [0b010, 0b101, 0b101, 0b101, 0b010],
        'P' => [0b110, 0b101, 0b110, 0b100, 0b100],
        'Q' => [0b010, 0b101, 0b101, 0b110, 0b011],
        'R' => [0b110, 0b101, 0b110, 0b101, 0b101],
        'S' => [0b011, 0b100, 0b010, 0b001, 0b110],
        'T' => [0b111, 0b010, 0b010, 0b010, 0b010],
        'U' => [0b101, 0b101, 0b101, 0b101, 0b111],
        'V' => [0b101, 0b101, 0b101, 0b101, 0b010],
        'W' => [0b101, 0b101, 0b111, 0b111, 0b101],
        'X' => [0b101, 0b101, 0b010, 0b101, 0b101],
        'Y' => [0b101, 0b101, 0b010, 0b010, 0b010],
        'Z' => [0b111, 0b001, 0b010, 0b100, 0b111],
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b110, 0b001, 0b010, 0b100, 0b111],
        '3' => [0b110, 0b001, 0b010, 0b001, 0b110],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b110, 0b001, 0b110],
        '6' => [0b011, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b010, 0b010, 0b010],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b110],
        '%' => [0b101, 0b001, 0b010, 0b100, 0b101],
        '.' => [0b000, 0b000, 0b000, 0b000, 0b010],
        '-' => [0b000, 0b000, 0b111, 0b000, 0b000],
        '/' => [0b001, 0b001, 0b010, 0b100, 0b100],
        ':' => [0b000, 0b010, 0b000, 0b010, 0b000],
        _ => [0b000; 5],
    }
}

fn text(vertices: &mut Vec<Vertex>, size: (u32, u32), s: &str, x: f32, y: f32, scale: f32, color: [u8; 4]) -> f32 {
    let mut pen = x;
    for c in s.chars() {
        let rows = glyph(c);
        for (row, bits) in rows.iter().enumerate() {
            for col in 0..3 {
                if bits & (0b100 >> col) != 0 {
                    rect(
                        vertices,
                        size,
                        pen + col as f32 * scale,
                        y + row as f32 * scale,
                        scale,
                        scale,
                        color,
                    );
                }
            }
        }
        pen += 4.0 * scale;
    }
    pen - x
}

/// One frame: background, title, bar, percent.
fn scene(
    vertices: &mut Vec<Vertex>,
    size: (u32, u32),
    fraction: Option<f32>,
    done: bool,
    frame: u64,
    speed: f32,
    failed: Option<&str>,
) {
    let (w, h) = (size.0 as f32, size.1 as f32);
    // Breathing between two purples, as the smoke screen does: a frozen screen
    // and a running one look the same in a screenshot otherwise.
    let t = (frame as f32 / 90.0).sin() * 0.5 + 0.5;
    let background = [
        (40.0 + 18.0 * t) as u8,
        (30.0 + 12.0 * t) as u8,
        (84.0 + 26.0 * (1.0 - t)) as u8,
        255,
    ];
    rect(vertices, size, 0.0, 0.0, w, h, background);

    let white = [238u8, 234, 255, 255];
    let accent = [176u8, 140, 255, 255];
    let dim = [110u8, 96, 160, 255];

    let scale = (w / 300.0).clamp(2.0, 12.0);
    let title = if failed.is_some() {
        "PREPARING FAILED"
    } else if done {
        "STARTING"
    } else {
        "PREPARING 26.3"
    };
    let title_width = title.chars().count() as f32 * 4.0 * scale;
    text(vertices, size, title, (w - title_width) / 2.0, h * 0.38, scale, white);

    // The bar: an outline, then the filled part.
    let bar_w = (w * 0.62).min(1200.0);
    let bar_h = (scale * 2.2).max(10.0);
    let (x, y) = ((w - bar_w) / 2.0, h * 0.50);
    let frame_color = if failed.is_some() { [200u8, 90, 90, 255] } else { dim };
    rect(vertices, size, x - 2.0, y - 2.0, bar_w + 4.0, bar_h + 4.0, frame_color);

    let fill = match (failed, fraction) {
        // Nothing measured yet: a segment sliding along, so the bar is never a
        // dead line while the list is being read.
        (None, None) => {
            let pos = (frame as f32 / 60.0).fract();
            let seg = bar_w * 0.22;
            Some(((pos * (bar_w + seg) - seg).clamp(0.0, bar_w - seg), seg, false))
        }
        (Some(_), _) => None,
        (None, Some(f)) => Some((0.0, bar_w * f, true)),
    };
    if let Some((fx, fw, complete)) = fill {
        let color = if complete { accent } else { white };
        rect(vertices, size, x + fx, y, fw.max(2.0), bar_h, color);
    }

    let label = match failed {
        Some(e) => {
            let short: String = e.chars().take(40).collect();
            format!("{short}")
        }
        None => match fraction {
            Some(f) => format!("{}%  {} MIB", (f * 100.0) as u32, speed.max(0.0) as u32),
            None => "READING 26.3".to_string(),
        },
    };
    let label_width = label.chars().count() as f32 * 4.0 * scale;
    text(vertices, size, &label, (w - label_width) / 2.0, y + bar_h + 3.0 * scale, scale, dim);
}

#[allow(clippy::too_many_arguments)]
fn paint(
    surface: &wgpu::Surface<'_>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    config: &wgpu::SurfaceConfiguration,
    pipeline: &wgpu::RenderPipeline,
    buffer: &wgpu::Buffer,
    vertices: &[Vertex],
) -> Result<(), String> {
    use wgpu::CurrentSurfaceTexture as Acquired;
    let texture = match surface.get_current_texture() {
        Acquired::Success(texture) | Acquired::Suboptimal(texture) => texture,
        Acquired::Lost | Acquired::Outdated => {
            surface.configure(device, config);
            return Ok(());
        }
        Acquired::Timeout | Acquired::Occluded => return Ok(()),
        Acquired::Validation => return Err("surface acquisition failed validation".into()),
    };
    let view = texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Loading"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Loading"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if !vertices.is_empty() {
            queue.write_buffer(buffer, 0, bytemuck_cast(vertices));
            pass.set_pipeline(pipeline);
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..vertices.len() as u32, 0..1);
        }
    }
    queue.submit(Some(encoder.finish()));
    queue.present(texture);
    Ok(())
}

/// `Vertex` is `#[repr(C)]` of two `f32` and four `u8`: no padding, no need for
/// a bytemuck dependency.
fn bytemuck_cast(vertices: &[Vertex]) -> &[u8] {
    // Safety: `Vertex` is plain old data, and the slice is read-only for the
    // duration of the copy.
    unsafe {
        std::slice::from_raw_parts(vertices.as_ptr().cast::<u8>(), std::mem::size_of_val(vertices))
    }
}
