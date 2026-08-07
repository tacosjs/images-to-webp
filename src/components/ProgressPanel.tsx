import { useEffect, useState } from "react";
import { ProgressState } from "../hooks/useConversion";

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(2)} MB`;
}

function formatDuration(ms: number) {
  if (ms < 1000) return `${ms}ms`;
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(1)}s`;
  const m = Math.floor(s / 60);
  const rem = Math.round(s % 60);
  return `${m}m ${rem}s`;
}

function basename(path: string) {
  return path.split("/").pop() ?? path;
}

interface Props {
  progress: ProgressState;
  onReset: () => void;
}

export function ProgressPanel({ progress, onReset }: Props) {
  const {
    isRunning,
    total,
    completed,
    currentFile,
    startedAt,
    avgMsPerFile,
    results,
  } = progress;

  // Tick every 100 ms while running so elapsed/ETA updates smoothly
  const [, setTick] = useState(0);
  useEffect(() => {
    if (!isRunning) return;
    const id = setInterval(() => setTick((t) => t + 1), 100);
    return () => clearInterval(id);
  }, [isRunning]);

  if (total === 0 && results.length === 0) return null;

  const now = Date.now();
  const elapsedMs = startedAt ? now - startedAt : 0;
  const pct = total > 0 ? (completed / total) * 100 : 0;
  const remaining = total - completed;
  const etaMs =
    avgMsPerFile !== null && remaining > 0
      ? // account for the file currently in-flight
        remaining * avgMsPerFile
      : null;

  const ok = results.filter((r) => r.success).length;
  const fail = results.filter((r) => !r.success).length;

  return (
    <div className="progress-panel">
      {isRunning ? (
        <>
          {/* Header row: spinner + count + elapsed */}
          <div className="progress-header">
            <span className="progress-spinner" aria-hidden />
            <span className="progress-count">
              {completed} / {total} files
            </span>
            <span className="progress-time elapsed">
              {formatDuration(elapsedMs)}
            </span>
            {etaMs !== null && (
              <span className="progress-time eta">
                ~{formatDuration(etaMs)} left
              </span>
            )}
          </div>

          {/* Animated progress bar */}
          <div className="progress-bar">
            <div
              className="progress-fill shimmer"
              style={{ width: `${pct}%` }}
            />
          </div>

          {/* Current file */}
          {currentFile && (
            <div className="progress-current">
              <span className="progress-current-label">Processing</span>
              <span className="progress-current-file">
                {basename(currentFile)}
              </span>
            </div>
          )}
        </>
      ) : (
        <div className="progress-header">
          <span className="progress-count done">
            Done in {formatDuration(elapsedMs)} — {ok} succeeded
            {fail > 0 ? `, ${fail} failed` : ""}
          </span>
          <button className="btn-ghost" onClick={onReset}>
            Clear
          </button>
        </div>
      )}

      {results.length > 0 && (
        <ul className="result-list">
          {results.map((r, i) => {
            const saved =
              r.original_size && r.output_size && r.original_size > 0
                ? Math.round((1 - r.output_size / r.original_size) * 100)
                : null;
            return (
              <li key={i} className={r.success ? "result-ok" : "result-fail"}>
                <span className="result-icon">{r.success ? "✓" : "✗"}</span>
                <span className="result-name">{basename(r.input)}</span>
                {r.success && r.output_size !== undefined && (
                  <span className="result-size">
                    {r.original_size
                      ? `${formatBytes(r.original_size)} → `
                      : ""}
                    {formatBytes(r.output_size)}
                    {saved !== null && (
                      <span className="result-pct"> −{saved}%</span>
                    )}
                  </span>
                )}
                {!r.success && r.error && (
                  <span className="result-error">{r.error}</span>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
