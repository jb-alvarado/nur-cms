use std::{fs::File, io::Write, path::Path};

use colored::Colorize;
use image::{
    ExtendedColorType, ImageEncoder,
    codecs::{
        avif::AvifEncoder,
        jpeg::JpegEncoder,
        png::{CompressionType, FilterType as PngFilterType, PngEncoder},
    },
    imageops::FilterType::Triangle,
};
use libwebp_sys::{WebPEncodeRGB, WebPEncodeRGBA};
use tokio::sync::broadcast::Sender;
use tracing::{debug, error};

use crate::{
    sse::{SSELevel as Level, SSEMessage},
    utils::errors::ServiceError,
};

type VarianceType = Vec<(i32, i32, String)>;

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
    mut image_resolutions: Vec<i32>,
    image_types: &[String],
    input_file: &Path,
    tx: Sender<String>,
) -> Result<VarianceType, Box<dyn std::error::Error>> {
    let img = image::open(input_file)?;
    let img_name = input_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("image");
    let mut variants = Vec::new();

    if !image_resolutions.contains(&320) {
        image_resolutions.insert(0, 320);
    }

    for in_w in image_resolutions {
        let scale_factor = in_w as f32 / img.width() as f32;
        let in_h = (img.height() as f32 * scale_factor).round() as u32;

        if img.width() <= in_w as u32 {
            continue;
        }

        let resized = img.resize_exact(in_w as u32, in_h, Triangle);
        let w = resized.width();
        let h = resized.height();

        debug!(
            "Process {}x{}, types {image_types:?}: {}",
            w.to_string().yellow(),
            h.to_string().yellow(),
            input_file.to_string_lossy().bright_magenta()
        );

        for ext in image_types {
            let mut buffer = Vec::new();
            let file_name = format!("{img_name}-{w}.{ext}");
            let mut output_path = input_file.with_file_name(&file_name);
            let has_alpha = resized.has_alpha();
            let color_type = match has_alpha {
                true => ExtendedColorType::Rgba8,
                false => ExtendedColorType::Rgb8,
            };

            if ["jpg", "jpeg", "png"].contains(&ext.as_str()) {
                if has_alpha {
                    output_path = output_path.with_extension("png");

                    let encoder_png = PngEncoder::new_with_quality(
                        &mut buffer,
                        CompressionType::Best,
                        PngFilterType::Adaptive,
                    );
                    encoder_png.write_image(resized.as_bytes(), w, h, color_type)?;
                } else {
                    let mut encoder_jpeg = JpegEncoder::new_with_quality(&mut buffer, 78);
                    let rgb_image = resized.to_rgb8();
                    encoder_jpeg.encode(rgb_image.as_raw(), w, h, ExtendedColorType::Rgb8)?;
                }
            } else if ext == "avif" {
                let encoder_avif = AvifEncoder::new_with_speed_quality(&mut buffer, 5, 78);
                encoder_avif.write_image(resized.as_bytes(), w, h, color_type)?;
            } else if ext == "webp" {
                buffer = encode_webp(resized.as_bytes(), w, h, 76, has_alpha)?;
            }

            if !buffer.is_empty() {
                File::create(&output_path)?.write_all(&buffer)?;
                variants.push((
                    w as i32,
                    h as i32,
                    output_path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                ));
            }

            let msg = SSEMessage::new(Level::Success, &format!("Created: '{file_name}'"));
            if let Err(e) = tx.send(msg.to_string()) {
                error!("{e}");
            };
        }
    }

    Ok(variants)
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
