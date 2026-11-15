import { useEffect, useState } from "react";
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
    <div className="flex max-w-xl flex-col gap-4">
      <div>
        <Button onClick={() => open(pickFolder)}>Open folder</Button>
      </div>
      <hr className="border-border" />
      {error && <p className="text-sm text-destructive">{error}</p>}
      {recent.length > 0 && (
        <section>
          <h2 className="mb-3 text-sm font-medium text-muted-foreground">Recent repos</h2>
          <ul className="flex flex-col gap-2">
            {recent.map((path) => (
              <li key={path}>
                <button
                  className="w-full rounded-lg border p-3 text-left transition-colors hover:bg-muted"
                  onClick={() => open(async () => path)}
                >
                  <div className="truncate font-medium">{path.split(/[\\/]/).filter(Boolean).pop() ?? path}</div>
                  <div className="truncate text-xs text-muted-foreground">{path}</div>
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
