import { Button } from "@/components/ui/button";
import { cancelGeneration } from "@/api";
import { parseModuleNames } from "@/lib/moduleNames";
import type { Generation } from "@/lib/useGeneration";

const formatTokens = (tokens: number) => (tokens < 1000 ? `${Math.round(tokens)}` : `${Math.round(tokens / 1000)}k`);

export default function GenerationProgress({
  generation,
  lastDuration,
  kind,
}: {
  generation: Generation;
  lastDuration?: number; // seconds of the previous run
  kind?: "modules" | "findings"; // what the streamed text holds: module JSON or markdown findings
}) {
  const { stage, seconds, streamed, stats } = generation;
  const names = kind === "modules" ? parseModuleNames(streamed) : [];
  // ponytail: counts numbered lines at column 0; a nested numbered list at column 0 would over-count
  const findings = kind === "findings" ? (streamed.match(/^\d+\.\s/gm)?.length ?? 0) : 0;
  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center gap-2">
        <span className="spinner" />
        <span role="status" className="text-sm text-muted-foreground tabular-nums">
          {stage}… {seconds}s
          {lastDuration !== undefined && ` · last run ${lastDuration}s`}
        </span>
        <Button variant="outline" size="sm" onClick={cancelGeneration}>
          Cancel
        </Button>
      </div>
      {stats && (
        <p className="text-xs text-muted-foreground tabular-nums">
          Sent {stats.files} {stats.files === 1 ? "file" : "files"}, about {formatTokens(stats.tokens)} tokens
        </p>
      )}
      {streamed !== "" && (
        <p className="text-xs text-muted-foreground tabular-nums">~{formatTokens(streamed.length / 4)} tokens received</p>
      )}
      {names.length > 0 && (
        <p className="text-xs break-words text-muted-foreground">
          Found {names.length} {names.length === 1 ? "module" : "modules"}: {names.join(", ")}…
        </p>
      )}
      {findings > 0 && (
        <p className="text-xs text-muted-foreground">
          ~{findings} {findings === 1 ? "finding" : "findings"} so far in this module
        </p>
      )}
    </div>
  );
}
