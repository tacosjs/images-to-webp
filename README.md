# SmallerPixels

A minimalist desktop app to batch-convert photos to WebP, while preserving original metadata and file dates. Built with [Tauri v2](https://tauri.app) and React, targeting macOS first with Android on the roadmap.

![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)
![Platform: macOS](https://img.shields.io/badge/platform-macOS-lightgrey.svg)
![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%20v2-orange.svg)

---

## Why

Phone cameras produce large JPEG files. WebP cuts file size by 70–90% at comparable quality, making it ideal for archiving, backup, and sharing. This app handles the conversion automatically; either as a one-shot batch or as a background watcher that processes new photos as they arrive.

---

## Features

- **Manual mode** — drag and drop folders or use the folder picker, choose an output directory, convert
- **Watch mode** — point at a source folder; the app watches for new images using filesystem events and runs a periodic sweep as a fallback
- **EXIF preserved** — metadata (camera model, GPS, date taken) is extracted and re-injected into the WebP output
- **Timestamps preserved** — output files keep the original modification and access times so file managers show the correct dates
- **Resize on conversion** — images exceeding a configurable max dimension are scaled down; smaller images are never upscaled
- **Parallel processing** — conversions run concurrently, bounded by CPU core count
- **Live progress** — spinner, elapsed time, per-file ETA, and an animated progress bar
- **Conversion log** — a dedicated Log tab shows per-batch history with compression stats (`4.2 MB → 312 KB · −93%`)

---

## Screenshots

> _TODO: add screenshots once the UI is stable_

---

## Getting started

```bash
npm install
npm run tauri dev
```

The first build compiles Rust dependencies and will take a few minutes. Subsequent incremental builds are fast.

```bash
# Production build — produces a signed .app and .dmg
npm run tauri build
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for prerequisites, project structure, and architecture details.

---

## Configuration

Settings are persisted per installation and adjustable in the UI at any time:

| Setting       | Default     | Description                                               |
| ------------- | ----------- | --------------------------------------------------------- |
| Max dimension | 2048 px     | Longest side limit — images below this are never upscaled |
| WebP quality  | 80          | Encoder quality (0 = smallest file, 100 = lossless)       |
| Watch trigger | File events | When to process new files in Watch mode                   |

---

## Roadmap

### Near-term

- [ ] Windows and Linux support (Tauri cross-compiles; main gap is the macOS-only folder picker)
- [ ] Drag-and-drop support for individual files in addition to folders
- [ ] Output format options beyond WebP (AVIF, optimized JPEG)
- [ ] Flat vs. mirrored folder structure in output

### Mobile — Android

The primary mobile target is **automatic photo backup with compression**:

- Watch the device camera folder (`DCIM/`) for new photos
- Convert to WebP in the background via an Android foreground service (persistent notification, survives app close)
- Push the optimized file directly to a privacy-respecting cloud provider:
  - **[Ente Photos](https://ente.io)** — open-source, end-to-end encrypted photo storage
  - **[Proton Drive](https://proton.me/drive)** — end-to-end encrypted cloud storage
  - Generic WebDAV for self-hosted setups (Nextcloud, Immich, etc.)

The goal is a lightweight, privacy-first alternative to Google Photos backup — originals stay on-device, compressed WebP copies sync automatically to a provider of your choice.

### Mobile — iOS

iOS support is planned after Android. It requires integrating with the Photos framework (`PHPhotoLibrary`) due to sandboxing, and is more constrained around background execution.

---

## Contributing

Pull requests are welcome. Please open an issue first for anything beyond small fixes. See [CONTRIBUTING.md](CONTRIBUTING.md) for the full guide.

---

## License

MIT — see [LICENSE](LICENSE).
