use std::path::PathBuf;
use std::process::Command;

use tauri::{AppHandle, Emitter, State};

use crate::converter::{self, ConversionConfig, ConversionResult};
use crate::watcher::{self, WatchConfig, WatchStatus};
use crate::AppState;

#[tauri::command]
pub async fn convert_batch(
    input_paths: Vec<String>,
    output_dir: String,
    config: ConversionConfig,
    app: AppHandle,
) -> Result<Vec<ConversionResult>, String> {
    let output = PathBuf::from(&output_dir);

    let mut images: Vec<(PathBuf, PathBuf)> = Vec::new(); // (image_path, source_root)
    for path_str in &input_paths {
        let path = PathBuf::from(path_str);
        if path.is_dir() {
            let found = converter::find_images(&path, None);
            for img in found {
                images.push((img, path.clone()));
            }
        } else if path.is_file() && converter::is_supported_image(&path) {
            let root = path.parent().map(PathBuf::from).unwrap_or_default();
            images.push((path, root));
        }
    }

    let total = images.len();
    app.emit("conversion:start", serde_json::json!({ "total": total }))
        .ok();

    let mut results = Vec::with_capacity(total);

    for (i, (image, source_root)) in images.iter().enumerate() {
        app.emit(
            "conversion:file-start",
            serde_json::json!({ "file": image.to_string_lossy(), "index": i }),
        )
        .ok();

        let result = converter::convert_image(image, source_root, &output, &config);

        app.emit(
            "conversion:progress",
            serde_json::json!({
                "file": image.to_string_lossy(),
                "index": i,
                "success": result.success,
                "outputSize": result.output_size.unwrap_or(0),
            }),
        )
        .ok();

        results.push(result);
    }

    app.emit(
        "conversion:complete",
        serde_json::json!({ "results": results }),
    )
    .ok();

    Ok(results)
}

#[tauri::command]
pub async fn start_watch(
    config: WatchConfig,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let handle = watcher::start(config, app).map_err(|e| e.to_string())?;
    let mut watcher = state.watcher.lock().map_err(|e| e.to_string())?;
    *watcher = Some(handle);
    Ok(())
}

#[tauri::command]
pub async fn stop_watch(state: State<'_, AppState>) -> Result<(), String> {
    let mut watcher = state.watcher.lock().map_err(|e| e.to_string())?;
    *watcher = None;
    Ok(())
}

#[tauri::command]
pub async fn get_watch_status(state: State<'_, AppState>) -> Result<WatchStatus, String> {
    let watcher = state.watcher.lock().map_err(|e| e.to_string())?;
    Ok(match watcher.as_ref() {
        Some(h) => h.status(),
        None => WatchStatus {
            active: false,
            source_dir: None,
            output_dir: None,
            last_run: None,
        },
    })
}

/// Show a native macOS folder-picker dialog via osascript.
#[tauri::command]
pub async fn pick_folder() -> Result<Option<String>, String> {
    let output = Command::new("osascript")
        .args(["-e", "POSIX path of (choose folder)"])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string();
        Ok(if path.is_empty() { None } else { Some(path) })
    } else {
        // User cancelled — not an error
        Ok(None)
    }
}
