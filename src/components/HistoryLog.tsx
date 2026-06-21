import { ConversionResult } from "../lib/commands";

export interface HistoryBatch {
  id: number;
  completedAt: number;
  results: ConversionResult[];
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(2)} MB`;
}

function timeAgo(ts: number) {
  const s = Math.round((Date.now() - ts) / 1000);
  if (s < 5) return "Just now";
  if (s < 60) return `${s}s ago`;
  const m = Math.round(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.round(m / 60);
  return `${h}h ago`;
}

function pct(original: number, output: number) {
  return Math.round((1 - output / original) * 100);
}

function pctColor(p: number): string {
  if (p >= 70) return "pct-great";
  if (p >= 40) return "pct-good";
  return "pct-modest";
}

interface SummaryBarProps {
  totalOriginal: number;
  totalOutput: number;
}

function SummaryBar({ totalOriginal, totalOutput }: SummaryBarProps) {
  if (totalOriginal === 0) return null;
  const outputPct = Math.max(2, (totalOutput / totalOriginal) * 100);
  const savedPct = 100 - outputPct;
  const saved = pct(totalOriginal, totalOutput);

  return (
    <div className="summary-block">
      <div className="summary-numbers">
        <span>{formatBytes(totalOriginal)}</span>
        <span className="summary-arrow">→</span>
        <span>{formatBytes(totalOutput)}</span>
        <span className={`summary-saved ${pctColor(saved)}`}>−{saved}%</span>
      </div>
      <div className="summary-bar-track">
        <div className="summary-bar-output" style={{ width: `${outputPct}%` }} />
        <div className="summary-bar-saved" style={{ width: `${savedPct}%` }} />
      </div>
      <div className="summary-bar-labels">
        <span>Output</span>
        <span>Saved</span>
      </div>
    </div>
  );
}

interface FileRowProps {
  result: ConversionResult;
}

function FileRow({ result }: FileRowProps) {
  const name = result.input.split("/").pop() ?? result.input;
  const hasSize = result.original_size && result.output_size && result.original_size > 0;
  const reduction = hasSize
    ? pct(result.original_size!, result.output_size!)
    : null;
  const fillPct = hasSize
    ? Math.max(2, (result.output_size! / result.original_size!) * 100)
    : 0;

  if (!result.success) {
    return (
      <li className="log-row log-row--fail">
        <span className="log-icon">✗</span>
        <div className="log-info">
          <span className="log-name">{name}</span>
          <span className="log-error">{result.error ?? "Conversion failed"}</span>
        </div>
      </li>
    );
  }

  return (
    <li className="log-row log-row--ok">
      <span className="log-icon">✓</span>
      <div className="log-info">
        <div className="log-name-row">
          <span className="log-name">{name}</span>
          {reduction !== null && (
            <span className={`log-pct ${pctColor(reduction)}`}>−{reduction}%</span>
          )}
        </div>
        {hasSize && (
          <>
            <span className="log-sizes">
              {formatBytes(result.original_size!)} → {formatBytes(result.output_size!)}
            </span>
            <div className="log-bar-track">
              <div className="log-bar-fill" style={{ width: `${fillPct}%` }} />
            </div>
          </>
        )}
      </div>
    </li>
  );
}

interface Props {
  history: HistoryBatch[];
  onClear: () => void;
}

export function HistoryLog({ history, onClear }: Props) {
  // Aggregate totals across all batches
  const allResults = history.flatMap((b) => b.results);
  const successful = allResults.filter((r) => r.success);
  const totalOriginal = successful.reduce((s, r) => s + (r.original_size ?? 0), 0);
  const totalOutput = successful.reduce((s, r) => s + (r.output_size ?? 0), 0);

  if (history.length === 0) {
    return (
      <div className="log-empty">
        <div className="log-empty-icon">📋</div>
        <div className="log-empty-label">No conversions yet</div>
        <div className="log-empty-sub">
          Convert some images and the log will appear here.
        </div>
      </div>
    );
  }

  return (
    <div className="log-view">
      {/* Session summary */}
      <div className="log-summary-card">
        <div className="log-summary-header">
          <span className="log-summary-title">
            {allResults.length} file{allResults.length !== 1 ? "s" : ""}
            {successful.length !== allResults.length &&
              ` (${allResults.length - successful.length} failed)`}
          </span>
          <button className="btn-ghost" onClick={onClear}>
            Clear
          </button>
        </div>
        {totalOriginal > 0 && (
          <SummaryBar totalOriginal={totalOriginal} totalOutput={totalOutput} />
        )}
      </div>

      {/* Batches, newest first */}
      <div className="log-batches">
        {history.map((batch) => (
          <div key={batch.id} className="log-batch">
            <div className="log-batch-header">
              <span className="log-batch-time">{timeAgo(batch.completedAt)}</span>
              <span className="log-batch-count">
                {batch.results.length} file{batch.results.length !== 1 ? "s" : ""}
              </span>
            </div>
            <ul className="log-list">
              {batch.results.map((r, i) => (
                <FileRow key={i} result={r} />
              ))}
            </ul>
          </div>
        ))}
      </div>
    </div>
  );
}
