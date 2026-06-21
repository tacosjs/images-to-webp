use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::{Context, Result};
use filetime::FileTime;
use image::imageops::FilterType;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::task::JoinSet;
use walkdir::WalkDir;

const SUPPORTED_EXTS: &[&str] = &["jpg", "jpeg", "png", "gif", "bmp", "webp"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionConfig {
    pub max_size: u32,
    pub quality: u8,
}

impl Default for ConversionConfig {
    fn default() -> Self {
        Self {
            max_size: 2048,
            quality: 80,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionResult {
    pub success: bool,
    pub input: String,
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn is_supported_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_EXTS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Recursively find all supported images under `dir` that are newer than `since`.
pub fn find_images(dir: &Path, since: Option<SystemTime>) -> Vec<PathBuf> {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| is_supported_image(e.path()))
        .filter(|e| {
            if let Some(since) = since {
                e.metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .map(|mtime| mtime > since)
                    .unwrap_or(false)
            } else {
                true
            }
        })
        .map(|e| e.into_path())
        .collect()
}

/// Process a list of `(image_path, source_root)` pairs in parallel, emitting
/// Tauri events for each file start, completion, and the final summary.
///
/// Concurrency is bounded to the number of logical CPU cores (max 8).
pub async fn convert_batch_parallel(
    images: Vec<(PathBuf, PathBuf)>,
    output_dir: PathBuf,
    config: ConversionConfig,
    app: AppHandle,
) -> Vec<ConversionResult> {
    let total = images.len();
    let concurrency = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(8);

    let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));
    let completed_count = Arc::new(AtomicUsize::new(0));
    let mut set: JoinSet<(usize, ConversionResult)> = JoinSet::new();

    for (i, (image, source_root)) in images.into_iter().enumerate() {
        let permit = Arc::clone(&semaphore).acquire_owned().await.unwrap();
        let app = app.clone();
        let output_dir = output_dir.clone();
        let config = config.clone();
        let counter = Arc::clone(&completed_count);

        set.spawn_blocking(move || {
            let _permit = permit; // released when this closure returns

            app.emit(
                "conversion:file-start",
                serde_json::json!({ "file": image.to_string_lossy(), "index": i }),
            )
            .ok();

            let result = convert_image(&image, &source_root, &output_dir, &config);
            let done = counter.fetch_add(1, Ordering::Relaxed) + 1;

            app.emit(
                "conversion:progress",
                serde_json::json!({
                    "file": image.to_string_lossy(),
                    "index": i,
                    "completed": done,
                    "total": total,
                    "success": result.success,
                    "originalSize": result.original_size.unwrap_or(0),
                    "outputSize": result.output_size.unwrap_or(0),
                }),
            )
            .ok();

            (i, result)
        });
    }

    let mut indexed: Vec<(usize, ConversionResult)> = Vec::with_capacity(total);
    while let Some(res) = set.join_next().await {
        if let Ok(pair) = res {
            indexed.push(pair);
        }
    }
    // Return results in original input order
    indexed.sort_by_key(|(i, _)| *i);
    indexed.into_iter().map(|(_, r)| r).collect()
}

/// Convert a single image to WebP, writing into `output_dir` mirroring the
/// directory structure relative to `source_root`.
pub fn convert_image(
    input: &Path,
    source_root: &Path,
    output_dir: &Path,
    config: &ConversionConfig,
) -> ConversionResult {
    let input_str = input.to_string_lossy().to_string();

    match do_convert(input, source_root, output_dir, config) {
        Ok((output_path, original_size, output_size)) => ConversionResult {
            success: true,
            input: input_str,
            output: output_path.to_string_lossy().to_string(),
            original_size: Some(original_size),
            output_size: Some(output_size),
            error: None,
        },
        Err(e) => ConversionResult {
            success: false,
            input: input_str,
            output: String::new(),
            original_size: None,
            output_size: None,
            error: Some(e.to_string()),
        },
    }
}

fn do_convert(
    input: &Path,
    source_root: &Path,
    output_dir: &Path,
    config: &ConversionConfig,
) -> Result<(PathBuf, u64, u64)> {
    // Read source bytes once — used for both decoding and EXIF extraction.
    let src_bytes = std::fs::read(input)
        .with_context(|| format!("failed to read {}", input.display()))?;
    let original_size = src_bytes.len() as u64;

    // Extract EXIF before handing bytes to the image decoder.
    let exif_bytes = find_jpeg_exif_bytes(&src_bytes);

    // Decode image.
    let img = image::load_from_memory(&src_bytes)
        .with_context(|| format!("failed to decode {}", input.display()))?;
    drop(src_bytes); // free ~10 MB of phone photo RAM now

    let img = resize_if_needed(img, config.max_size);

    // Encode to WebP in memory.
    let encoder = webp::Encoder::from_image(&img)
        .map_err(|e| anyhow::anyhow!("webp encoder error: {e}"))?;
    let webp_data = encoder.encode(config.quality as f32);

    // Optionally inject EXIF — stays in memory, no extra disk round-trip.
    let final_bytes: Vec<u8> = if let Some(exif) = exif_bytes {
        inject_exif_into_webp(&*webp_data, &exif).unwrap_or_else(|_| webp_data.to_vec())
    } else {
        webp_data.to_vec()
    };

    // Compute output path (mirrors source tree, .webp extension).
    let rel = input
        .strip_prefix(source_root)
        .unwrap_or_else(|_| Path::new(input.file_name().unwrap_or_default()));
    let output_path = {
        let stem = rel.file_stem().unwrap_or_default().to_string_lossy();
        let parent = rel.parent().unwrap_or(Path::new(""));
        output_dir.join(parent).join(format!("{stem}.webp"))
    };

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create dir {}", parent.display()))?;
    }

    // Single write to disk.
    let output_size = final_bytes.len() as u64;
    std::fs::write(&output_path, &final_bytes)
        .with_context(|| format!("failed to write {}", output_path.display()))?;

    // Copy mtime/atime from source to output.
    copy_timestamps(input, &output_path);

    Ok((output_path, original_size, output_size))
}

fn resize_if_needed(img: image::DynamicImage, max_size: u32) -> image::DynamicImage {
    let (w, h) = (img.width(), img.height());
    if w <= max_size && h <= max_size {
        return img;
    }
    let (nw, nh) = if w >= h {
        (max_size, (h as f64 * max_size as f64 / w as f64).round() as u32)
    } else {
        ((w as f64 * max_size as f64 / h as f64).round() as u32, max_size)
    };
    img.resize(nw, nh, FilterType::Lanczos3)
}

/// Extract the raw EXIF payload from a JPEG APP1 segment.
fn find_jpeg_exif_bytes(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }
    let mut i = 2usize;
    while i + 3 < data.len() {
        if data[i] != 0xFF {
            break;
        }
        let marker = data[i + 1];
        let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        if marker == 0xE1 && i + 2 + len <= data.len() {
            let segment = &data[i + 4..i + 2 + len];
            if segment.starts_with(b"Exif\0\0") {
                return Some(segment[6..].to_vec());
            }
        }
        i += 2 + len;
    }
    None
}

/// Insert raw EXIF bytes into a WebP RIFF container as an EXIF chunk.
fn inject_exif_into_webp(webp: &[u8], exif: &[u8]) -> Result<Vec<u8>> {
    if webp.len() < 12 || &webp[0..4] != b"RIFF" || &webp[8..12] != b"WEBP" {
        anyhow::bail!("not a valid WebP file");
    }

    let chunk_size = exif.len();
    let padded = chunk_size + (chunk_size & 1); // RIFF chunks are word-aligned
    let mut out = Vec::with_capacity(webp.len() + 8 + padded);

    out.extend_from_slice(&webp[0..12]); // RIFF header + WEBP
    out.extend_from_slice(&webp[12..]); // existing chunks

    // Append EXIF chunk
    out.extend_from_slice(b"EXIF");
    out.extend_from_slice(&(chunk_size as u32).to_le_bytes());
    out.extend_from_slice(exif);
    if chunk_size & 1 == 1 {
        out.push(0);
    }

    // Fix RIFF file size field
    let riff_size = (out.len() - 8) as u32;
    out[4..8].copy_from_slice(&riff_size.to_le_bytes());

    Ok(out)
}

fn copy_timestamps(src: &Path, dst: &Path) {
    if let Ok(meta) = std::fs::metadata(src) {
        let mtime = FileTime::from_last_modification_time(&meta);
        let atime = FileTime::from_last_access_time(&meta);
        let _ = filetime::set_file_times(dst, atime, mtime);
    }
}
