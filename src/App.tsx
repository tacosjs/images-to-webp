import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ManualMode } from "./components/ManualMode";
import { WatchMode } from "./components/WatchMode";
import { SettingsPanel } from "./components/SettingsPanel";
import { HistoryLog, HistoryBatch } from "./components/HistoryLog";
import { ConversionConfig, ConversionResult } from "./lib/commands";

import "./App.css";

type Mode = "manual" | "watch" | "log";

function loadConfig(): ConversionConfig {
  try {
    const raw = localStorage.getItem("settings:config");
    if (raw) return JSON.parse(raw);
  } catch {
    /* ignore */
  }
  return { max_size: 2048, quality: 80 };
}

export default function App() {
  const [mode, setMode] = useState<Mode>(
    () => (localStorage.getItem("mode") as Mode | null) ?? "manual",
  );
  const [config, setConfig] = useState<ConversionConfig>(loadConfig);
  const [history, setHistory] = useState<HistoryBatch[]>([]);
  const batchIdRef = useRef(0);
  // Track whether we have unseen log entries to show a badge
  const [unseenCount, setUnseenCount] = useState(0);

  const handleModeChange = (m: Mode) => {
    setMode(m);
    localStorage.setItem("mode", m);
    if (m === "log") setUnseenCount(0);
  };

  const handleConfigChange = (c: ConversionConfig) => {
    setConfig(c);
    localStorage.setItem("settings:config", JSON.stringify(c));
  };

  // Listen globally to conversion:complete to build history across all modes
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<{ results?: ConversionResult[] }>(
      "conversion:complete",
      ({ payload }) => {
        const results = payload.results;
        if (!results || results.length === 0) return;
        const batch: HistoryBatch = {
          id: ++batchIdRef.current,
          completedAt: Date.now(),
          results,
        };
        setHistory((prev) => [batch, ...prev]);
        setUnseenCount((n) => n + results.length);
      },
    ).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  return (
    <div className="app">
      <header className="app-header">
        <h1 className="app-title">Images → WebP</h1>
        <nav className="mode-tabs">
          <button
            className={mode === "manual" ? "tab active" : "tab"}
            onClick={() => handleModeChange("manual")}
          >
            Manual
          </button>
          <button
            className={mode === "watch" ? "tab active" : "tab"}
            onClick={() => handleModeChange("watch")}
          >
            Watch
          </button>
          <button
            className={mode === "log" ? "tab active" : "tab"}
            onClick={() => handleModeChange("log")}
          >
            Log
            {unseenCount > 0 && mode !== "log" && (
              <span className="tab-badge">{unseenCount}</span>
            )}
          </button>
        </nav>
      </header>

      {mode !== "log" && (
        <SettingsPanel config={config} onChange={handleConfigChange} />
      )}

      <main className="app-main">
        {mode === "manual" && <ManualMode config={config} />}
        {mode === "watch" && <WatchMode config={config} />}
        {mode === "log" && (
          <HistoryLog
            history={history}
            onClear={() => {
              setHistory([]);
              setUnseenCount(0);
            }}
          />
        )}
      </main>
    </div>
  );
}
