import { useState } from "react";
import { ChevronDown, ChevronRight, Sparkles, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import AgentPromptDialog from "@/components/AgentPromptDialog";
import LlmHint from "@/components/LlmHint";
import { createProjectAgentPrompt } from "@/api";
import { useLlmBlocked } from "@/lib/llmConfigured";
import { useGeneration } from "@/lib/useGeneration";
import type { ModuleReport, ProjectReport } from "@/types";

const STATUS_BADGE: Record<ModuleReport["status"], { label: string; className: string }> = {
  done: { label: "Done", className: "bg-muted text-foreground/80" },
  nothingFound: { label: "Nothing found", className: "bg-muted text-muted-foreground" },
  failed: { label: "Failed", className: "bg-destructive/10 text-destructive dark:bg-destructive/20" },
};

function ModuleBody({ module }: { module: ModuleReport }) {
  switch (module.status) {
    case "done":
      return <p className="text-sm leading-relaxed whitespace-pre-wrap">{module.text}</p>;
    case "nothingFound":
      return <p className="text-sm text-muted-foreground">No findings</p>;
    case "failed":
      return (
        <p role="alert" className="text-sm text-destructive">
          {module.error}
        </p>
      );
  }
}

// Header, scrolling body, footer with the agent prompt.
export default function ProjectReportView({
  repoRoot,
  projectId,
  report,
  analysisName,
  requested,
  onClose,
}: {
  repoRoot: string;
  projectId: string;
  report: ProjectReport;
  analysisName: string;
  requested: number; // modules the run was asked to analyze
  onClose: () => void;
}) {
  const [open, setOpen] = useState(true);
  const [agentPrompt, setAgentPrompt] = useState<string | null>(null); // kept while the dialog is closed
  const [promptOpen, setPromptOpen] = useState(false);
  const generation = useGeneration();
  const llmBlocked = useLlmBlocked();
  const findings = report.modules.filter((m) => m.status === "done").map((m) => ({ moduleId: m.moduleId, text: m.text }));

  async function createPrompt() {
    const text = await generation.run(() => createProjectAgentPrompt(repoRoot, projectId, findings));
    if (text !== null) setAgentPrompt(text);
  }

  return (
    <section className={`flex shrink-0 flex-col rounded-xl border bg-card ${open ? "h-[40vh]" : ""}`}>
      <div className="flex items-center gap-2 px-1.5 py-1.5">
        <Button variant="ghost" size="sm" aria-expanded={open} onClick={() => setOpen(!open)}>
          {open ? <ChevronDown /> : <ChevronRight />}
          {analysisName}
        </Button>
        <span className="truncate text-xs text-muted-foreground">
          {new Date(report.createdAt).toLocaleString()} · {report.model}
        </span>
        {report.cancelled && (
          <span className="shrink-0 rounded-md bg-amber-500/15 px-2 py-0.5 text-xs text-amber-700 dark:text-amber-300">
            Cancelled: {report.modules.length} of {requested} modules finished
          </span>
        )}
        <Button variant="ghost" size="sm" className="ml-auto text-muted-foreground" onClick={onClose}>
          <X />
          Close
        </Button>
      </div>
      {open && (
        <>
          <div className="flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto border-t p-4">
            <ul className="flex flex-col gap-0.5">
              {report.modules.map((m) => (
                <li key={m.moduleId} className="flex items-center gap-2 rounded-md px-2 py-1 text-sm">
                  <a
                    href={`#report-${m.moduleId}`}
                    className="min-w-0 flex-1 truncate rounded-sm outline-none hover:underline focus-visible:ring-3 focus-visible:ring-ring/50"
                  >
                    {m.moduleName}
                  </a>
                  <span className={`shrink-0 rounded-md px-1.5 py-0.5 text-xs ${STATUS_BADGE[m.status].className}`}>
                    {STATUS_BADGE[m.status].label}
                  </span>
                </li>
              ))}
            </ul>
            {report.modules.map((m) => (
              <article key={m.moduleId} id={`report-${m.moduleId}`} className="flex flex-col gap-2">
                <h4 className="font-medium">{m.moduleName}</h4>
                <ModuleBody module={m} />
              </article>
            ))}
          </div>
          <LlmHint className="px-4 pb-2" />
          <div className="flex items-center justify-end gap-2 border-t p-3">
            {findings.length === 0 && <span className="text-xs text-muted-foreground">No findings to act on</span>}
            <Button
              variant="outline"
              disabled={findings.length === 0 || llmBlocked}
              onClick={() => {
                setPromptOpen(true);
                if (agentPrompt === null) createPrompt();
              }}
            >
              <Sparkles />
              Create agent prompt
            </Button>
          </div>
        </>
      )}
      {promptOpen && (
        <AgentPromptDialog
          prompt={agentPrompt}
          progress={generation.generating ? generation : null}
          error={generation.error}
          onChange={setAgentPrompt}
          onRegenerate={createPrompt}
          onClose={() => setPromptOpen(false)}
        />
      )}
    </section>
  );
}
