import { useState } from "react";
import { Button } from "@/components/ui/button";
import RepoView from "@/components/RepoView";
import Settings from "@/components/Settings";

function App() {
  const [showSettings, setShowSettings] = useState(false);

  return (
    <div className="flex h-screen flex-col bg-background text-foreground">
      <header className="flex items-center justify-between border-b px-4 py-2">
        <h1 className="text-lg font-semibold">ai-nspector</h1>
        <Button variant="outline" onClick={() => setShowSettings(!showSettings)}>
          Settings
        </Button>
      </header>
      <main className="flex-1 overflow-auto p-4">
        {showSettings ? <Settings /> : <RepoView />}
      </main>
    </div>
  );
}

export default App;
