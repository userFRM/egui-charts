//! Offscreen GPU rendering and readback for chart image delivery.

use super::{Screenshot, ScreenshotTarget};
use eframe::egui_glow::Painter;
use eframe::glow::{self, HasContext as _};
use egui_charts::widget::ChartImage;
use std::sync::{Arc, mpsc};

pub(super) fn render_image(
    gl: &Arc<glow::Context>,
    mut frame: ChartImage,
    target: ScreenshotTarget,
    generation: u64,
    sender: mpsc::Sender<Screenshot>,
    ctx: egui::Context,
) -> Result<(), String> {
    let [width, height] = frame.size;
    let mut painter =
        Painter::new(gl.clone(), "", None, false).map_err(|error| error.to_string())?;
    if width.max(height) as usize > painter.max_texture_side() {
        painter.destroy();
        frame.textures_delta.clear();
        return Err("Image exceeds the GPU's maximum texture size".into());
    }
    let image = paint(gl, &mut painter, &mut frame);
    painter.destroy();
    let image = image?;
    let _ = sender.send((Ok(Arc::new(image)), target, generation));
    ctx.request_repaint();
    Ok(())
}

/// Paints `frame` into a texture of its size and reads it back.
fn paint(
    gl: &glow::Context,
    painter: &mut Painter,
    frame: &mut ChartImage,
) -> Result<egui::ColorImage, String> {
    let [width, height] = frame.size;
    // SAFETY: the context is current on this thread while the app runs, and the texture and
    // framebuffer made here are unbound and deleted before returning.
    unsafe {
        let texture = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA as i32,
            width as i32,
            height as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        let framebuffer = gl
            .create_framebuffer()
            .inspect_err(|_| gl.delete_texture(texture))?;
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            Some(texture),
            0,
        );
        let complete = gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
        let image = complete.then(|| {
            painter.clear(frame.size, [0.0; 4]);
            painter.paint_and_update_textures(
                frame.size,
                1.0,
                &frame.primitives,
                &mut frame.textures_delta,
            );
            painter.read_screen_rgba(frame.size)
        });
        // The app paints to the default framebuffer.
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.delete_framebuffer(framebuffer);
        gl.delete_texture(texture);
        image.ok_or_else(|| "The GPU cannot render an image of this size".to_owned())
    }
}
