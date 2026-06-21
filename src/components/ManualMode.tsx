import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { convertBatch, ConversionConfig, pickFolder } from "../lib/commands";
import { ProgressPanel } from "./ProgressPanel";
import { useConversion } from "../hooks/useConversion";

interface Props {
  config: ConversionConfig;
}

export function ManualMode({ config }: Props) {
  const [inputPaths, setInputPaths] = useState<string[]>([]);
  const [outputDir, setOutputDir] = useState<string>("");
  const [isDragOver, setIsDragOver] = useState(false);
  const { progress, reset } = useConversion();

  // Wire up Tauri drag-drop events
  useEffect(() => {
    const win = getCurrentWindow();
    let unlisten: (() => void) | undefined;

    win
      .onDragDropEvent((event) => {
        if (event.payload.type === "over") {
          setIsDragOver(true);
        } else if (event.payload.type === "drop") {
          setIsDragOver(false);
          const paths = event.payload.paths ?? [];
          setInputPaths((prev) => {
            const combined = [...prev, ...paths];
            return [...new Set(combined)];
          });
        } else {
          setIsDragOver(false);
        }
      })
      .then((fn) => {
        unlisten = fn;
      });

    return () => unlisten?.();
  }, []);

  const addFolder = async () => {
    const folder = await pickFolder();
    if (folder) {
      setInputPaths((prev) => [...new Set([...prev, folder])]);
    }
  };

  const chooseOutput = async () => {
    const folder = await pickFolder();
    if (folder) setOutputDir(folder);
  };

  const removePath = (p: string) =>
    setInputPaths((prev) => prev.filter((x) => x !== p));

  const handleConvert = async () => {
    if (!outputDir || inputPaths.length === 0) return;
    reset();
    await convertBatch(inputPaths, outputDir, config);
  };

  const canConvert =
    inputPaths.length > 0 && outputDir !== "" && !progress.isRunning;

  return (
    <div className="mode-content">
      {/* Drop zone */}
      <div
        className={`drop-zone ${isDragOver ? "drag-over" : ""}`}
        onClick={addFolder}
      >
        {inputPaths.length === 0 ? (
          <>
            <div className="drop-icon">⬇</div>
            <div className="drop-label">Drop folders here</div>
            <div className="drop-sub">or click to pick a folder</div>
          </>
        ) : (
          <ul className="path-list">
            {inputPaths.map((p) => (
              <li key={p}>
                <span className="path-text">{p}</span>
                <button
                  className="btn-remove"
                  onClick={(e) => {
                    e.stopPropagation();
                    removePath(p);
                  }}
                >
                  ×
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>

      {/* Output folder */}
      <div className="folder-row">
        <span className="folder-label">Output</span>
        <span className="folder-path">{outputDir || "—"}</span>
        <button className="btn-secondary" onClick={chooseOutput}>
          Choose…
        </button>
      </div>

      <button
        className="btn-primary"
        disabled={!canConvert}
        onClick={handleConvert}
      >
        {progress.isRunning ? "Converting…" : "Convert"}
      </button>

      <ProgressPanel progress={progress} onReset={reset} />
    </div>
  );
}
