import { useEffect, useRef, useState } from "react";
import { Boxes, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cancelGeneration, generateModules, loadModules, onModulesProgress } from "@/api";
import { projectLabel } from "@/lib/project";
import type { ModulesFile, Project } from "@/types";

// Backend texts about the LLM server, the network or the API key store (see error.rs, llm.rs, settings.rs).
// The "Set the ... in Settings." messages already name Settings.
const LLM_CONNECTION_ERROR = /LLM server|Network error|HTTP client|credential store/;

export default function ProjectView({
  repoRoot,
  project,
  onOpenModule,
  data,
  setData,
}: {
  repoRoot: string;
  project: Project;
  onOpenModule: (id: string) => void;
  data: ModulesFile | null;
  setData: (d: ModulesFile | null) => void;
}) {
  const [stage, setStage] = useState<string | null>(null); // non-null while generating
  const [error, setError] = useState<string | null>(null);
  const [seconds, setSeconds] = useState(0);
  const mounted = useRef(false); // a run that outlives this view must not touch the shared state
  const unlisten = useRef<(() => void) | null>(null);

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

  const generating = stage !== null;
  useEffect(() => {
    if (!generating) return;
    setSeconds(0);
    const timer = setInterval(() => setSeconds((n) => n + 1), 1000);
    return () => clearInterval(timer);
  }, [generating]);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      unlisten.current?.();
    };
  }, []);

  async function generate() {
    setError(null);
    setStage("Starting");
    try {
      unlisten.current = await onModulesProgress((s) => mounted.current && setStage(s));
      const result = await generateModules(repoRoot, project.id);
      if (mounted.current) setData(result);
    } catch (e) {
      if (mounted.current && String(e) !== "Cancelled") setError(String(e));
    } finally {
      unlisten.current?.();
      unlisten.current = null;
      if (mounted.current) setStage(null);
    }
  }

  return (
    <section className="flex flex-col gap-4">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h2 className="text-2xl font-semibold tracking-tight">{projectLabel(project)}</h2>
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
            <>
              <span className="size-4 animate-spin rounded-full border-2 border-current border-t-transparent" />
              <span role="status" className="text-sm text-muted-foreground">
                {stage}… {seconds}s
              </span>
              <Button variant="outline" onClick={cancelGeneration}>
                Cancel
              </Button>
            </>
          ) : (
            <Button variant={data ? "outline" : "default"} onClick={generate}>
              <Sparkles />
              {data ? "Regenerate" : "Generate modules"}
            </Button>
          )}
        </div>
      </div>
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
          {LLM_CONNECTION_ERROR.test(error) && " (LLM settings are under Settings.)"}
        </p>
      )}
      {!data && !generating && !error && (
        <div className="flex flex-col items-center gap-2 rounded-xl border border-dashed p-10 text-center">
          <Boxes className="size-8 text-muted-foreground" />
          <p className="text-sm text-muted-foreground">No modules yet. Generate them to see how this project is organized.</p>
        </div>
      )}
      {data && (
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
          {data.modules.map((m) => (
            <button
              key={m.id}
              className="flex flex-col rounded-xl border bg-card p-4 text-left transition-colors hover:border-primary/50 hover:bg-muted/40"
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
    </section>
  );
}
