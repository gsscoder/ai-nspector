import { useEffect, useState } from "react";
import { ExternalLink } from "lucide-react";
import { Button } from "@/components/ui/button";
import AgentPromptDialog from "@/components/AgentPromptDialog";
import AnalysisPanel from "@/components/AnalysisPanel";
import ChatPanel, { type PendingTurn } from "@/components/ChatPanel";
import CodeView from "@/components/CodeView";
import {
  createAgentPrompt,
  discardChat,
  listAnalyses,
  loadChat,
  onChatDelta,
  openInEditor,
  sendChatMessage,
  startAnalysis,
} from "@/api";
import { splitPath } from "@/lib/project";
import { useGeneration } from "@/lib/useGeneration";
import type { AnalysisInfo, ChatFile, Module, OverviewInfo } from "@/types";

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
  const [chat, setChat] = useState<ChatFile | null | undefined>(undefined); // undefined: still loading
  const [loadError, setLoadError] = useState<string | null>(null);
  const [chatOpen, setChatOpen] = useState(true);
  const [pending, setPending] = useState<PendingTurn | null>(null);
  const [agentPrompt, setAgentPrompt] = useState<string | null>(null); // kept while the dialog is closed
  const [promptOpen, setPromptOpen] = useState(false);
  const chatGeneration = useGeneration();
  const promptGeneration = useGeneration();

  useEffect(() => {
    let stale = false;
    Promise.all([listAnalyses(), loadChat(repoRoot, projectId, module.id)])
      .then(([list, file]) => {
        if (stale) return;
        setAnalyses(list);
        setChat(file);
      })
      .catch((e) => !stale && setLoadError(String(e)));
    return () => {
      stale = true;
    };
  }, [repoRoot, projectId, module.id]);

  async function runChat(question: string | null, call: () => Promise<ChatFile>) {
    setChatOpen(true);
    setPending({ question, reply: "" });
    const file = await chatGeneration.run(async () => {
      const unlisten = await onChatDelta((text) => setPending((p) => p && { ...p, reply: p.reply + text }));
      try {
        return await call();
      } finally {
        unlisten();
      }
    });
    setPending(null);
    if (!file) return false;
    setChat(file);
    setAgentPrompt(null);
    return true;
  }

  async function discard() {
    await discardChat(repoRoot, projectId, module.id);
    setChat(null);
    setAgentPrompt(null);
  }

  async function createPrompt() {
    const text = await promptGeneration.run(() => createAgentPrompt(repoRoot, projectId, module.id));
    if (text !== null) setAgentPrompt(text);
  }

  const chatProgress = chatGeneration.generating ? chatGeneration : null;
  const promptProgress = promptGeneration.generating ? promptGeneration : null;
  const chatError = chatGeneration.error ?? loadError;

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
            chat={chat}
            overviewMissing={overview === null}
            running={chatProgress !== null}
            onRun={(analysisId) => runChat(null, () => startAnalysis(repoRoot, projectId, module.id, analysisId))}
            onOpenChat={() => setChatOpen(true)}
          />
        </div>
        {(chat || chatProgress || chatError) && (
          <ChatPanel
            chat={chat ?? null}
            title={chat ? (analyses.find((a) => a.id === chat.analysis)?.name ?? chat.analysis) : "Analysis"}
            pending={pending}
            progress={chatProgress}
            error={chatError}
            open={chatOpen}
            onToggle={() => setChatOpen(!chatOpen)}
            onSend={(text) => runChat(text, () => sendChatMessage(repoRoot, projectId, module.id, text))}
            onDiscard={discard}
            onCreateAgentPrompt={() => {
              setPromptOpen(true);
              if (agentPrompt === null) createPrompt();
            }}
          />
        )}
      </div>
      {promptOpen && (
        <AgentPromptDialog
          prompt={agentPrompt}
          progress={promptProgress}
          error={promptGeneration.error}
          onChange={setAgentPrompt}
          onRegenerate={createPrompt}
          onClose={() => setPromptOpen(false)}
        />
      )}
    </div>
  );
}
