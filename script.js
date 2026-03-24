/**
 * Batch-convert images under ./input to WebP under ./output, preserving folder layout.
 *
 * - Resizes so the longest side is at most MAX_SIZE (keeps aspect ratio).
 * - Keeps embedded metadata (EXIF, etc.) via sharp.keepMetadata().
 * - After writing each file, copies the source file’s timestamps onto the output
 *   (creation / modification / access) so Finder and similar tools show the same
 *   dates as the originals. Uses the `utimes` package when available so birth time
 *   can be set on macOS; falls back to fs.utimes otherwise.
 *
 * Usage: node script.js
 */

import { promises as fs } from 'fs';
import path from 'path';
import sharp from 'sharp';
import { utimes as setFileTimes } from 'utimes';

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

const CONFIG = {
  INPUT_DIR: './input',
  OUTPUT_DIR: './output',
  MAX_SIZE: 2048,
  QUALITY: 80,
  SUPPORTED_EXTS: ['.jpg', '.jpeg', '.png', '.gif', '.bmp', '.webp']
};

const BATCH_SIZE = 4;

// ---------------------------------------------------------------------------
// Filesystem helpers
// ---------------------------------------------------------------------------

/**
 * The `utimes` package requires whole-number milliseconds; Node’s `*Ms` stats
 * can include fractional values.
 */
function msInt(ms) {
  return Math.floor(Number(ms));
}

/**
 * Apply the input file’s birth, modification, and access times to the output
 * file so the converted image matches the original’s dates in the file manager.
 */
async function copyInputTimestampsToOutput(outputPath, inputPath) {
  const stat = await fs.stat(inputPath);
  const birthMs = stat.birthtimeMs ?? stat.mtimeMs;

  try {
    await setFileTimes(outputPath, {
      btime: msInt(birthMs),
      mtime: msInt(stat.mtimeMs),
      atime: msInt(stat.atimeMs)
    });
  } catch {
    await fs.utimes(outputPath, stat.atime, stat.mtime);
  }
}

// ---------------------------------------------------------------------------
// Discovery & paths
// ---------------------------------------------------------------------------

function isSupportedImage(filename) {
  const ext = path.extname(filename).toLowerCase();
  return CONFIG.SUPPORTED_EXTS.includes(ext);
}

/**
 * Recursively lists supported images under `dir`.
 * `baseDir` is the root used to compute paths relative to the input folder.
 */
async function findImages(dir, baseDir = dir) {
  const entries = await fs.readdir(dir, { withFileTypes: true });
  const images = [];

  for (const entry of entries) {
    const fullPath = path.join(dir, entry.name);

    if (entry.isDirectory()) {
      images.push(...(await findImages(fullPath, baseDir)));
    } else if (entry.isFile() && isSupportedImage(entry.name)) {
      images.push({
        inputPath: fullPath,
        relativePath: path.relative(baseDir, fullPath)
      });
    }
  }

  return images;
}

function outputPathFor(relativePath) {
  const dir = path.join(CONFIG.OUTPUT_DIR, path.dirname(relativePath));
  const base = path.basename(relativePath, path.extname(relativePath));
  return {
    dir,
    filePath: path.join(dir, `${base}.webp`)
  };
}

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------

async function processImage({ inputPath, relativePath }) {
  try {
    const metadata = await sharp(inputPath).metadata();

    let pipeline = sharp(inputPath).keepMetadata();
    const tooLarge =
      metadata.width > CONFIG.MAX_SIZE || metadata.height > CONFIG.MAX_SIZE;

    if (tooLarge) {
      pipeline = pipeline.resize({
        width: Math.min(CONFIG.MAX_SIZE, metadata.width),
        height: Math.min(CONFIG.MAX_SIZE, metadata.height),
        fit: 'inside',
        withoutEnlargement: true
      });
    }

    const { dir: outputDir, filePath: outputPath } = outputPathFor(relativePath);

    await fs.mkdir(outputDir, { recursive: true });
    await pipeline.webp({ quality: CONFIG.QUALITY }).toFile(outputPath);
    await copyInputTimestampsToOutput(outputPath, inputPath);

    return {
      success: true,
      input: inputPath,
      output: outputPath,
      originalSize: `${metadata.width}x${metadata.height}`
    };
  } catch (error) {
    return {
      success: false,
      input: inputPath,
      error: error.message
    };
  }
}

// ---------------------------------------------------------------------------
// Entry
// ---------------------------------------------------------------------------

async function main() {
  console.log('🔍 Scanning for images in:', CONFIG.INPUT_DIR);

  try {
    await fs.access(CONFIG.INPUT_DIR);
  } catch {
    console.error('❌ Input directory does not exist:', CONFIG.INPUT_DIR);
    process.exit(1);
  }

  const images = await findImages(CONFIG.INPUT_DIR);
  console.log(`📁 Found ${images.length} images across subfolders`);

  if (images.length === 0) {
    console.log('No supported images found.');
    return;
  }

  const results = [];

  for (let i = 0; i < images.length; i += BATCH_SIZE) {
    const batch = images.slice(i, i + BATCH_SIZE);
    const batchResults = await Promise.all(batch.map(processImage));
    results.push(...batchResults);

    batchResults.forEach((result, idx) => {
      const n = i + idx + 1;
      const icon = result.success ? '✅' : '❌';
      console.log(`[${n}/${images.length}] ${icon} ${batch[idx].relativePath}`);
    });
  }

  const ok = results.filter((r) => r.success).length;
  console.log(`\n📊 Summary: ${ok}/${results.length} processed successfully`);
  console.log(`Output location: ${CONFIG.OUTPUT_DIR}`);
}

main().catch(console.error);
