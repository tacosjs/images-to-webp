import { ConversionConfig } from "../lib/commands";

interface Props {
  config: ConversionConfig;
  onChange: (config: ConversionConfig) => void;
}

export function SettingsPanel({ config, onChange }: Props) {
  return (
    <div className="settings-panel">
      <label>
        <span>Max dimension</span>
        <div className="input-row">
          <input
            type="range"
            min={256}
            max={4096}
            step={256}
            value={config.max_size}
            onChange={(e) =>
              onChange({ ...config, max_size: Number(e.target.value) })
            }
          />
          <span className="value-badge">{config.max_size}px</span>
        </div>
      </label>
      <label>
        <span>WebP quality</span>
        <div className="input-row">
          <input
            type="range"
            min={0}
            max={100}
            step={10}
            value={config.quality}
            onChange={(e) =>
              onChange({ ...config, quality: Number(e.target.value) })
            }
          />
          <span className="value-badge">{config.quality}</span>
        </div>
      </label>
    </div>
  );
}
