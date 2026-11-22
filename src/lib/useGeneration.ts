import { useEffect, useState } from "react";
import { onChatDelta, onModulesProgress, onPromptStats } from "@/api";
import type { PromptStats } from "@/types";

// Runs one cancellable backend generation and tracks its progress stage, elapsed time, streamed text, prompt size and error.
export function useGeneration() {
  const [stage, setStage] = useState<string | null>(null); // non-null while generating
  const [error, setError] = useState<string | null>(null);
  const [seconds, setSeconds] = useState(0);
  const [streamed, setStreamed] = useState(""); // text of the current model call
  const [stats, setStats] = useState<PromptStats | null>(null);

  const generating = stage !== null;
  useEffect(() => {
    if (!generating) return;
    setSeconds(0);
    const timer = setInterval(() => setSeconds((n) => n + 1), 1000);
    return () => clearInterval(timer);
  }, [generating]);

  // Resolves to null when the call failed or was cancelled; the error is in `error`.
  async function run<T>(call: () => Promise<T>): Promise<T | null> {
    setError(null);
    setStreamed("");
    setStats(null);
    setStage("Starting");
    let unlisten: (() => void)[] = [];
    try {
      unlisten = await Promise.all([
        onModulesProgress((s) => {
          setStage(s);
          setStreamed("");
        }),
        onChatDelta((text) => setStreamed((t) => t + text)),
        onPromptStats(setStats),
      ]);
      return await call();
    } catch (e) {
      if (String(e) !== "Cancelled") setError(String(e));
      return null;
    } finally {
      unlisten.forEach((u) => u());
      setStage(null);
    }
  }

  return { stage, seconds, streamed, stats, error, generating, run };
}

export type Generation = ReturnType<typeof useGeneration>;
