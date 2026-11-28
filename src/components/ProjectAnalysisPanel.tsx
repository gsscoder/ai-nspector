import { useState } from "react";
import { PanelRightClose, PanelRightOpen, Play } from "lucide-react";
import { Button } from "@/components/ui/button";
import GenerationProgress from "@/components/GenerationProgress";
import LlmHint from "@/components/LlmHint";
import Markdown from "@/components/Markdown";
import { onProjectAnalysisProgress, startProjectAnalysis } from "@/api";
import { useGeneration } from "@/lib/useGeneration";
import { useLlmBlocked } from "@/lib/llmConfigured";
import type { AnalysisInfo, Module, ProjectAnalysisProgress, ProjectReport } from "@/types";

export default function ProjectAnalysisPanel({
  repoRoot,
  projectId,
  modules,
  analyses,
  overviewMissing,
  onReport,
}: {
  repoRoot: string;
  projectId: string;
  modules: Module[];
  analyses: AnalysisInfo[];
  overviewMissing: boolean;
  onReport: (report: ProjectReport, requested: number) => void;
}) {
  const [open, setOpen] = useState(true);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [analysisId, setAnalysisId] = useState("");
  const [progress, setProgress] = useState<ProjectAnalysisProgress | null>(null);
  // The last finished run; its time is shown only when the new run has the same number of modules.
  const [last, setLast] = useState<{ seconds: number; modules: number } | null>(null);
  const generation = useGeneration();
  const llmBlocked = useLlmBlocked();

  const running = generation.generating;
  const analysis = analyses.find((a) => a.id === analysisId) ?? analyses[0];
  const chosen = modules.filter((m) => selected.has(m.id));

  function toggle(id: string) {
    const next = new Set(selected);
    if (!next.delete(id)) next.add(id);
    setSelected(next);
  }

  async function run() {
    if (!analysis) return;
    const report = await generation.run(async () => {
      const unlisten = await onProjectAnalysisProgress(setProgress);
      try {
        return await startProjectAnalysis(repoRoot, projectId, analysis.id, chosen.map((m) => m.id));
      } finally {
        unlisten();
      }
    });
    setProgress(null);
    if (report) {
      if (!report.cancelled) setLast({ seconds: report.durationSecs, modules: chosen.length });
      onReport(report, chosen.length);
    }
  }

  if (!open) {
    return (
      <Button variant="ghost" size="icon-sm" aria-label="Show project analysis" onClick={() => setOpen(true)}>
        <PanelRightOpen />
      </Button>
    );
  }
  return (
    <aside className="w-64 shrink-0">
      <div className="mb-4 flex h-8.5 items-end justify-between">
        <h3 className="text-xs font-medium tracking-wider text-muted-foreground uppercase">Project analysis</h3>
        <Button
          variant="ghost"
          size="icon-sm"
          className="-mb-1.5"
          aria-label="Hide project analysis"
          disabled={running}
          onClick={() => setOpen(false)}
        >
          <PanelRightClose />
        </Button>
      </div>
      <div className="flex flex-col gap-2 rounded-xl border bg-card p-3">
        {overviewMissing && (
          <p className="text-xs text-muted-foreground">Generate the repo overview first. The analysis needs it.</p>
        )}
        <LlmHint />
        <select
          className="field h-8"
          aria-label="Analysis"
          value={analysis?.id ?? ""}
          disabled={running}
          onChange={(e) => setAnalysisId(e.target.value)}
        >
          {analyses.map((a) => (
            <option key={a.id} value={a.id}>
              {a.name}
            </option>
          ))}
        </select>
        <div className="flex gap-2">
          <Button
            variant="outline"
            size="sm"
            disabled={running}
            onClick={() => setSelected(new Set(modules.map((m) => m.id)))}
          >
            Select all
          </Button>
          <Button variant="outline" size="sm" disabled={running} onClick={() => setSelected(new Set())}>
            None
          </Button>
        </div>
        <ul className="flex max-h-64 flex-col gap-0.5 overflow-y-auto rounded-lg border p-1">
          {modules.map((m) => (
            <li key={m.id}>
              <label className="flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-muted/60">
                <input
                  type="checkbox"
                  className="accent-primary"
                  checked={selected.has(m.id)}
                  disabled={running}
                  onChange={() => toggle(m.id)}
                />
                <span className="min-w-0 flex-1 truncate" title={m.name}>
                  {m.name}
                </span>
              </label>
            </li>
          ))}
        </ul>
        {running ? (
          <>
            {progress && (
              <p className="text-sm">
                Module {progress.index} of {progress.total}: {progress.moduleName}
              </p>
            )}
            <GenerationProgress
              generation={generation}
              lastDuration={last?.modules === chosen.length ? last.seconds : undefined}
              kind="findings"
            />
            {generation.streamed !== "" && (
              <Markdown text={generation.streamed} className="max-h-40 overflow-y-auto text-xs text-muted-foreground" />
            )}
          </>
        ) : (
          <>
            <p className="text-xs text-muted-foreground">
              {chosen.length} {chosen.length === 1 ? "module" : "modules"} = {chosen.length}{" "}
              {chosen.length === 1 ? "request" : "requests"}
            </p>
            <Button size="sm" disabled={!analysis || chosen.length === 0 || llmBlocked} onClick={run}>
              <Play />
              Run
            </Button>
          </>
        )}
        {generation.error && (
          <p role="alert" className="text-xs text-destructive">
            {generation.error}
          </p>
        )}
      </div>
    </aside>
  );
}
