import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { downloadDir } from "@tauri-apps/api/path";
import {
  convertBatch,
  ConversionConfig,
  pickFolder,
  revealInFinder,
} from "../lib/commands";
import { ProgressPanel } from "./ProgressPanel";
import { useConversion } from "../hooks/useConversion";

const SUPPORTED_EXTS = new Set(["jpg", "jpeg", "png", "gif", "bmp", "webp"]);

function isImageOrFolder(path: string): boolean {
  const name = path.split("/").pop() ?? path;
  const dot = name.lastIndexOf(".");
  if (dot === -1) return true; // no extension → treat as folder
  return SUPPORTED_EXTS.has(name.slice(dot + 1).toLowerCase());
}

interface Props {
  config: ConversionConfig;
}

export function ManualMode({ config }: Props) {
  const [inputPaths, setInputPaths] = useState<string[]>([]);
  const [outputDir, setOutputDir] = useState<string>("");
  const [isDragOver, setIsDragOver] = useState(false);
  const [rejectedCount, setRejectedCount] = useState(0);
  const rejectionTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(
    undefined,
  );
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
          const all = event.payload.paths ?? [];
          const accepted = all.filter(isImageOrFolder);
          const rejected = all.length - accepted.length;
          if (rejected > 0) {
            clearTimeout(rejectionTimerRef.current);
            setRejectedCount(rejected);
            rejectionTimerRef.current = setTimeout(
              () => setRejectedCount(0),
              3000,
            );
          }
          setInputPaths((prev) => [...new Set([...prev, ...accepted])]);
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

  const isDownloadMode = outputDir === "";

  const handleConvert = async () => {
    if (inputPaths.length === 0) return;
    reset();
    const targetDir = outputDir || (await downloadDir());
    await convertBatch(inputPaths, targetDir, config);
    if (isDownloadMode) {
      await revealInFinder(targetDir);
    }
  };

  const canConvert = inputPaths.length > 0 && !progress.isRunning;

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

      {rejectedCount > 0 && (
        <p className="drop-rejected">
          {rejectedCount} file{rejectedCount !== 1 ? "s" : ""} skipped — only
          image files are supported
        </p>
      )}

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
        {progress.isRunning
          ? isDownloadMode
            ? "Downloading…"
            : "Converting…"
          : isDownloadMode
            ? "Download"
            : "Convert"}
      </button>

      <ProgressPanel progress={progress} onReset={reset} />
    </div>
  );
}
