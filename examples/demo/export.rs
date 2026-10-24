//! Offscreen GPU rendering and readback for chart image delivery.

use super::{Screenshot, ScreenshotTarget};
use eframe::egui_wgpu::{self, wgpu};
use egui_charts::widget::export::ChartImage;
use std::sync::{Arc, mpsc};

pub(super) fn render_image(
    state: &egui_wgpu::RenderState,
    mut frame: ChartImage,
    target: ScreenshotTarget,
    generation: u64,
    sender: mpsc::Sender<Screenshot>,
    ctx: egui::Context,
) -> Result<(), String> {
    let [width, height] = frame.size;
    let device = &state.device;
    if width.max(height) > device.limits().max_texture_dimension_2d {
        frame.textures_delta.clear();
        return Err("Image exceeds the GPU's maximum texture size".into());
    }
    let extent = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Chart export"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut renderer =
        egui_wgpu::Renderer::new(device, wgpu::TextureFormat::Rgba8Unorm, Default::default());
    for (id, deltas) in frame.textures_delta.set.drain() {
        for delta in deltas {
            renderer.update_texture(device, &state.queue, id, &delta);
        }
    }
    let screen = egui_wgpu::ScreenDescriptor {
        size_in_pixels: frame.size,
        pixels_per_point: 1.0,
    };
    let mut encoder = device.create_command_encoder(&Default::default());
    let commands = renderer.update_buffers(
        device,
        &state.queue,
        &mut encoder,
        &frame.primitives,
        &screen,
    );
    {
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Chart export"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &texture.create_view(&Default::default()),
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            })
            .forget_lifetime();
        renderer.render(&mut pass, &frame.primitives, &screen);
    }
    let stride = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Chart export readback"),
        size: u64::from(stride) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: None,
            },
        },
        extent,
    );
    state
        .queue
        .submit(commands.into_iter().chain([encoder.finish()]));
    for id in frame.textures_delta.free.drain() {
        renderer.free_texture(&id);
    }
    let readback = buffer.clone();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let result = result.map_err(|error| error.to_string()).and_then(|()| {
                let bytes = readback
                    .slice(..)
                    .get_mapped_range()
                    .map_err(|error| error.to_string())?;
                let pixels = bytes
                    .chunks(stride as usize)
                    .flat_map(|row| {
                        row[..width as usize * 4].chunks_exact(4).map(|rgba| {
                            egui::Color32::from_rgba_premultiplied(
                                rgba[0], rgba[1], rgba[2], rgba[3],
                            )
                        })
                    })
                    .collect();
                drop(bytes);
                readback.unmap();
                Ok(Arc::new(egui::ColorImage::new(
                    [width as usize, height as usize],
                    pixels,
                )))
            });
            let _ = sender.send((result, target, generation));
            ctx.request_repaint();
        });
    Ok(())
}
