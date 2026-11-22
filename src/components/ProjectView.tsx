import { useEffect, useRef, useState } from "react";
import { Boxes, LayoutGrid, Network, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import GenerationProgress from "@/components/GenerationProgress";
import LlmHint from "@/components/LlmHint";
import ModuleGraph from "@/components/ModuleGraph";
import ProjectAnalysisPanel from "@/components/ProjectAnalysisPanel";
import ProjectReportView from "@/components/ProjectReportView";
import { generateModules, listAnalyses, loadModules } from "@/api";
import { useLlmBlocked } from "@/lib/llmConfigured";
import { projectLabel } from "@/lib/project";
import { useGeneration } from "@/lib/useGeneration";
import type { AnalysisInfo, ModulesFile, OverviewInfo, Project, ProjectReport } from "@/types";

// Backend texts about the LLM server, the network or the API key store (see error.rs, llm.rs, settings.rs).
// The "Set the ... in Settings." messages already name Settings.
const LLM_CONNECTION_ERROR = /LLM server|Network error|HTTP client|credential store/;

type View = "cards" | "graph";
const VIEWS: { value: View; label: string; Icon: typeof Network }[] = [
  { value: "cards", label: "Cards", Icon: LayoutGrid },
  { value: "graph", label: "Graph", Icon: Network },
];

export default function ProjectView({
  repoRoot,
  project,
  overview,
  onOpenModule,
  data,
  setData,
}: {
  repoRoot: string;
  project: Project;
  overview: OverviewInfo | null | undefined;
  onOpenModule: (id: string) => void;
  data: ModulesFile | null;
  setData: (d: ModulesFile | null) => void;
}) {
  const [error, setError] = useState<string | null>(null);
  const [view, setView] = useState<View>("cards");
  const [analyses, setAnalyses] = useState<AnalysisInfo[]>([]);
  const [finished, setFinished] = useState<{ report: ProjectReport; requested: number } | null>(null);
  const llmBlocked = useLlmBlocked();
  const generation = useGeneration();
  const mounted = useRef(false); // a run that outlives this view must not touch the shared state

  useEffect(() => {
    let stale = false;
    setError(null);
    loadModules(repoRoot, project.id)
      .then((d) => !stale && setData(d))
      .catch((e) => !stale && setError(String(e)));
    return () => {
      stale = true;
    };
  }, [repoRoot, project.id, setData]);

  useEffect(() => {
    let stale = false;
    listAnalyses()
      .then((list) => !stale && setAnalyses(list))
      .catch((e) => !stale && setError(String(e)));
    return () => {
      stale = true;
    };
  }, []);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const generating = generation.generating;
  const shownError = generation.error ?? error;

  async function generate() {
    setError(null);
    const result = await generation.run(() => generateModules(repoRoot, project.id));
    if (result && mounted.current) setData(result);
  }

  return (
    <section className="flex flex-col gap-4">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h2 className="text-xl font-semibold tracking-tight">{projectLabel(project)}</h2>
          <p className="text-sm text-muted-foreground">
            {project.files.length} {project.files.length === 1 ? "file" : "files"}
            {data && ` · ${data.modules.length} ${data.modules.length === 1 ? "module" : "modules"}`}
          </p>
        </div>
        <div className="flex items-center gap-2">
          {data?.outdated && !generating && (
            <span className="rounded-md bg-amber-500/15 px-2 py-0.5 text-xs text-amber-700 dark:text-amber-300">
              Outdated: the repo changed since {new Date(data.generatedAt).toLocaleString()}
            </span>
          )}
          {generating ? (
            <GenerationProgress generation={generation} lastDuration={data?.durationSecs} kind="modules" />
          ) : (
            <>
              <LlmHint />
              <Button variant={data ? "outline" : "default"} disabled={llmBlocked} onClick={generate}>
                <Sparkles />
                {data ? "Identify again" : "Identify modules"}
              </Button>
            </>
          )}
        </div>
      </div>
      {shownError && (
        <p role="alert" className="text-sm text-destructive">
          {shownError}
          {LLM_CONNECTION_ERROR.test(shownError) && " (LLM settings are under Settings.)"}
        </p>
      )}
      {!data && !generating && !shownError && (
        <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed p-12 text-center">
          <Boxes className="size-6 text-muted-foreground/60" />
          <p className="text-sm text-muted-foreground">No modules yet. Identify them to see how this project is organized.</p>
        </div>
      )}
      {data && (
        <div className="flex items-start gap-6">
          <div className="flex min-w-0 flex-1 flex-col gap-4">
            <div role="group" aria-label="Module view" className="flex w-fit gap-0.5 rounded-lg border bg-card p-0.5">
              {VIEWS.map(({ value, label, Icon }) => (
                <Button
                  key={value}
                  type="button"
                  size="sm"
                  variant={view === value ? "secondary" : "ghost"}
                  className={view === value ? "" : "text-muted-foreground"}
                  aria-pressed={view === value}
                  onClick={() => setView(value)}
                >
                  <Icon />
                  {label}
                </Button>
              ))}
            </div>
            {view === "graph" && (
              <ModuleGraph
                repoRoot={repoRoot}
                projectId={project.id}
                modules={data.modules}
                onOpenModule={onOpenModule}
              />
            )}
            {view === "cards" && (
              <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
                {data.modules.map((m) => (
                  <button
                    key={m.id}
                    className="flex flex-col rounded-xl border bg-card p-4 text-left outline-none transition-colors hover:border-foreground/20 hover:bg-muted/40 focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
                    onClick={() => onOpenModule(m.id)}
                  >
                    <div className="font-medium">{m.name}</div>
                    <div className="mt-1 text-sm text-muted-foreground">{m.purpose}</div>
                    <div className="mt-auto pt-3 text-xs text-muted-foreground">
                      {m.files.length} {m.files.length === 1 ? "file" : "files"}
                    </div>
                  </button>
                ))}
              </div>
            )}
            {finished && (
              <ProjectReportView
                key={finished.report.createdAt}
                repoRoot={repoRoot}
                projectId={project.id}
                report={finished.report}
                analysisName={analyses.find((a) => a.id === finished.report.analysis)?.name ?? finished.report.analysis}
                requested={finished.requested}
                onClose={() => setFinished(null)}
              />
            )}
          </div>
          <ProjectAnalysisPanel
            repoRoot={repoRoot}
            projectId={project.id}
            modules={data.modules}
            analyses={analyses}
            overviewMissing={overview === null}
            onReport={(report, requested) => setFinished({ report, requested })}
          />
        </div>
      )}
    </section>
  );
}
