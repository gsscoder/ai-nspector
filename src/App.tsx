import { useCallback, useEffect, useState } from "react";
import { ArrowLeft, ScanSearch, Settings2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import Notice from "@/components/Notice";
import RepoView from "@/components/RepoView";
import Settings from "@/components/Settings";
import { loadOverview } from "@/api";
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
  const repoRoot = repo?.root;

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
      <header className="flex h-12 shrink-0 items-center justify-between border-b bg-card/40 px-4">
        <h1 className="flex items-center gap-2 font-semibold tracking-tight">
          <ScanSearch className="size-5 text-primary" />
          ai-nspector
        </h1>
        <Button variant="ghost" size="sm" onClick={() => setShowSettings(!showSettings)}>
          {showSettings ? <ArrowLeft /> : <Settings2 />}
          {showSettings ? "Back" : "Settings"}
        </Button>
      </header>
      {!showSettings && llmConfigured === false && !llmNoticeDismissed && (
        <Notice onDismiss={() => setLlmNoticeDismissed(true)}>
          The LLM is not configured. Open Settings and set the base URL and the model.
        </Notice>
      )}
      {repo !== null && overview === null && overviewNoticeDismissedFor !== repo && (
        <Notice onDismiss={() => setOverviewNoticeDismissedFor(repo)}>
          This repo has no overview yet. Use the Overview button to create brief.md and overview.md.
        </Notice>
      )}
      <main className="flex flex-1 flex-col overflow-auto p-6">
        <div className="mx-auto min-h-0 w-full max-w-7xl flex-1">
          <div className={showSettings ? "hidden" : "h-full"}>
            <RepoView
              repo={repo}
              overview={overview}
              onOpen={setRepo}
              onOverviewChange={setOverview}
              onClose={() => setRepo(null)}
            />
          </div>
          {showSettings && <Settings onSaved={refreshLlmConfigured} />}
        </div>
      </main>
    </div>
  );
}

export default App;
