import { useState } from "react";
import { ArrowLeft, ScanSearch, Settings2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import RepoView from "@/components/RepoView";
import Settings from "@/components/Settings";

function App() {
  const [showSettings, setShowSettings] = useState(false);

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
      <main className="flex-1 overflow-auto">
        <div className="mx-auto max-w-7xl p-6">
          <div hidden={showSettings}>
            <RepoView />
          </div>
          {showSettings && <Settings />}
        </div>
      </main>
    </div>
  );
}

export default App;
