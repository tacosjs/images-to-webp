import { useEffect, useRef, useState } from "react";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { ConversionResult } from "../lib/commands";

export interface ProgressState {
  isRunning: boolean;
  total: number;
  completed: number;
  currentFile: string | null;
  startedAt: number | null;
  /** Rolling average ms per file based on observed completions. */
  avgMsPerFile: number | null;
  results: ConversionResult[];
}

const INITIAL: ProgressState = {
  isRunning: false,
  total: 0,
  completed: 0,
  currentFile: null,
  startedAt: null,
  avgMsPerFile: null,
  results: [],
};

export function useConversion() {
  const [progress, setProgress] = useState<ProgressState>(INITIAL);
  const unlisteners = useRef<UnlistenFn[]>([]);
  // Track when the current file started so we can measure actual duration
  const fileStartRef = useRef<number | null>(null);
  const samplesRef = useRef<number[]>([]);

  useEffect(() => {
    const setup = async () => {
      const unStart = await listen<{ total: number }>(
        "conversion:start",
        ({ payload }) => {
          samplesRef.current = [];
          fileStartRef.current = null;
          setProgress({
            ...INITIAL,
            isRunning: true,
            total: payload.total,
            startedAt: Date.now(),
          });
        },
      );

      const unFileStart = await listen<{ file: string; index: number }>(
        "conversion:file-start",
        ({ payload }) => {
          fileStartRef.current = Date.now();
          setProgress((prev) => ({ ...prev, currentFile: payload.file }));
        },
      );

      const unProgress = await listen<{
        file: string;
        index: number;
        completed: number;
        total: number;
        success: boolean;
        originalSize: number;
        outputSize: number;
      }>("conversion:progress", ({ payload }) => {
        // Record timing sample
        if (fileStartRef.current !== null) {
          const elapsed = Date.now() - fileStartRef.current;
          samplesRef.current.push(elapsed);
          // Keep last 10 samples for rolling average
          if (samplesRef.current.length > 10) samplesRef.current.shift();
          fileStartRef.current = null;
        }

        const avg =
          samplesRef.current.length > 0
            ? samplesRef.current.reduce((a, b) => a + b, 0) /
              samplesRef.current.length
            : null;

        setProgress((prev) => ({
          ...prev,
          completed: payload.completed,
          currentFile: null,
          avgMsPerFile: avg,
          results: [
            ...prev.results,
            {
              success: payload.success,
              input: payload.file,
              output: "",
              original_size: payload.originalSize,
              output_size: payload.outputSize,
            },
          ],
        }));
      });

      const unComplete = await listen<{ results: ConversionResult[] }>(
        "conversion:complete",
        ({ payload }) => {
          setProgress((prev) => ({
            ...prev,
            isRunning: false,
            currentFile: null,
            results: payload.results ?? prev.results,
          }));
        },
      );

      unlisteners.current = [unStart, unFileStart, unProgress, unComplete];
    };

    setup();
    return () => unlisteners.current.forEach((fn) => fn());
  }, []);

  const reset = () => {
    samplesRef.current = [];
    fileStartRef.current = null;
    setProgress(INITIAL);
  };

  return { progress, reset };
}
