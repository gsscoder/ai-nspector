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
    <div className="mx-auto flex max-w-xl flex-col gap-4">
      <div>
        <Button onClick={() => open(pickFolder)}>Open folder</Button>
      </div>
      {error && <p className="text-sm text-destructive">{error}</p>}
      {recent.length > 0 && (
        <section>
          <h2 className="mb-2 text-sm font-medium text-muted-foreground">Recent repos</h2>
          <ul>
            {recent.map((path) => (
              <li key={path}>
                <button
                  className="w-full truncate rounded px-2 py-1 text-left text-sm hover:bg-muted"
                  onClick={() => open(async () => path)}
                >
                  {path}
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
