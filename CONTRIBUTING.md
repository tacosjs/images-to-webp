# Contributing

Thank you for your interest in contributing. This document covers prerequisites, project structure, how the conversion pipeline works, and the workflow for submitting changes.

---

## Prerequisites

| Tool                     | Version | Install                                                           |
| ------------------------ | ------- | ----------------------------------------------------------------- |
| Node.js                  | ≥ 20    | [nodejs.org](https://nodejs.org)                                  |
| Rust + Cargo             | stable  | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Xcode Command Line Tools | latest  | `xcode-select --install`                                          |
| Tauri CLI                | v2      | installed via `@tauri-apps/cli` in devDependencies                |

---

## Development setup

```bash
# Install JS dependencies
pnpm install

# Optional: verify Rust side compiles before starting the dev server
cd src-tauri && cargo check && cd ..

# Start the app in development mode
pnpm start
```

The first `tauri dev` compiles all Rust dependencies from scratch — expect 3–5 minutes. Subsequent incremental builds are fast (seconds) because only changed files recompile.

**Dev performance note:** `src-tauri/Cargo.toml` includes:

```toml
[profile.dev.package."*"]
opt-level = 3
```

This compiles all _dependencies_ at full optimization while leaving your own code in debug mode. It makes pixel-math and WebP encoding 10–20× faster in dev without sacrificing incremental rebuild speed for code you're actively changing.

### Production build

```bash
pnpm run tauri build
```

Produces a notarized `.app` bundle and a `.dmg` installer under `src-tauri/target/release/bundle/`.

---

## Project structure

```text
smallerpixels/
├── src/                        # React frontend (TypeScript)
│   ├── main.tsx
│   ├── App.tsx                 # Tab switcher: Manual | Watch | Log
│   ├── App.css                 # Dark theme + animations
│   ├── components/
│   │   ├── ManualMode.tsx      # Drag-drop + folder picker + convert button
│   │   ├── WatchMode.tsx       # Source/output config + start/stop
│   │   ├── ProgressPanel.tsx   # Spinner, shimmer bar, elapsed time, ETA, results list
│   │   ├── SettingsPanel.tsx   # Quality + max-dimension sliders
│   │   └── HistoryLog.tsx      # Per-batch history with compression stats
│   ├── hooks/
│   │   └── useConversion.ts    # Tauri event listeners + progress/results state
│   └── lib/
│       └── commands.ts         # Typed Tauri command wrappers
├── src-tauri/
│   ├── src/
│   │   ├── main.rs             # Tauri entry point
│   │   ├── lib.rs              # AppState, plugin/command registration
│   │   ├── commands.rs         # Tauri IPC commands (convert_batch, pick_folder, watch*)
│   │   ├── converter.rs        # Core: decode → resize → encode → EXIF → timestamps
│   │   └── watcher.rs          # notify watcher + tokio interval sweep
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── capabilities/
│       └── default.json        # Tauri v2 permissions (core:default only)
├── index.html
├── package.json
└── vite.config.ts
```

---

## How it works

### Conversion pipeline

Each image goes through this pipeline inside `converter.rs` on a dedicated thread:

1. **Read** — raw bytes loaded once from disk into memory
2. **EXIF extraction** — scan for the JPEG APP1 segment (the `0xFF 0xE1` marker); capture the raw bytes before decoding so the original EXIF block is preserved byte-for-byte
3. **Decode** — `image::load_from_memory()` via the `image` crate (zune-jpeg decoder for JPEG; fast enough for the pipeline to be CPU-bound at the WebP encoder, not the decoder)
4. **Resize** — if `max(width, height) > max_size`, scale down with `FilterType::Lanczos3`; images already within bounds are never upscaled
5. **Encode** — `webp::Encoder::from_image(&img).encode(quality)` produces an in-memory `WebPMemory` buffer
6. **EXIF injection** — manually append an RIFF `EXIF` chunk to the WebP container in memory; the WebP format is a RIFF container, so this is a handful of bytes prepended before the final write
7. **Write** — single `std::fs::write()` call; one disk write per file
8. **Timestamps** — `filetime::set_file_times()` copies the original `mtime` and `atime` to the output file so the converted file appears with the original capture date in Finder / file managers

### Parallelism

`convert_batch_parallel` (in `converter.rs`) spawns one `tokio::task::spawn_blocking` per image, bounded by a `tokio::sync::Semaphore` capped at `min(cpu_count, 8)`. An `AtomicUsize` tracks completions so progress events always report a monotonically increasing count regardless of which file finishes first.

### Watch mode

`watcher.rs` runs two concurrent mechanisms inside a single tokio task:

1. **Filesystem events** — `notify::recommended_watcher()` (kqueue on macOS, inotify on Linux, ReadDirectoryChangesW on Windows) fires immediately when a new file appears
2. **Interval sweep** — `tokio::time::interval` ticks periodically and scans for files newer than `last_processed`; catches files that arrive while the watcher is briefly suspended or miss events (e.g. network volumes)

Both paths feed into the same `convert_batch_parallel` call and emit the same Tauri events to the frontend.

---

## Tech stack

### Rust crates

| Crate                | Purpose                                                          |
| -------------------- | ---------------------------------------------------------------- |
| `tauri 2`            | Desktop app framework — WebView + IPC + bundler                  |
| `tokio`              | Async runtime; `spawn_blocking` for CPU-bound tasks              |
| `image 0.25`         | Decode JPEG/PNG/GIF/BMP; Lanczos3 resize                         |
| `webp 0.3`           | WebP encoder (libwebp FFI)                                       |
| `kamadak-exif 0.5`   | EXIF reader (used for diagnostics; raw bytes extracted manually) |
| `filetime 0.2`       | Set `mtime`/`atime` on output files                              |
| `walkdir 2`          | Recursive directory scan                                         |
| `notify 8`           | Cross-platform filesystem watcher                                |
| `anyhow 1`           | Error propagation                                                |
| `serde / serde_json` | Serialise structs for Tauri IPC                                  |

### Frontend

| Package             | Purpose                          |
| ------------------- | -------------------------------- |
| `react 19`          | UI framework                     |
| `@tauri-apps/api 2` | `invoke()`, `listen()`, `once()` |
| `vite 7`            | Dev server + bundler             |
| `typescript 5.8`    | Type safety                      |

---

## Tauri events reference

The Rust backend emits these events; the React frontend subscribes via `listen()`:

| Event                   | Payload                                                                                                                         |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `conversion:start`      | `{ total: number }`                                                                                                             |
| `conversion:file-start` | `{ file: string, index: number, total: number }`                                                                                |
| `conversion:progress`   | `{ file: string, index: number, completed: number, total: number, success: boolean, originalSize: number, outputSize: number }` |
| `conversion:complete`   | `{ results: ConversionResult[] }`                                                                                               |
| `watch:status`          | `{ active: boolean, lastRun: string \| null }`                                                                                  |

---

## Submitting changes

1. **Open an issue first** for anything beyond a small fix — describe the problem or feature before investing in implementation
2. Fork the repo and create a branch off `main`
3. Make your changes
4. Run both checks before opening a PR:
   ```bash
   npx tsc --noEmit          # TypeScript type check (no build output)
   cd src-tauri && cargo check  # Rust compile check (no linking)
   ```
5. Open a pull request against `main` — keep the description focused on _why_, not just what changed

Small, focused PRs are easier to review and merge than large ones. If a change touches both the Rust backend and the React frontend, a single PR is fine — just describe both sides.
