use std::{fs::File, io::Write, path::Path};

use colored::Colorize;
use image::{
    ColorType, ExtendedColorType, GenericImageView, ImageEncoder,
    codecs::{
        avif::AvifEncoder,
        jpeg::JpegEncoder,
        png::{CompressionType, FilterType as PngFilterType, PngEncoder},
    },
    imageops::FilterType::Triangle,
};
use libwebp_sys::{WebPEncodeRGB, WebPEncodeRGBA};
use tokio::sync::broadcast::Sender;
use tracing::{debug, info, warn};

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
    tx: Option<Sender<String>>,
) -> Result<VarianceType, Box<dyn std::error::Error>> {
    let img = image::open(input_file)?;
    let (orig_w, orig_h) = img.dimensions();
    let img_name = input_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("image");

    // ensure 320 exists, sort + dedup
    if !image_resolutions.contains(&320) {
        image_resolutions.push(320);
    }
    image_resolutions.sort_unstable();
    image_resolutions.dedup();

    let mut variants = Vec::new();

    for in_w in image_resolutions {
        if orig_w <= in_w as u32 {
            continue;
        }

        let scale = in_w as f32 / orig_w as f32;
        let in_h = (orig_h as f32 * scale).round() as u32;

        let resized = img.resize_exact(in_w as u32, in_h, Triangle);
        let (w, h) = resized.dimensions();

        debug!(
            "Process {}x{}, types {image_types:?}: {}",
            w.to_string().yellow(),
            h.to_string().yellow(),
            input_file.to_string_lossy().bright_magenta()
        );

        let has_alpha = resized.has_alpha();
        let is_gray = matches!(
            resized.color(),
            ColorType::L8 | ColorType::L16 | ColorType::La8 | ColorType::La16
        );

        // Universal colorspace for PNG/JPEG/AVIF
        let (bytes_normal, color_type_normal) = match (is_gray, has_alpha) {
            (true, true) => (resized.to_luma_alpha8().into_raw(), ExtendedColorType::La8),
            (true, false) => (resized.to_luma8().into_raw(), ExtendedColorType::L8),
            (false, true) => (resized.to_rgba8().into_raw(), ExtendedColorType::Rgba8),
            (false, false) => (resized.to_rgb8().into_raw(), ExtendedColorType::Rgb8),
        };

        for ext in image_types {
            let mut buffer = Vec::new();
            let mut output_name = format!("{img_name}-{w}.{ext}");
            let mut output_path = input_file.with_file_name(&output_name);

            if output_path.is_file() {
                if ["jpg", "jpeg"].contains(&ext.as_str()) && has_alpha {
                    output_path = output_path.with_extension("png");
                    output_name = output_name.replace(ext, "png");
                }

                warn!(
                    "Skip existing file: {}",
                    output_path.to_string_lossy().bright_magenta()
                );

                variants.push((w as i32, h as i32, output_name.clone()));

                continue;
            }

            match ext.as_str() {
                // PNG/JPEG handling
                "jpg" | "jpeg" | "png" => {
                    if has_alpha {
                        // Always convert alpha images to PNG
                        output_path = output_path.with_extension("png");
                        output_name = output_name.replace(ext, "png");

                        let encoder = PngEncoder::new_with_quality(
                            &mut buffer,
                            CompressionType::Best,
                            PngFilterType::Adaptive,
                        );
                        encoder.write_image(&bytes_normal, w, h, color_type_normal)?;
                    } else {
                        let mut encoder = JpegEncoder::new_with_quality(&mut buffer, 78);
                        encoder.encode(&bytes_normal, w, h, color_type_normal)?;
                    }
                }

                // AVIF
                "avif" => {
                    let encoder = AvifEncoder::new_with_speed_quality(&mut buffer, 5, 78);
                    encoder.write_image(&bytes_normal, w, h, color_type_normal)?;
                }

                // WEBP — always RGB8 / RGBA8
                "webp" => {
                    // Colorspace *specifically for WEBP*
                    // (libwebp_sys requires RGB8 or RGBA8)
                    let webp_bytes = if has_alpha {
                        resized.to_rgba8().into_raw()
                    } else {
                        resized.to_rgb8().into_raw()
                    };

                    buffer = encode_webp(&webp_bytes, w, h, 76, has_alpha)?;
                }

                _ => {}
            }

            if !buffer.is_empty() {
                File::create(&output_path)?.write_all(&buffer)?;

                variants.push((w as i32, h as i32, output_name.clone()));
            }

            match tx {
                Some(ref tx) => {
                    let msg = SSEMessage::new(Level::Success, &format!("Created: '{output_name}'"));
                    let _ = tx.send(msg.to_string());
                }
                None => info!("Created: '{output_name}'"),
            }
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
