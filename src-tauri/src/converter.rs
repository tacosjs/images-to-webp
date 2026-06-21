use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use filetime::FileTime;
use image::imageops::FilterType;
use serde::{Deserialize, Serialize};
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
    let original_size = std::fs::metadata(input).map(|m| m.len()).unwrap_or(0);

    // Open and optionally resize
    let img = image::open(input)
        .with_context(|| format!("failed to open {}", input.display()))?;

    let img = resize_if_needed(img, config.max_size);

    // Encode to WebP
    let encoder = webp::Encoder::from_image(&img)
        .map_err(|e| anyhow::anyhow!("webp encoder error: {e}"))?;
    let webp_data = encoder.encode(config.quality as f32);

    // Compute output path (mirrors source tree, swaps extension to .webp)
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

    std::fs::write(&output_path, &*webp_data)
        .with_context(|| format!("failed to write {}", output_path.display()))?;

    // Inject EXIF from source into output WebP where possible
    let _ = transfer_exif(input, &output_path);

    // Copy mtime/atime from source to output
    copy_timestamps(input, &output_path);

    let output_size = std::fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);
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

/// Read EXIF from JPEG source and write it into the output WebP's EXIF chunk.
/// Silently skips sources that have no EXIF or are not JPEG/TIFF.
fn transfer_exif(src: &Path, dst: &Path) -> Result<()> {
    let src_file = std::fs::File::open(src)?;
    let mut reader = BufReader::new(src_file);
    let exif = exif::Reader::new().read_from_container(&mut reader)?;

    // Re-read raw bytes for the EXIF chunk
    let src_bytes = std::fs::read(src)?;
    let raw_exif: Option<Vec<u8>> = find_jpeg_exif_bytes(&src_bytes);

    if let Some(exif_bytes) = raw_exif {
        let dst_bytes = std::fs::read(dst)?;
        let patched = inject_exif_into_webp(&dst_bytes, &exif_bytes)?;
        std::fs::write(dst, patched)?;
    }

    let _ = exif; // suppress unused warning
    Ok(())
}

/// Extract the raw APP1/EXIF segment from a JPEG byte stream.
fn find_jpeg_exif_bytes(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return None; // not JPEG
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

/// Inject raw EXIF bytes into a WebP file by inserting an EXIF chunk into the RIFF container.
fn inject_exif_into_webp(webp: &[u8], exif: &[u8]) -> Result<Vec<u8>> {
    // Validate RIFF/WEBP header
    if webp.len() < 12 || &webp[0..4] != b"RIFF" || &webp[8..12] != b"WEBP" {
        anyhow::bail!("not a valid WebP file");
    }

    let mut out = Vec::with_capacity(webp.len() + 8 + exif.len() + 1);

    // Copy RIFF header (12 bytes), then all existing chunks, then append EXIF chunk
    out.extend_from_slice(&webp[0..12]);

    let mut chunks: Vec<u8> = webp[12..].to_vec();

    // Build EXIF chunk: FourCC "EXIF" + LE u32 size + data (padded to even)
    let chunk_size = exif.len();
    let mut exif_chunk = Vec::with_capacity(8 + chunk_size + (chunk_size & 1));
    exif_chunk.extend_from_slice(b"EXIF");
    exif_chunk.extend_from_slice(&(chunk_size as u32).to_le_bytes());
    exif_chunk.extend_from_slice(exif);
    if chunk_size & 1 == 1 {
        exif_chunk.push(0); // padding byte
    }

    chunks.extend_from_slice(&exif_chunk);
    out.extend_from_slice(&chunks);

    // Fix the RIFF file size field (bytes 4..8 = total file size − 8)
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
