use std::{fs::File, io::Write, path::Path};

use image::{
    ExtendedColorType, ImageEncoder,
    codecs::{avif::AvifEncoder, jpeg::JpegEncoder},
    imageops::FilterType::Triangle,
};
use libwebp_sys::{WebPEncodeRGB, WebPEncodeRGBA};
use tokio::sync::broadcast::Sender;
use tracing::{debug, error};

use crate::{
    sse::{SSELevel as Level, SSEMessage},
    utils::errors::ServiceError,
};

fn encode_webp(
    input_image: &[u8],
    width: u32,
    height: u32,
    quality: i32,
    has_alpha: bool,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    unsafe {
        let mut out_buf = std::ptr::null_mut();

        let len = if has_alpha {
            let stride = width as i32 * 4;
            WebPEncodeRGBA(
                input_image.as_ptr(),
                width as i32,
                height as i32,
                stride,
                quality as f32,
                &mut out_buf,
            )
        } else {
            let stride = width as i32 * 3;
            WebPEncodeRGB(
                input_image.as_ptr(),
                width as i32,
                height as i32,
                stride,
                quality as f32,
                &mut out_buf,
            )
        };

        Ok(std::slice::from_raw_parts(out_buf, len as usize).into())
    }
}

pub fn save_image(
    image_widths: &[u32],
    image_types: &[&str],
    input_file: &Path,
    tx: Sender<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Bild laden
    let img = image::open(input_file)?;
    let img_name = input_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("image");

    for &in_w in image_widths {
        let scale_factor = in_w as f32 / img.width() as f32;
        let in_h = (img.height() as f32 * scale_factor).round() as u32;
        let resized = img.resize_exact(in_w, in_h, Triangle);
        let w = resized.width();
        let h = resized.height();

        for &ext in image_types {
            let mut buffer = Vec::new();
            let output_path = input_file.with_file_name(format!("{img_name}-{w}.{ext}"));

            debug!("Process {w}x{h}: {output_path:?}");

            if ext == "jpg" || ext == "jpeg" {
                let mut encoder_jpeg = JpegEncoder::new_with_quality(&mut buffer, 78);
                let rgb_image = resized.to_rgb8();
                encoder_jpeg.encode(rgb_image.as_raw(), w, h, ExtendedColorType::Rgb8)?;

                File::create(&output_path)?.write_all(&buffer)?;
            } else if ext == "avif" {
                let color_type = match resized.has_alpha() {
                    true => ExtendedColorType::Rgba8,
                    false => ExtendedColorType::Rgb8,
                };

                let encoder_avif = AvifEncoder::new_with_speed_quality(&mut buffer, 5, 78);
                encoder_avif.write_image(resized.as_bytes(), w, h, color_type)?;

                File::create(&output_path)?.write_all(&buffer)?;
            } else if ext == "webp" {
                let encoder_webp = encode_webp(resized.as_bytes(), w, h, 76, resized.has_alpha())?;

                File::create(&output_path)?.write_all(&encoder_webp)?;
            }

            let msg = SSEMessage::new(
                Level::Success,
                &format!("Created: '{}'", output_path.to_string_lossy()),
            );
            if let Err(e) = tx.send(msg.to_string()) {
                error!("{e}");
            };
        }
    }

    Ok(())
}

pub async fn delete_image(size: &(u32, u32), path: &Path, name: &str) -> Result<(), ServiceError> {
    let (w, _) = size.to_owned();
    let thumb_jpeg = path.join(format!("{name}-{w}.jpg"));
    let thumb_avif = path.join(format!("{name}-{w}.avif"));

    if thumb_jpeg.is_file() {
        tokio::fs::remove_file(&thumb_jpeg).await?;
    };

    if thumb_avif.is_file() {
        tokio::fs::remove_file(&thumb_avif).await?;
    };

    Ok(())
}
