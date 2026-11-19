import { useState } from "react";
import { PanelRightClose, PanelRightOpen, Play } from "lucide-react";
import { Button } from "@/components/ui/button";
import GenerationProgress from "@/components/GenerationProgress";
import LlmHint from "@/components/LlmHint";
import { onChatDelta, onProjectAnalysisProgress, startProjectAnalysis } from "@/api";
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
  const [streamed, setStreamed] = useState("");
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
      const unlisten = await Promise.all([
        onProjectAnalysisProgress((p) => {
          setProgress(p);
          setStreamed("");
        }),
        onChatDelta((text) => setStreamed((s) => s + text)),
      ]);
      try {
        return await startProjectAnalysis(repoRoot, projectId, analysis.id, chosen.map((m) => m.id));
      } finally {
        unlisten.forEach((u) => u());
      }
    });
    setProgress(null);
    setStreamed("");
    if (report) onReport(report, chosen.length);
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
      <div className="mb-2 flex items-center justify-between">
        <h3 className="text-xs font-medium tracking-wider text-muted-foreground uppercase">Project analysis</h3>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label="Hide project analysis"
          disabled={running}
          onClick={() => setOpen(false)}
        >
          <PanelRightClose />
        </Button>
      </div>
      {overviewMissing && (
        <p className="mb-2 text-xs text-muted-foreground">Generate the repo overview for better results.</p>
      )}
      <LlmHint className="mb-2" />
      <div className="flex flex-col gap-2 rounded-xl border bg-card p-3">
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
            <GenerationProgress stage={generation.stage} seconds={generation.seconds} />
            {streamed !== "" && (
              <p className="max-h-40 overflow-y-auto text-xs whitespace-pre-wrap text-muted-foreground">{streamed}</p>
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
