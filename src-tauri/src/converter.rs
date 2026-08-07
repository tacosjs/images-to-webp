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

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, RgbImage};
    use std::io::Cursor;
    use std::time::Duration;

    // ── is_supported_image ────────────────────────────────────────────────────

    #[test]
    fn supported_extensions_are_accepted() {
        for ext in &["jpg", "jpeg", "png", "gif", "bmp", "webp"] {
            let p = Path::new("file").with_extension(ext);
            assert!(is_supported_image(&p), "{ext} should be supported");
        }
    }

    #[test]
    fn unsupported_extensions_are_rejected() {
        for ext in &["txt", "pdf", "mp4", "tiff", "svg"] {
            let p = Path::new("file").with_extension(ext);
            assert!(!is_supported_image(&p), "{ext} should not be supported");
        }
    }

    #[test]
    fn extension_check_is_case_insensitive() {
        assert!(is_supported_image(Path::new("photo.JPG")));
        assert!(is_supported_image(Path::new("photo.PNG")));
        assert!(is_supported_image(Path::new("photo.Jpeg")));
    }

    #[test]
    fn missing_extension_is_rejected() {
        assert!(!is_supported_image(Path::new("Makefile")));
    }

    // ── resize_if_needed ─────────────────────────────────────────────────────

    fn blank(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::new(w, h))
    }

    #[test]
    fn small_image_is_not_resized() {
        let result = resize_if_needed(blank(800, 600), 2048);
        assert_eq!((result.width(), result.height()), (800, 600));
    }

    #[test]
    fn image_at_exact_limit_is_not_resized() {
        let result = resize_if_needed(blank(2048, 1024), 2048);
        assert_eq!((result.width(), result.height()), (2048, 1024));
    }

    #[test]
    fn landscape_is_constrained_by_width() {
        let result = resize_if_needed(blank(4000, 2000), 2048);
        assert_eq!((result.width(), result.height()), (2048, 1024));
    }

    #[test]
    fn portrait_is_constrained_by_height() {
        let result = resize_if_needed(blank(2000, 4000), 2048);
        assert_eq!((result.width(), result.height()), (1024, 2048));
    }

    #[test]
    fn square_image_respects_max_size() {
        let result = resize_if_needed(blank(4096, 4096), 2048);
        assert_eq!((result.width(), result.height()), (2048, 2048));
    }

    #[test]
    fn small_image_is_never_upscaled() {
        let result = resize_if_needed(blank(100, 100), 2048);
        assert_eq!((result.width(), result.height()), (100, 100));
    }

    // ── find_jpeg_exif_bytes ─────────────────────────────────────────────────

    /// Build a minimal JPEG with an APP1/Exif segment containing `payload`.
    fn jpeg_with_exif(payload: &[u8]) -> Vec<u8> {
        let segment_data: Vec<u8> = [b"Exif\0\0".as_slice(), payload].concat();
        let seg_len = (segment_data.len() + 2) as u16; // includes 2-byte length field
        let mut out = vec![0xFF, 0xD8, 0xFF, 0xE1];
        out.extend_from_slice(&seg_len.to_be_bytes());
        out.extend_from_slice(&segment_data);
        out
    }

    #[test]
    fn extracts_exif_payload_from_jpeg() {
        let payload = b"fake exif bytes";
        assert_eq!(
            find_jpeg_exif_bytes(&jpeg_with_exif(payload)).as_deref(),
            Some(payload.as_slice())
        );
    }

    #[test]
    fn returns_none_for_jpeg_without_exif() {
        // APP0 (JFIF) marker — not an EXIF segment
        let mut data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        data.extend_from_slice(&[0u8; 14]); // 14 bytes of filler to match len=16
        assert_eq!(find_jpeg_exif_bytes(&data), None);
    }

    #[test]
    fn returns_none_for_non_jpeg_bytes() {
        assert_eq!(find_jpeg_exif_bytes(b"\x89PNG\r\n"), None);
        assert_eq!(find_jpeg_exif_bytes(b""), None);
    }

    // ── inject_exif_into_webp ─────────────────────────────────────────────────

    /// Build a minimal but structurally valid RIFF/WEBP container.
    fn minimal_webp() -> Vec<u8> {
        let chunk_data = vec![0u8; 4];
        let mut body: Vec<u8> = b"WEBP".to_vec();
        body.extend_from_slice(b"VP8L");
        body.extend_from_slice(&(chunk_data.len() as u32).to_le_bytes());
        body.extend_from_slice(&chunk_data);

        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn injects_exif_chunk_and_keeps_riff_header() {
        let result = inject_exif_into_webp(&minimal_webp(), b"fake exif").unwrap();
        assert_eq!(&result[0..4], b"RIFF");
        assert_eq!(&result[8..12], b"WEBP");
        let has_exif = result[12..].windows(4).any(|w| w == b"EXIF");
        assert!(has_exif, "EXIF chunk must be present in output");
    }

    #[test]
    fn riff_size_field_matches_output_length() {
        let result = inject_exif_into_webp(&minimal_webp(), b"fake exif").unwrap();
        let reported = u32::from_le_bytes(result[4..8].try_into().unwrap()) as usize;
        assert_eq!(reported, result.len() - 8);
    }

    #[test]
    fn odd_length_exif_is_padded_to_even_boundary() {
        let result = inject_exif_into_webp(&minimal_webp(), b"odd").unwrap(); // 3 bytes
        // "EXIF" + 4-byte size + 3 bytes data + 1 pad = 12 bytes; total must be even
        assert_eq!(result.len() % 2, 0);
    }

    #[test]
    fn rejects_invalid_webp_input() {
        assert!(inject_exif_into_webp(b"not a webp file!!!", b"x").is_err());
        assert!(inject_exif_into_webp(b"", b"x").is_err());
    }

    // ── find_images ───────────────────────────────────────────────────────────

    #[test]
    fn finds_images_recursively_skipping_other_files() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();

        std::fs::write(dir.path().join("a.jpg"), b"").unwrap();
        std::fs::write(sub.join("b.png"), b"").unwrap();
        std::fs::write(dir.path().join("readme.txt"), b"").unwrap();

        let found = find_images(dir.path(), None);
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn find_images_returns_empty_when_no_images_present() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("data.json"), b"").unwrap();
        assert!(find_images(dir.path(), None).is_empty());
    }

    #[test]
    fn find_images_since_excludes_files_older_than_cutoff() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("old.jpg"), b"").unwrap();

        // Cutoff is 1 hour in the future — existing file won't match
        let future = SystemTime::now() + Duration::from_secs(3600);
        assert!(find_images(dir.path(), Some(future)).is_empty());

        // Without a cutoff the same file is found
        assert_eq!(find_images(dir.path(), None).len(), 1);
    }

    // ── convert_image (integration) ───────────────────────────────────────────

    fn make_png_bytes() -> Vec<u8> {
        let img = image::RgbImage::from_fn(64, 64, |x, y| {
            image::Rgb([(x * 4) as u8, (y * 4) as u8, 128])
        });
        let mut buf = Vec::new();
        DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        buf
    }

    #[test]
    fn convert_image_produces_webp_file() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("test.png");
        let out_dir = dir.path().join("out");
        std::fs::create_dir(&out_dir).unwrap();
        std::fs::write(&input, make_png_bytes()).unwrap();

        let result = convert_image(&input, dir.path(), &out_dir, &ConversionConfig::default());

        assert!(result.success, "conversion failed: {:?}", result.error);
        assert!(result.output.ends_with(".webp"), "output should be .webp");
        assert!(Path::new(&result.output).exists(), "output file should exist");
        assert!(result.original_size.is_some());
        assert!(result.output_size.is_some());
    }

    #[test]
    fn convert_image_mirrors_directory_structure() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("vacation");
        std::fs::create_dir(&sub).unwrap();
        let input = sub.join("photo.png");
        let out_dir = dir.path().join("out");
        std::fs::create_dir(&out_dir).unwrap();
        std::fs::write(&input, make_png_bytes()).unwrap();

        let result = convert_image(&input, dir.path(), &out_dir, &ConversionConfig::default());

        assert!(result.success);
        // Output should mirror: out/vacation/photo.webp
        let expected = out_dir.join("vacation").join("photo.webp");
        assert_eq!(Path::new(&result.output), expected);
    }

    #[test]
    fn convert_image_fails_gracefully_on_bad_input() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("corrupt.png");
        std::fs::write(&input, b"this is not an image").unwrap();

        let result = convert_image(&input, dir.path(), dir.path(), &ConversionConfig::default());

        assert!(!result.success);
        assert!(result.error.is_some());
    }
}
