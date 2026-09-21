use std::{
    fs::{self, File},
    io::{BufReader, Write},
    path::Path,
    ptr,
};

use colored::Colorize;
use image::{
    AnimationDecoder, ColorType, ExtendedColorType, GenericImageView, ImageEncoder, ImageFormat,
    RgbaImage,
    codecs::{
        avif::AvifEncoder,
        gif::GifDecoder,
        jpeg::JpegEncoder,
        png::{CompressionType, FilterType as PngFilterType, PngEncoder},
    },
    imageops::FilterType::Triangle,
    metadata::LoopCount,
};
use libwebp_sys::{
    WEBP_MUX_ABI_VERSION, WebPAnimEncoder, WebPAnimEncoderAdd, WebPAnimEncoderAssemble,
    WebPAnimEncoderDelete, WebPAnimEncoderNewInternal, WebPAnimEncoderOptions,
    WebPAnimEncoderOptionsInitInternal, WebPConfig, WebPData, WebPDataClear, WebPEncodeRGB,
    WebPEncodeRGBA, WebPFree, WebPPicture, WebPPictureFree, WebPPictureImportRGBA, WebPPreset,
};
use tokio::sync::broadcast::Sender;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::{
    MAX_IMAGE_PIXELS,
    sse::{SSELevel as Level, SSEMessage},
    utils::errors::NurError,
};

type VarianceType = Vec<(i32, i32, String)>;

const MAX_ANIMATED_IMAGE_FRAMES: usize = 500;
const MAX_ANIMATED_IMAGE_DURATION_MS: i32 = 5 * 60 * 1_000;
const MAX_ANIMATED_IMAGE_PIXEL_FACTOR: u64 = 10;

struct AnimatedWebpEncoder {
    encoder: *mut WebPAnimEncoder,
    config: WebPConfig,
    width: u32,
    height: u32,
}

impl AnimatedWebpEncoder {
    fn new(width: u32, height: u32, loop_count: LoopCount) -> Result<Self, String> {
        let mut options = std::mem::MaybeUninit::<WebPAnimEncoderOptions>::uninit();
        let initialized = unsafe {
            WebPAnimEncoderOptionsInitInternal(options.as_mut_ptr(), WEBP_MUX_ABI_VERSION as i32)
        };
        if initialized == 0 {
            return Err("Could not initialize animated WebP options".into());
        }

        let mut options = unsafe { options.assume_init() };
        options.anim_params.loop_count = match loop_count {
            LoopCount::Infinite => 0,
            LoopCount::Finite(count) => i32::try_from(count.get()).unwrap_or(i32::MAX),
        };
        options.allow_mixed = 1;

        let mut config = WebPConfig::new_with_preset(WebPPreset::WEBP_PRESET_PICTURE, 76.0)
            .map_err(|_| "Could not initialize WebP configuration")?;
        config.thread_level = 1;
        config.exact = 1;

        let encoder = unsafe {
            WebPAnimEncoderNewInternal(
                width as i32,
                height as i32,
                &options,
                WEBP_MUX_ABI_VERSION as i32,
            )
        };
        if encoder.is_null() {
            return Err("Could not initialize animated WebP encoder".into());
        }

        Ok(Self {
            encoder,
            config,
            width,
            height,
        })
    }

    fn add_frame(&mut self, frame: &RgbaImage, timestamp_ms: i32) -> Result<(), String> {
        let mut picture = WebPPicture::new()
            .map_err(|_| "Could not initialize animated WebP frame".to_string())?;
        picture.use_argb = 1;
        picture.width = self.width as i32;
        picture.height = self.height as i32;

        let imported = unsafe {
            WebPPictureImportRGBA(&mut picture, frame.as_raw().as_ptr(), self.width as i32 * 4)
        };
        if imported == 0 {
            unsafe { WebPPictureFree(&mut picture) };
            return Err("Could not import animated WebP frame".into());
        }

        let added =
            unsafe { WebPAnimEncoderAdd(self.encoder, &mut picture, timestamp_ms, &self.config) };
        unsafe { WebPPictureFree(&mut picture) };
        if added == 0 {
            return Err("Could not encode animated WebP frame".into());
        }

        Ok(())
    }

    fn finish(mut self, timestamp_ms: i32) -> Result<Vec<u8>, String> {
        let finalized =
            unsafe { WebPAnimEncoderAdd(self.encoder, ptr::null_mut(), timestamp_ms, ptr::null()) };
        if finalized == 0 {
            return Err("Could not finalize animated WebP frames".into());
        }

        let mut data = WebPData::default();
        let assembled = unsafe { WebPAnimEncoderAssemble(self.encoder, &mut data) };
        if assembled == 0 || data.bytes.is_null() || data.size == 0 {
            if !data.bytes.is_null() {
                unsafe { WebPDataClear(&mut data) };
            }
            return Err("Could not assemble animated WebP".into());
        }

        let bytes = unsafe { std::slice::from_raw_parts(data.bytes, data.size) }.to_vec();
        unsafe { WebPDataClear(&mut data) };
        unsafe { WebPAnimEncoderDelete(self.encoder) };
        self.encoder = ptr::null_mut();

        Ok(bytes)
    }
}

impl Drop for AnimatedWebpEncoder {
    fn drop(&mut self) {
        if !self.encoder.is_null() {
            unsafe { WebPAnimEncoderDelete(self.encoder) };
        }
    }
}

fn frame_delay_ms(frame: &image::Frame) -> Result<i32, String> {
    let (numerator, denominator) = frame.delay().numer_denom_ms();
    let rounded = (u64::from(numerator) + u64::from(denominator) / 2) / u64::from(denominator);

    i32::try_from(rounded.max(1)).map_err(|_| "Animated image frame delay is too large".into())
}

fn normalized_resolutions(mut resolutions: Vec<i32>, original_width: u32) -> Vec<i32> {
    resolutions.retain(|width| *width > 0 && *width <= original_width as i32);
    resolutions.sort_unstable();
    resolutions.dedup();

    if resolutions.is_empty() {
        resolutions.push(original_width as i32);
    }

    resolutions
}

fn replace_file(path: &Path, contents: &[u8]) -> Result<(), std::io::Error> {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image");
    let temporary_path = path.with_file_name(format!(".{filename}.{}.tmp", Uuid::new_v4()));

    let result = (|| {
        File::create(&temporary_path)?.write_all(contents)?;
        fs::rename(&temporary_path, path)
    })();

    if result.is_err() {
        let _ = fs::remove_file(temporary_path);
    }

    result
}

fn save_animated_gif(
    image_resolutions: Vec<i32>,
    image_types: &[String],
    input_file: &Path,
    tx: Option<&Sender<String>>,
) -> Result<Option<VarianceType>, Box<dyn std::error::Error>> {
    if image::ImageReader::open(input_file)?
        .with_guessed_format()?
        .format()
        != Some(ImageFormat::Gif)
    {
        return Ok(None);
    }

    let decoder = GifDecoder::new(BufReader::new(File::open(input_file)?))?;
    let mut frames = decoder.into_frames();
    let Some(first) = frames.next().transpose()? else {
        return Err("GIF contains no frames".into());
    };
    let Some(_second) = frames.next().transpose()? else {
        return Ok(None);
    };

    if !image_types.iter().any(|extension| extension == "webp") {
        return Ok(Some(Vec::new()));
    }

    let (original_width, original_height) = first.buffer().dimensions();
    let pixels_per_frame = u64::from(original_width) * u64::from(original_height);

    if pixels_per_frame == 0 || pixels_per_frame > *MAX_IMAGE_PIXELS {
        return Err(format!("Image exceeds the maximum of {} pixels", *MAX_IMAGE_PIXELS).into());
    }

    let resolutions = normalized_resolutions(image_resolutions, original_width);
    let image_name = input_file
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("image");
    let mut outputs = Vec::with_capacity(resolutions.len());

    for width in resolutions {
        let height =
            ((original_height as f64 * width as f64 / original_width as f64).round() as u32).max(1);
        let filename = format!("{image_name}-{width}.webp");
        let output_path = input_file.with_file_name(&filename);
        let decoder = GifDecoder::new(BufReader::new(File::open(input_file)?))?;
        let loop_count = decoder.loop_count();
        let frames = decoder.into_frames();
        let mut encoder = AnimatedWebpEncoder::new(width as u32, height, loop_count)?;
        let mut timestamp_ms = 0;
        let mut frame_count = 0usize;
        let max_total_pixels = MAX_IMAGE_PIXELS.saturating_mul(MAX_ANIMATED_IMAGE_PIXEL_FACTOR);

        for frame in frames {
            let frame = frame?;
            frame_count += 1;

            if frame_count > MAX_ANIMATED_IMAGE_FRAMES {
                return Err(format!(
                    "Animated image exceeds the maximum of {MAX_ANIMATED_IMAGE_FRAMES} frames"
                )
                .into());
            }

            if pixels_per_frame.saturating_mul(frame_count as u64) > max_total_pixels {
                return Err(format!(
                    "Animated image exceeds the processing limit of {max_total_pixels} total frame pixels"
                )
                .into());
            }

            let resized = if frame.buffer().dimensions() == (width as u32, height) {
                frame.buffer().clone()
            } else {
                image::imageops::resize(frame.buffer(), width as u32, height, Triangle)
            };
            encoder.add_frame(&resized, timestamp_ms)?;

            timestamp_ms = timestamp_ms
                .checked_add(frame_delay_ms(&frame)?)
                .ok_or("Animated image duration is too large")?;

            if timestamp_ms > MAX_ANIMATED_IMAGE_DURATION_MS {
                return Err(format!(
                    "Animated image exceeds the maximum duration of {} seconds",
                    MAX_ANIMATED_IMAGE_DURATION_MS / 1_000
                )
                .into());
            }
        }

        let buffer = encoder.finish(timestamp_ms)?;
        replace_file(&output_path, &buffer)?;
        outputs.push((width, height as i32, filename.clone()));

        match tx {
            Some(tx) => {
                let message = SSEMessage::new(Level::Success, &format!("Created: '{filename}'"));
                let _ = tx.send(message.to_string());
            }
            None => info!("Created: '{filename}'"),
        }
    }

    outputs.sort_by_key(|(width, _, _)| *width);

    Ok(Some(outputs))
}

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

        if len == 0 || out_buf.is_null() {
            return Err("WebP encoding failed".into());
        }

        let encoded = std::slice::from_raw_parts(out_buf, len as usize).to_vec();
        WebPFree(out_buf.cast());
        Ok(encoded)
    }
}

pub fn save_image(
    mut image_resolutions: Vec<i32>,
    image_types: &[String],
    input_file: &Path,
    tx: Option<Sender<String>>,
) -> Result<VarianceType, Box<dyn std::error::Error>> {
    if let Some(variants) = save_animated_gif(
        image_resolutions.clone(),
        image_types,
        input_file,
        tx.as_ref(),
    )? {
        return Ok(variants);
    }

    let dimensions = image::ImageReader::open(input_file)?
        .with_guessed_format()?
        .into_dimensions()?;
    let pixels = u64::from(dimensions.0) * u64::from(dimensions.1);
    if pixels == 0 || pixels > *MAX_IMAGE_PIXELS {
        return Err(format!("Image exceeds the maximum of {} pixels", *MAX_IMAGE_PIXELS).into());
    }
    let img = image::open(input_file)?;
    let (orig_w, orig_h) = img.dimensions();
    let img_name = input_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("image");

    image_resolutions.retain(|width| *width > 0);
    image_resolutions.sort_unstable();
    image_resolutions.dedup();

    // Do not upscale images. Without a usable configured size, retain one
    // variant at the natural width instead of making image uploads and video
    // posters fail processing.
    if image_resolutions.is_empty()
        || !image_resolutions
            .iter()
            .any(|width| *width <= orig_w as i32)
    {
        image_resolutions.push(orig_w as i32);
        image_resolutions.sort_unstable();
    }

    let mut variants = Vec::new();

    for in_w in image_resolutions {
        if orig_w < in_w as u32 {
            continue;
        }

        let scale = in_w as f32 / orig_w as f32;
        let in_h = (orig_h as f32 * scale).round() as u32;

        let (resized, w, h) = if orig_w as i32 == in_w {
            (img.clone(), orig_w, orig_h)
        } else {
            let r = img.resize_exact(in_w as u32, in_h, Triangle);
            let (w, h) = r.dimensions();

            (r, w, h)
        };

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
                    let encoder = AvifEncoder::new_with_speed_quality(&mut buffer, 5, 60);
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

pub async fn delete_image(size: &(u32, u32), path: &Path, name: &str) -> Result<(), NurError> {
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

#[cfg(test)]
mod tests {
    use std::{fs::File, io::BufReader, time::Duration};

    use image::{
        AnimationDecoder, Delay, Frame, GenericImageView, Rgba, RgbaImage,
        codecs::{
            gif::{GifEncoder, Repeat},
            webp::WebPDecoder,
        },
        metadata::LoopCount,
    };

    use super::save_image;

    #[test]
    fn generates_only_configured_image_variants_without_upscaling() {
        let directory =
            std::env::temp_dir().join(format!("nur-cms-image-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("test directory can be created");
        let source = directory.join("source.png");
        RgbaImage::from_pixel(640, 360, Rgba([20, 40, 60, 255]))
            .save(&source)
            .expect("source image can be written");

        let variants = save_image(vec![160, 1_280], &["png".to_string()], &source, None)
            .expect("image variants can be generated");
        assert_eq!(variants, vec![(160, 90, "source-160.png".into()),]);

        for (width, height, filename) in &variants {
            let generated =
                image::open(directory.join(filename)).expect("generated image can be decoded");
            assert_eq!(generated.dimensions(), (*width as u32, *height as u32));
        }

        std::fs::remove_dir_all(directory).expect("test directory can be removed");
    }

    #[test]
    fn generates_a_natural_size_variant_when_the_image_is_smaller_than_every_configuration() {
        let directory =
            std::env::temp_dir().join(format!("nur-cms-small-image-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("test directory can be created");
        let source = directory.join("source.png");
        RgbaImage::from_pixel(120, 60, Rgba([20, 40, 60, 255]))
            .save(&source)
            .expect("source image can be written");

        let variants = save_image(vec![480, 1_280], &["webp".to_string()], &source, None)
            .expect("small image variant can be generated");
        assert_eq!(variants, vec![(120, 60, "source-120.webp".into())]);

        std::fs::remove_dir_all(directory).expect("test directory can be removed");
    }

    #[test]
    fn generates_an_original_size_variant_without_configured_resolutions() {
        let directory =
            std::env::temp_dir().join(format!("nur-cms-no-sizes-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("test directory can be created");
        let source = directory.join("source.png");
        RgbaImage::from_pixel(100, 50, Rgba([20, 40, 60, 255]))
            .save(&source)
            .expect("source image can be written");

        let variants = save_image(Vec::new(), &["webp".to_string()], &source, None)
            .expect("fallback image variant can be generated");
        assert_eq!(variants, vec![(100, 50, "source-100.webp".into())]);

        std::fs::remove_dir_all(directory).expect("test directory can be removed");
    }

    #[test]
    fn preserves_gif_animation_in_webp_variants() {
        let directory =
            std::env::temp_dir().join(format!("nur-cms-animated-gif-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("test directory can be created");
        let source = directory.join("source.gif");
        let file = File::create(&source).expect("GIF can be created");
        let mut encoder = GifEncoder::new(file);
        encoder
            .set_repeat(Repeat::Finite(3))
            .expect("GIF loop count can be set");
        let frames = [
            Frame::from_parts(
                RgbaImage::from_pixel(64, 32, Rgba([255, 0, 0, 255])),
                0,
                0,
                Delay::from_numer_denom_ms(40, 1),
            ),
            Frame::from_parts(
                RgbaImage::from_pixel(64, 32, Rgba([0, 0, 255, 255])),
                0,
                0,
                Delay::from_numer_denom_ms(60, 1),
            ),
        ];
        encoder
            .encode_frames(frames)
            .expect("animated GIF can be encoded");
        drop(encoder);

        let static_variants = save_image(vec![32], &["jpg".to_string()], &source, None)
            .expect("animated GIF can remain without generated variants");
        assert!(static_variants.is_empty());
        assert!(!directory.join("source-32.jpg").exists());

        RgbaImage::from_pixel(16, 8, Rgba([255, 0, 0, 255]))
            .save(directory.join("source-16.webp"))
            .expect("existing static WebP variant can be written");

        let variants = save_image(vec![16, 32], &["webp".to_string()], &source, None)
            .expect("animated WebP variant can be generated");
        assert_eq!(
            variants,
            vec![
                (16, 8, "source-16.webp".into()),
                (32, 16, "source-32.webp".into()),
            ]
        );

        let output = File::open(directory.join("source-16.webp")).expect("variant exists");
        let decoder = WebPDecoder::new(BufReader::new(output)).expect("variant is valid WebP");
        assert!(decoder.has_animation());
        assert!(matches!(
            decoder.loop_count(),
            LoopCount::Finite(count) if count.get() == 3
        ));
        let decoded_frames = decoder
            .into_frames()
            .collect_frames()
            .expect("animated WebP frames can be decoded");
        assert_eq!(decoded_frames.len(), 2);
        assert_eq!(
            Duration::from(decoded_frames[0].delay()),
            Duration::from_millis(40)
        );
        assert_eq!(
            Duration::from(decoded_frames[1].delay()),
            Duration::from_millis(60)
        );

        std::fs::remove_dir_all(directory).expect("test directory can be removed");
    }
}
