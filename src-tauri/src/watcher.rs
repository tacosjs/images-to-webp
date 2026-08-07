use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::converter::{self, ConversionConfig};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchConfig {
    pub source_dir: String,
    pub output_dir: String,
    pub conversion: ConversionConfig,
    /// 0 means "react to file events only, no periodic sweep"
    pub interval_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchStatus {
    pub active: bool,
    pub source_dir: Option<String>,
    pub output_dir: Option<String>,
    pub last_run: Option<String>,
}

pub struct WatcherHandle {
    pub config: WatchConfig,
    pub last_processed: Arc<Mutex<SystemTime>>,
    // kept alive so the watcher doesn't drop
    _watcher: RecommendedWatcher,
    _task: JoinHandle<()>,
}

impl WatcherHandle {
    pub fn status(&self) -> WatchStatus {
        let last_run = self
            .last_processed
            .lock()
            .ok()
            .and_then(|t| {
                t.duration_since(SystemTime::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_secs())
            })
            .map(|secs| {
                // ISO-8601 seconds-only representation for simplicity
                format!("{secs}")
            });

        WatchStatus {
            active: true,
            source_dir: Some(self.config.source_dir.clone()),
            output_dir: Some(self.config.output_dir.clone()),
            last_run,
        }
    }
}

pub fn start(config: WatchConfig, app: AppHandle) -> anyhow::Result<WatcherHandle> {
    let source_dir = PathBuf::from(&config.source_dir);
    let output_dir = PathBuf::from(&config.output_dir);
    let conv_config = config.conversion.clone();
    let interval_secs = config.interval_secs;

    let last_processed = Arc::new(Mutex::new(SystemTime::now()));
    let last_processed_clone = Arc::clone(&last_processed);

    // Channel from the notify callback into the async task
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<PathBuf>>();
    let _tx_interval = tx.clone();

    // Set up filesystem watcher
    let source_dir_watch = source_dir.clone();
    let mut fs_watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        if let Ok(event) = res {
            if matches!(
                event.kind,
                EventKind::Create(_) | EventKind::Modify(_)
            ) {
                let images: Vec<PathBuf> = event
                    .paths
                    .into_iter()
                    .filter(|p| converter::is_supported_image(p))
                    .collect();
                if !images.is_empty() {
                    let _ = tx.send(images);
                }
            }
        }
    })?;

    fs_watcher.watch(&source_dir_watch, RecursiveMode::Recursive)?;

    // Spawn async task that handles both event-driven and interval-driven conversions
    let app_clone = app.clone();
    let task = tokio::spawn(async move {
        let mut interval = if interval_secs > 0 {
            Some(tokio::time::interval(Duration::from_secs(interval_secs)))
        } else {
            None
        };

        loop {
            let paths_to_process: Option<Vec<PathBuf>> = if let Some(ref mut ticker) = interval {
                tokio::select! {
                    Some(paths) = rx.recv() => Some(paths),
                    _ = ticker.tick() => {
                        // Periodic sweep: find files newer than last_processed
                        let since = last_processed_clone.lock().ok().map(|t| *t);
                        let found = converter::find_images(&source_dir, since);
                        if found.is_empty() { None } else { Some(found) }
                    }
                }
            } else {
                rx.recv().await
            };

            if let Some(paths) = paths_to_process {
                process_and_emit(&paths, &source_dir, &output_dir, &conv_config, &app_clone).await;
                if let Ok(mut last) = last_processed_clone.lock() {
                    *last = SystemTime::now();
                }
            }
        }
    });

    Ok(WatcherHandle {
        config,
        last_processed,
        _watcher: fs_watcher,
        _task: task,
    })
}

async fn process_and_emit(
    paths: &[PathBuf],
    source_dir: &Path,
    output_dir: &Path,
    config: &ConversionConfig,
    app: &AppHandle,
) {
    let images: Vec<(PathBuf, PathBuf)> = paths
        .iter()
        .map(|p| (p.clone(), source_dir.to_path_buf()))
        .collect();

    let total = images.len();
    let _ = app.emit("conversion:start", serde_json::json!({ "total": total }));

    let results = converter::convert_batch_parallel(
        images,
        output_dir.to_path_buf(),
        config.clone(),
        app.clone(),
    )
    .await;

    let _ = app.emit("conversion:complete", serde_json::json!({ "results": results }));
}
