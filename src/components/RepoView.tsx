import { useState } from "react";
import RepoScreen from "@/components/RepoScreen";
import StartScreen from "@/components/StartScreen";
import type { Repo } from "@/types";

export default function RepoView() {
  const [repo, setRepo] = useState<Repo | null>(null);

  if (repo) {
    return <RepoScreen repo={repo} onClose={() => setRepo(null)} />;
  }
  return <StartScreen onOpen={setRepo} />;
}
