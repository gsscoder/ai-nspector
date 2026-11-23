import { useCallback, useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { ArrowLeft, GitBranch, ScanSearch, Settings2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import Notice from "@/components/Notice";
import RepoView from "@/components/RepoView";
import Settings from "@/components/Settings";
import WindowControls from "@/components/WindowControls";
import { loadOverview } from "@/api";
import { LlmConfiguredContext } from "@/lib/llmConfigured";
import { getSettings } from "@/settingsApi";
import type { OverviewInfo, Repo } from "@/types";

function App() {
  const [showSettings, setShowSettings] = useState(false);
  const [repo, setRepo] = useState<Repo | null>(null);
  // undefined: not known yet or the load failed; null: the repo has no overview.
  const [overview, setOverview] = useState<OverviewInfo | null | undefined>(undefined);
  const [overviewNoticeDismissedFor, setOverviewNoticeDismissedFor] = useState<Repo | null>(null);
  // undefined: not known yet or the load failed.
  const [llmConfigured, setLlmConfigured] = useState<boolean | undefined>(undefined);
  const [llmNoticeDismissed, setLlmNoticeDismissed] = useState(false);
  // Only a label: when the read fails, the header shows no version.
  const [version, setVersion] = useState<string | null>(null);
  const repoRoot = repo?.root;

  useEffect(() => {
    getVersion().then(setVersion).catch(() => setVersion(null));
  }, []);

  useEffect(() => {
    setOverview(undefined);
    if (repoRoot === undefined) return;
    let stale = false;
    // Only a hint: when the load fails, the dialog still works and generation reports its own errors.
    loadOverview(repoRoot)
      .then((o) => !stale && setOverview(o))
      .catch(() => !stale && setOverview(undefined));
    return () => {
      stale = true;
    };
  }, [repoRoot]);

  // The API key is optional (local servers need none), so only the base URL and the model count.
  const refreshLlmConfigured = useCallback(() => {
    getSettings()
      .then(({ baseUrl, model }) => {
        const configured = baseUrl !== "" && model !== "";
        setLlmConfigured(configured);
        if (configured) setLlmNoticeDismissed(false);
      })
      .catch(() => setLlmConfigured(undefined));
  }, []);

  useEffect(() => {
    if (!showSettings) refreshLlmConfigured();
  }, [showSettings, refreshLlmConfigured]);

  return (
    <div className="flex h-screen flex-col bg-background text-foreground">
      <header
        data-tauri-drag-region
        className="flex h-9 shrink-0 select-none items-center border-b bg-card/40 pl-6"
      >
        <h1 className="pointer-events-none flex flex-1 items-center gap-2 text-sm font-semibold tracking-tight">
          <ScanSearch className="size-4 text-primary" />
          ai-nspector
          {version !== null && <span className="font-normal text-muted-foreground tabular-nums">v{version}</span>}
        </h1>
        {repo?.branch && (
          <span
            data-tauri-drag-region
            className="mr-2 flex select-none items-center gap-1 text-xs text-muted-foreground"
          >
            <GitBranch className="size-3.5" />
            <span data-tauri-drag-region className="max-w-48 truncate">{repo.branch}</span>
          </span>
        )}
        <Button
          variant="ghost"
          size="sm"
          className="mr-2 text-muted-foreground"
          onClick={() => setShowSettings(!showSettings)}
        >
          {showSettings ? <ArrowLeft /> : <Settings2 />}
          {showSettings ? "Back" : "Settings"}
        </Button>
        <WindowControls />
      </header>
      {!showSettings && llmConfigured === false && !llmNoticeDismissed && (
        <Notice tone="error" onDismiss={() => setLlmNoticeDismissed(true)}>
          The LLM is not configured. Core features need the AI and do not work until you set the base URL and the model
          in Settings.
        </Notice>
      )}
      {repo !== null && overview === null && overviewNoticeDismissedFor !== repo && (
        <Notice tone="warning" onDismiss={() => setOverviewNoticeDismissedFor(repo)}>
          This repo has no overview yet. AI answers will be less precise. Use the Overview button to create brief.md and
          overview.md.
        </Notice>
      )}
      <main className="flex flex-1 flex-col overflow-auto p-6">
        <div className="mx-auto min-h-0 w-full max-w-7xl flex-1">
          <div className={showSettings ? "hidden" : "h-full"}>
            <LlmConfiguredContext.Provider value={llmConfigured}>
              <RepoView
                repo={repo}
                overview={overview}
                onOpen={setRepo}
                onOverviewChange={setOverview}
                onClose={() => setRepo(null)}
              />
            </LlmConfiguredContext.Provider>
          </div>
          {showSettings && <Settings onSaved={refreshLlmConfigured} />}
        </div>
      </main>
    </div>
  );
}

export default App;
