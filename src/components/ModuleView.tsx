import { useEffect, useState } from "react";
import { ExternalLink } from "lucide-react";
import { Button } from "@/components/ui/button";
import AnalysisPanel from "@/components/AnalysisPanel";
import CodeView from "@/components/CodeView";
import GenerationProgress from "@/components/GenerationProgress";
import ProjectReportView from "@/components/ProjectReportView";
import { listAnalyses, openInEditor, startProjectAnalysis } from "@/api";
import { splitPath } from "@/lib/project";
import { useGeneration } from "@/lib/useGeneration";
import type { AnalysisInfo, Module, OverviewInfo, ProjectReport } from "@/types";

export default function ModuleView({
  repoRoot,
  projectId,
  module,
  overview,
}: {
  repoRoot: string;
  projectId: string;
  module: Module;
  overview: OverviewInfo | null | undefined;
}) {
  // Primary files first, then secondary.
  const files = [...module.files].sort((a, b) => Number(b.primary) - Number(a.primary));
  const [selected, setSelected] = useState(files[0]?.path);
  const [openError, setOpenError] = useState<string | null>(null);
  const [analyses, setAnalyses] = useState<AnalysisInfo[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [report, setReport] = useState<ProjectReport | null>(null);
  const generation = useGeneration();

  useEffect(() => {
    let stale = false;
    listAnalyses()
      .then((list) => !stale && setAnalyses(list))
      .catch((e) => !stale && setLoadError(String(e)));
    return () => {
      stale = true;
    };
  }, []);

  async function run(analysisId: string) {
    const result = await generation.run(() => startProjectAnalysis(repoRoot, projectId, analysisId, [module.id]));
    if (result) setReport(result);
  }

  const error = generation.error ?? loadError;

  return (
    <div className="flex min-h-0 flex-1 gap-6">
      <aside className="w-72 shrink-0 overflow-y-auto">
        <h2 className="text-xl leading-snug font-semibold tracking-tight">{module.name}</h2>
        <p className="mt-1.5 mb-4 text-sm text-muted-foreground">{module.purpose}</p>
        <ul className="flex flex-col gap-0.5">
          {files.map((f) => {
            const { name, dir } = splitPath(f.path);
            return (
              <li key={f.path}>
                <button
                  className={`w-full rounded-md px-2.5 py-1.5 text-left text-sm outline-none transition-colors focus-visible:ring-3 focus-visible:ring-ring/50 ${f.path === selected ? "bg-muted" : "hover:bg-muted/50"}`}
                  title={f.path}
                  aria-current={f.path === selected ? "true" : undefined}
                  onClick={() => {
                    setSelected(f.path);
                    setOpenError(null);
                  }}
                >
                  <div className="flex items-center gap-2">
                    <span className="truncate">{name}</span>
                    {!f.primary && (
                      <span className="shrink-0 rounded border px-1 text-[10px] leading-4 text-muted-foreground">secondary</span>
                    )}
                  </div>
                  <div className="truncate text-xs text-muted-foreground">{dir}</div>
                </button>
              </li>
            );
          })}
        </ul>
      </aside>
      <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-6">
        <div className="flex min-h-0 flex-1 gap-6">
          <section className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-xl border bg-card">
            {selected ? (
              <>
                <div className="flex shrink-0 items-center justify-between gap-2 border-b py-1.5 pr-1.5 pl-4">
                  <span className="truncate font-mono text-xs text-muted-foreground" title={selected}>
                    {selected}
                  </span>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="text-muted-foreground"
                    onClick={() =>
                      openInEditor(repoRoot, selected).then(
                        () => setOpenError(null),
                        (e) => setOpenError(`Cannot open the file: ${String(e)}`),
                      )
                    }
                  >
                    <ExternalLink />
                    Open in editor
                  </Button>
                </div>
                {openError && (
                  <p role="alert" className="shrink-0 border-b px-4 py-2 text-sm text-destructive">
                    {openError}
                  </p>
                )}
                <CodeView repoPath={repoRoot} path={selected} />
              </>
            ) : (
              <p className="p-4 text-sm text-muted-foreground">This module has no files.</p>
            )}
          </section>
          <AnalysisPanel
            analyses={analyses}
            overviewMissing={overview === null}
            running={generation.generating}
            onRun={run}
          />
        </div>
        {(generation.generating || error) && (
          <div className="flex shrink-0 flex-col gap-2 rounded-xl border bg-card p-3">
            {generation.generating && (
              <>
                <GenerationProgress
                  generation={generation}
                  lastDuration={report && !report.cancelled ? report.durationSecs : undefined}
                  kind="findings"
                />
                {generation.streamed !== "" && (
                  <p className="max-h-40 overflow-y-auto text-xs whitespace-pre-wrap text-muted-foreground">{generation.streamed}</p>
                )}
              </>
            )}
            {error && (
              <p role="alert" className="text-sm text-destructive">
                {error}
              </p>
            )}
          </div>
        )}
        {report && (
          <ProjectReportView
            key={report.createdAt}
            repoRoot={repoRoot}
            projectId={projectId}
            report={report}
            analysisName={analyses.find((a) => a.id === report.analysis)?.name ?? report.analysis}
            requested={1}
            onClose={() => setReport(null)}
          />
        )}
      </div>
    </div>
  );
}
