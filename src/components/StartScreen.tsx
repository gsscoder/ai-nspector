import { useEffect, useState } from "react";
import { ChevronRight, FolderGit2, FolderOpen, ScanSearch } from "lucide-react";
import { Button } from "@/components/ui/button";
import { listRecentRepos, openRepo, pickFolder } from "@/api";
import type { Repo } from "@/types";

export default function StartScreen({ onOpen }: { onOpen: (repo: Repo) => void }) {
  const [recent, setRecent] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    listRecentRepos().then(setRecent).catch((e) => setError(String(e)));
  }, []);

  async function open(findPath: () => Promise<string | null>) {
    setError(null);
    try {
      const path = await findPath();
      if (path) onOpen(await openRepo(path));
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="mx-auto flex max-w-xl flex-col gap-8 pt-12">
      <div className="flex flex-col items-center gap-3 text-center">
        <div className="rounded-xl border bg-card p-3">
          <ScanSearch className="size-7 text-primary" />
        </div>
        <h2 className="text-2xl font-semibold tracking-tight">Understand and improve any codebase</h2>
        <p className="text-sm text-muted-foreground">
          Open a repository to see its projects and the logical modules that AI finds in them.
        </p>
        <Button size="lg" className="mt-2 px-4" onClick={() => open(pickFolder)}>
          <FolderOpen />
          Open folder
        </Button>
      </div>
      {error && <p className="text-sm text-destructive">{error}</p>}
      {recent.length > 0 && (
        <section>
          <h3 className="mb-2 text-xs font-medium tracking-wider text-muted-foreground uppercase">Recent</h3>
          <ul className="divide-y overflow-hidden rounded-xl border bg-card">
            {recent.map((path) => (
              <li key={path}>
                <button
                  className="group flex w-full items-center gap-3 px-4 py-3 text-left outline-none transition-colors hover:bg-muted/50 focus-visible:bg-muted/50"
                  title={path}
                  onClick={() => open(async () => path)}
                >
                  <FolderGit2 className="size-4 shrink-0 text-muted-foreground transition-colors group-hover:text-foreground" />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium">{path.split(/[\\/]/).filter(Boolean).pop() ?? path}</div>
                    <div className="truncate text-xs text-muted-foreground">{path}</div>
                  </div>
                  <ChevronRight className="size-4 shrink-0 text-muted-foreground/50 transition-colors group-hover:text-muted-foreground" />
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
