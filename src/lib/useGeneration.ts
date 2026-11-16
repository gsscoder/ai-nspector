import { useEffect, useState } from "react";
import { onModulesProgress } from "@/api";

// Runs one cancellable backend generation and tracks its progress stage, elapsed time and error.
export function useGeneration() {
  const [stage, setStage] = useState<string | null>(null); // non-null while generating
  const [error, setError] = useState<string | null>(null);
  const [seconds, setSeconds] = useState(0);

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
    setStage("Starting");
    let unlisten: (() => void) | null = null;
    try {
      unlisten = await onModulesProgress(setStage);
      return await call();
    } catch (e) {
      if (String(e) !== "Cancelled") setError(String(e));
      return null;
    } finally {
      unlisten?.();
      setStage(null);
    }
  }

  return { stage, seconds, error, generating, run };
}
