//! Host-owned picture selection and checks before an image drawing is loaded.

use egui_charts::Chart;
use egui_charts::widget::ImageRequest;
use std::io::Cursor;
use std::sync::mpsc::{self, Receiver, Sender};

const MAX_FILE_BYTES: usize = 20 * 1024 * 1024;
const MAX_IMAGE_SIDE: usize = 10_000;
const MAX_PIXELS: u64 = 33_554_432;

type Reply = (usize, usize, Result<(String, Vec<u8>), &'static str>);

pub(super) struct Images {
    tx: Sender<Reply>,
    rx: Receiver<Reply>,
    #[cfg(target_arch = "wasm32")]
    next: u64,
}

impl Default for Images {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            #[cfg(target_arch = "wasm32")]
            next: 0,
        }
    }
}

impl Images {
    pub(super) fn request(&self, ctx: &egui::Context, slot: usize, request: ImageRequest) {
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        #[cfg(not(target_arch = "wasm32"))]
        {
            let result = (|| {
                use std::io::Read;
                let path = if let Some(file) = request.file {
                    file.path().to_owned()
                } else {
                    rfd::FileDialog::new()
                        .add_filter("Pictures", &["png", "jpg", "jpeg"])
                        .pick_file()
                        .ok_or("Picture selection cancelled")?
                };
                let uri = format!(
                    "file://{}",
                    path.to_str().ok_or("Picture path is not UTF-8")?
                );
                let file = std::fs::File::open(path).map_err(|_| "Could not read picture")?;
                if file
                    .metadata()
                    .map_err(|_| "Could not read picture size")?
                    .len()
                    > MAX_FILE_BYTES as u64
                {
                    return Err("Picture exceeds 20 MiB");
                }
                let mut bytes = Vec::new();
                file.take(MAX_FILE_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|_| "Could not read picture")?;
                Ok((uri, bytes))
            })();
            let _ = tx.send((slot, request.drawing, result));
            ctx.request_repaint();
        }
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            let result = if let Some(file) = request.file {
                let name = file.path().to_string_lossy().into_owned();
                file.bytes_async()
                    .await
                    .map(|bytes| (name, bytes))
                    .map_err(|_| "Could not read picture")
            } else if let Some(file) = rfd::AsyncFileDialog::new()
                .add_filter("Pictures", &["png", "jpg", "jpeg"])
                .pick_file()
                .await
            {
                Ok((file.file_name(), file.read().await))
            } else {
                Err("Picture selection cancelled")
            };
            let _ = tx.send((slot, request.drawing, result));
            ctx.request_repaint();
        });
    }

    pub(super) fn poll(
        &mut self,
        ctx: &egui::Context,
        chart: &mut Chart,
        slot: usize,
    ) -> Vec<&'static str> {
        self.rx
            .try_iter()
            .filter_map(|(asked_slot, drawing, result)| {
                let answer = (|| {
                    if asked_slot != slot {
                        return Err("Picture was for a different chart slot");
                    }
                    let (uri, bytes) = result?;
                    let size = checked_picture(&bytes, ctx.input(|i| i.max_texture_side))?;
                    #[cfg(target_arch = "wasm32")]
                    let uri = {
                        self.next += 1;
                        let uri = format!("bytes://egui-charts-demo/image/{}/{uri}", self.next);
                        // shortcut: web pictures live in memory; store their bytes in host storage
                        // when the demo gains persistent picture attachments.
                        ctx.include_bytes(uri.clone(), bytes);
                        uri
                    };
                    chart.set_drawing_image(
                        drawing,
                        uri,
                        egui::vec2(size[0] as f32, size[1] as f32),
                    )
                })();
                answer.err()
            })
            .collect()
    }
}

/// Check the header before decoding or asking egui to allocate a texture.
fn checked_picture(bytes: &[u8], max_side: usize) -> Result<[u32; 2], &'static str> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err("Picture exceeds 20 MiB");
    }
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| "Could not read picture format")?;
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Png | image::ImageFormat::Jpeg)
    ) {
        return Err("Choose a PNG or JPEG picture");
    }
    let format = reader.format();
    let (width, height) = reader
        .into_dimensions()
        .map_err(|_| "Could not read picture dimensions")?;
    let max_side = max_side.min(MAX_IMAGE_SIDE) as u64;
    if width == 0
        || height == 0
        || u64::from(width) > max_side
        || u64::from(height) > max_side
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        return Err("Picture dimensions exceed the texture limits");
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes));
    reader.set_format(format.ok_or("Could not read picture format")?);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(width);
    limits.max_image_height = Some(height);
    limits.max_alloc = Some(MAX_PIXELS * 4);
    reader.limits(limits);
    reader.decode().map_err(|_| "Could not decode picture")?;
    Ok([width, height])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Untrusted files must pass byte, format and header limits before decode, and truncated
    /// files with a valid header must still be refused.
    #[test]
    fn pictures_are_checked_before_loading() {
        let picture = image::DynamicImage::new_rgb8(2, 1);
        let encode = |format| {
            let mut bytes = Cursor::new(Vec::new());
            picture.write_to(&mut bytes, format).unwrap();
            bytes.into_inner()
        };
        let png = encode(image::ImageFormat::Png);
        let jpeg = encode(image::ImageFormat::Jpeg);
        let mut header = Vec::new();
        // A valid oversized PNG is cheap to encode as a uniform image, and remains under 20 MiB.
        image::DynamicImage::new_luma8(6000, 6000)
            .write_to(&mut Cursor::new(&mut header), image::ImageFormat::Png)
            .unwrap();
        let image_data = png.windows(4).position(|chunk| chunk == b"IDAT").unwrap();
        let truncated = &png[..image_data + 5];
        let too_many_bytes = vec![0; MAX_FILE_BYTES + 1];
        for (bytes, max_side, expected) in [
            (
                too_many_bytes.as_slice(),
                4096,
                Err("Picture exceeds 20 MiB"),
            ),
            (png.as_slice(), 4096, Ok([2, 1])),
            (jpeg.as_slice(), 4096, Ok([2, 1])),
            (
                png.as_slice(),
                1,
                Err("Picture dimensions exceed the texture limits"),
            ),
            (
                header.as_slice(),
                10_000,
                Err("Picture dimensions exceed the texture limits"),
            ),
            (truncated, 4096, Err("Could not decode picture")),
            (
                b"GIF89a".as_slice(),
                4096,
                Err("Choose a PNG or JPEG picture"),
            ),
            (b"".as_slice(), 4096, Err("Choose a PNG or JPEG picture")),
            (
                b"random bytes".as_slice(),
                4096,
                Err("Choose a PNG or JPEG picture"),
            ),
        ] {
            assert_eq!(checked_picture(bytes, max_side), expected);
        }
    }
}
