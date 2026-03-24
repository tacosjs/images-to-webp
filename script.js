import { promises as fs } from 'fs';
import path from 'path';
import sharp from 'sharp';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const CONFIG = {
  INPUT_DIR: './input',
  OUTPUT_DIR: './output',
  MAX_SIZE: 2048,
  QUALITY: 80,
  SUPPORTED_EXTS: ['.jpg', '.jpeg', '.png', '.gif', '.bmp', '.webp']
};

async function findImages(dir, baseDir = dir) {
  const entries = await fs.readdir(dir, { withFileTypes: true });
  const images = [];

  for (const entry of entries) {
    const fullPath = path.join(dir, entry.name);
    
    if (entry.isDirectory()) {
      images.push(...await findImages(fullPath, baseDir));
    } else if (entry.isFile() && 
              CONFIG.SUPPORTED_EXTS.includes(path.extname(entry.name).toLowerCase())) {
      images.push({
        inputPath: fullPath,
        relativePath: path.relative(baseDir, fullPath)
      });
    }
  }
  
  return images;
}

async function processImage(imageInfo) {
  const { inputPath, relativePath } = imageInfo;
  
  try {
    const metadata = await sharp(inputPath).metadata();
    
    // Calculate resize dimensions
    let resizeOptions = {};
    if (metadata.width > CONFIG.MAX_SIZE || metadata.height > CONFIG.MAX_SIZE) {
      resizeOptions = {
        width: Math.min(CONFIG.MAX_SIZE, metadata.width),
        height: Math.min(CONFIG.MAX_SIZE, metadata.height),
        fit: 'inside',
        withoutEnlargement: true
      };
    }

    // Create output path
    const outputDir = path.join(CONFIG.OUTPUT_DIR, path.dirname(relativePath));
    const outputName = path.basename(relativePath, path.extname(relativePath)) + '.webp';
    const outputPath = path.join(outputDir, outputName);

    // Ensure output directory exists
    await fs.mkdir(outputDir, { recursive: true });

    // Process with sharp
    await sharp(inputPath)
      .resize(resizeOptions)
      .webp({ quality: CONFIG.QUALITY })
      .toFile(outputPath);

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

  // Process with concurrency limit (adjust based on CPU cores)
  const CONCURRENCY = 4;
  const results = [];
  
  for (let i = 0; i < images.length; i += CONCURRENCY) {
    const batch = images.slice(i, i + CONCURRENCY);
    const batchResults = await Promise.all(batch.map(img => processImage(img)));
    results.push(...batchResults);
    
    batchResults.forEach((result, idx) => {
      const index = i + idx + 1;
      console.log(`[${index}/${images.length}] ${result.success ? '✅' : '❌'} ${batch[idx].relativePath}`);
    });
  }

  const successCount = results.filter(r => r.success).length;
  console.log(`\n📊 Summary: ${successCount}/${results.length} processed successfully`);
  console.log(`Output location: ${CONFIG.OUTPUT_DIR}`);
}

main().catch(console.error);