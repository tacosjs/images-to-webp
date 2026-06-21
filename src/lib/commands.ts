import { invoke } from "@tauri-apps/api/core";

export interface ConversionConfig {
  max_size: number;
  quality: number;
}

export interface ConversionResult {
  success: boolean;
  input: string;
  output: string;
  original_size?: number;
  output_size?: number;
  error?: string;
}

export interface WatchConfig {
  source_dir: string;
  output_dir: string;
  conversion: ConversionConfig;
  interval_secs: number;
}

export interface WatchStatus {
  active: boolean;
  source_dir?: string;
  output_dir?: string;
  last_run?: string;
}

export const convertBatch = (
  inputPaths: string[],
  outputDir: string,
  config: ConversionConfig,
): Promise<ConversionResult[]> =>
  invoke("convert_batch", { inputPaths, outputDir, config });

export const startWatch = (config: WatchConfig): Promise<void> =>
  invoke("start_watch", { config });

export const stopWatch = (): Promise<void> => invoke("stop_watch");

export const getWatchStatus = (): Promise<WatchStatus> =>
  invoke("get_watch_status");

export const pickFolder = (): Promise<string | null> => invoke("pick_folder");
