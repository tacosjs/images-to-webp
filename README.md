# Resize camera photos for storage

Node script: reads images under `input/`, writes WebP under `output/` (same subfolders, `.webp` names). Large images are scaled so the longest side is at most **2048 px**; smaller ones are converted without upscaling.

**Run:** `npm install`, then `node script.js`.

Tweak paths, max size, quality, and formats in `CONFIG` at the top of `script.js`.
