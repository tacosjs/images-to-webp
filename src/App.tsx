import { useState } from "react";
import { ManualMode } from "./components/ManualMode";
import { WatchMode } from "./components/WatchMode";
import { SettingsPanel } from "./components/SettingsPanel";
import { ConversionConfig } from "./lib/commands";
import "./App.css";

type Mode = "manual" | "watch";

function loadConfig(): ConversionConfig {
  try {
    const raw = localStorage.getItem("settings:config");
    if (raw) return JSON.parse(raw);
  } catch {}
  return { max_size: 2048, quality: 80 };
}

export default function App() {
  const [mode, setMode] = useState<Mode>(
    () => (localStorage.getItem("mode") as Mode | null) ?? "manual"
  );
  const [config, setConfig] = useState<ConversionConfig>(loadConfig);

  const handleModeChange = (m: Mode) => {
    setMode(m);
    localStorage.setItem("mode", m);
  };

  const handleConfigChange = (c: ConversionConfig) => {
    setConfig(c);
    localStorage.setItem("settings:config", JSON.stringify(c));
  };

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
        </nav>
      </header>

      <SettingsPanel config={config} onChange={handleConfigChange} />

      <main className="app-main">
        {mode === "manual" ? (
          <ManualMode config={config} />
        ) : (
          <WatchMode config={config} />
        )}
      </main>
    </div>
  );
}
