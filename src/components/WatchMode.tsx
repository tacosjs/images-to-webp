import { useEffect, useState } from "react";
import {
  startWatch,
  stopWatch,
  getWatchStatus,
  pickFolder,
  ConversionConfig,
  WatchStatus,
} from "../lib/commands";
import { ProgressPanel } from "./ProgressPanel";
import { useConversion } from "../hooks/useConversion";

const INTERVAL_OPTIONS = [
  { label: "When files are added", value: 0 },
  { label: "Every hour", value: 3600 },
  { label: "Every day", value: 86400 },
  { label: "Every 3 days", value: 259200 },
];

interface Props {
  config: ConversionConfig;
}

export function WatchMode({ config }: Props) {
  const [sourceDir, setSourceDir] = useState(() =>
    localStorage.getItem("watch:sourceDir") ?? ""
  );
  const [outputDir, setOutputDir] = useState(() =>
    localStorage.getItem("watch:outputDir") ?? ""
  );
  const [intervalSecs, setIntervalSecs] = useState(() =>
    Number(localStorage.getItem("watch:intervalSecs") ?? "0")
  );
  const [status, setStatus] = useState<WatchStatus>({ active: false });
  const { progress, reset } = useConversion();

  useEffect(() => {
    getWatchStatus().then(setStatus);
  }, []);

  useEffect(() => {
    localStorage.setItem("watch:sourceDir", sourceDir);
  }, [sourceDir]);

  useEffect(() => {
    localStorage.setItem("watch:outputDir", outputDir);
  }, [outputDir]);

  useEffect(() => {
    localStorage.setItem("watch:intervalSecs", String(intervalSecs));
  }, [intervalSecs]);

  const pickSource = async () => {
    const f = await pickFolder();
    if (f) setSourceDir(f);
  };

  const pickOutput = async () => {
    const f = await pickFolder();
    if (f) setOutputDir(f);
  };

  const handleStart = async () => {
    reset();
    await startWatch({
      source_dir: sourceDir,
      output_dir: outputDir,
      conversion: config,
      interval_secs: intervalSecs,
    });
    setStatus({ active: true, source_dir: sourceDir, output_dir: outputDir });
  };

  const handleStop = async () => {
    await stopWatch();
    setStatus({ active: false });
  };

  const canStart = sourceDir !== "" && outputDir !== "" && !status.active;

  return (
    <div className="mode-content">
      <div className="folder-row">
        <span className="folder-label">Source</span>
        <span className="folder-path">{sourceDir || "—"}</span>
        <button className="btn-secondary" disabled={status.active} onClick={pickSource}>
          Choose…
        </button>
      </div>

      <div className="folder-row">
        <span className="folder-label">Output</span>
        <span className="folder-path">{outputDir || "—"}</span>
        <button className="btn-secondary" disabled={status.active} onClick={pickOutput}>
          Choose…
        </button>
      </div>

      <div className="folder-row">
        <span className="folder-label">Trigger</span>
        <select
          className="select"
          value={intervalSecs}
          disabled={status.active}
          onChange={(e) => setIntervalSecs(Number(e.target.value))}
        >
          {INTERVAL_OPTIONS.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
      </div>

      <div className="watch-actions">
        {status.active ? (
          <>
            <div className="status-badge active">Watching…</div>
            <button className="btn-danger" onClick={handleStop}>
              Stop
            </button>
          </>
        ) : (
          <button className="btn-primary" disabled={!canStart} onClick={handleStart}>
            Start watching
          </button>
        )}
      </div>

      {status.active && (
        <div className="watch-info">
          <span>Source: {status.source_dir}</span>
          <span>Output: {status.output_dir}</span>
        </div>
      )}

      <ProgressPanel progress={progress} onReset={reset} />
    </div>
  );
}
