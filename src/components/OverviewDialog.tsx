import { useEffect, useRef, useState } from "react";
import { Info, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import GenerationProgress from "@/components/GenerationProgress";
import LlmHint from "@/components/LlmHint";
import { generateOverview } from "@/api";
import { useLlmBlocked } from "@/lib/llmConfigured";
import { splitPath } from "@/lib/project";
import { useGeneration } from "@/lib/useGeneration";
import type { DocFile, OverviewInfo, Repo } from "@/types";

// Mirrors MAX_PROMPT_TOKENS in src-tauri/src/modules.rs; the backend enforces it too.
const MAX_PROMPT_TOKENS = 100_000;

// Mirrors the backend error for documentation files that do not describe the project.
const INSUFFICIENT_SOURCES_ERROR = "The selected files do not describe the project. Select other files.";

// Mirrors MAX_FILES in src-tauri/src/overview.rs; the backend enforces it too.
const MAX_FILES = 5;

const suggestedPaths = (docs: DocFile[]) =>
  new Set(
    docs
      .filter((d) => d.suggested)
      .slice(0, MAX_FILES)
      .map((d) => d.path),
  );

// The files of the last run that still exist, or the suggested files when none remain.
const initialPaths = (docs: DocFile[], overview: OverviewInfo | null) => {
  const previous = docs.filter((d) => overview?.files.includes(d.path)).map((d) => d.path);
  return previous.length > 0 ? new Set(previous) : suggestedPaths(docs);
};

export default function OverviewDialog({
  repo,
  overview,
  onGenerated,
  onClose,
}: {
  repo: Repo;
  overview: OverviewInfo | null;
  onGenerated: (info: OverviewInfo) => void;
  onClose: () => void;
}) {
  // The parent mounts this dialog on every open, so the selection always starts from the last run's files or the suggested files.
  const [selected, setSelected] = useState(() => initialPaths(repo.docs, overview));
  const generation = useGeneration();
  const llmBlocked = useLlmBlocked();
  const mounted = useRef(false); // a run that outlives this dialog must not touch the parent state

  const { generating, error } = generation;
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const files = repo.docs.filter((d) => selected.has(d.path));
  const tokens = Math.round(files.reduce((sum, d) => sum + d.size, 0) / 4);
  const overBudget = tokens > MAX_PROMPT_TOKENS;
  const atLimit = files.length >= MAX_FILES;

  function toggle(path: string) {
    const next = new Set(selected);
    if (!next.delete(path)) next.add(path);
    setSelected(next);
  }

  async function generate(infer: boolean) {
    const result = await generation.run(() =>
      generateOverview(repo.root, infer ? [] : files.map((d) => d.path), infer),
    );
    if (result && mounted.current) onGenerated(result);
  }

  const noDocs = repo.docs.length === 0;
  const progress = <GenerationProgress generation={generation} lastDuration={overview?.durationSecs} />;

  return (
    <Dialog open onOpenChange={(open) => !open && !generating && onClose()}>
      <DialogContent>
        <div>
          <DialogTitle>Repository overview</DialogTitle>
          <DialogDescription>
            {noDocs
              ? "No documentation found. The app can infer the overview from the repo structure: file paths and manifest files. No source code is sent. The result can be less accurate."
              : "Select the documentation files that describe this repo. The LLM writes the overview from them."}
          </DialogDescription>
          {overview && (
            <p className="mt-1 text-xs text-muted-foreground">Generated {new Date(overview.generatedAt).toLocaleDateString()}</p>
          )}
          {overview && overview.files.length > 0 && (
            <p className="mt-1 break-words text-xs text-muted-foreground">Files: {overview.files.join(", ")}</p>
          )}
        </div>
        {noDocs ? (
          <>
            {error && (
              <p role="alert" className="text-sm text-destructive">
                {error}
              </p>
            )}
            <div className="flex items-center gap-2 self-end">
              {generating && progress}
              <LlmHint />
              {!generating && (
                <>
                  <Button variant="outline" onClick={onClose}>
                    Cancel
                  </Button>
                  <Button disabled={llmBlocked} onClick={() => generate(true)}>
                    <Sparkles />
                    {overview ? "Regenerate" : "Generate"}
                  </Button>
                </>
              )}
            </div>
          </>
        ) : (
          <>
            <div className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                disabled={generating}
                onClick={() => setSelected(suggestedPaths(repo.docs))}
              >
                Select suggested
              </Button>
              <Button variant="outline" size="sm" disabled={generating} onClick={() => setSelected(new Set())}>
                None
              </Button>
            </div>
            <ul className="flex min-h-0 flex-col gap-0.5 overflow-y-auto rounded-lg border p-1">
              {repo.docs.map((d) => {
                const { name, dir } = splitPath(d.path);
                return (
                  <li key={d.path}>
                    <label className="flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 hover:bg-muted/60">
                      <input
                        type="checkbox"
                        className="accent-primary"
                        checked={selected.has(d.path)}
                        disabled={generating || (atLimit && !selected.has(d.path))}
                        onChange={() => toggle(d.path)}
                      />
                      <span className="min-w-0 flex-1 truncate font-mono text-xs" title={d.path}>
                        <span className="text-muted-foreground">{dir === "" ? "" : `${dir}/`}</span>
                        {name}
                      </span>
                      <span className="shrink-0 text-xs text-muted-foreground">{Math.ceil(d.size / 1024)} KB</span>
                    </label>
                  </li>
                );
              })}
            </ul>
            <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <Info className="size-3.5 shrink-0" />
              The text of the selected files is sent to the LLM.
            </p>
            {error && (
              <div className="flex flex-wrap items-center gap-3">
                <p role="alert" className="text-sm text-destructive">
                  {error}
                </p>
                {error === INSUFFICIENT_SOURCES_ERROR && (
                  <Button variant="outline" size="sm" disabled={generating || llmBlocked} onClick={() => generate(true)}>
                    Infer from structure
                  </Button>
                )}
              </div>
            )}
            <div className="flex flex-wrap items-center justify-between gap-3">
              <p className="text-sm text-muted-foreground tabular-nums">
                {files.length}/{MAX_FILES} files · ~{tokens} tokens
                {atLimit && ` · ${MAX_FILES} files maximum`}
                {overBudget && <span className="text-destructive"> · Too many tokens. Select fewer files.</span>}
              </p>
              <div className="flex items-center gap-2">
                <LlmHint />
                {generating ? (
                  progress
                ) : (
                  <Button disabled={files.length === 0 || overBudget || llmBlocked} onClick={() => generate(false)}>
                    <Sparkles />
                    {overview ? "Regenerate" : "Generate"}
                  </Button>
                )}
              </div>
            </div>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}
