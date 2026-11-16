import RepoScreen from "@/components/RepoScreen";
import StartScreen from "@/components/StartScreen";
import type { OverviewInfo, Repo } from "@/types";

export default function RepoView({
  repo,
  overview,
  onOpen,
  onOverviewChange,
  onClose,
}: {
  repo: Repo | null;
  overview: OverviewInfo | null | undefined;
  onOpen: (repo: Repo) => void;
  onOverviewChange: (info: OverviewInfo) => void;
  onClose: () => void;
}) {
  if (repo) {
    return <RepoScreen repo={repo} overview={overview} onOverviewChange={onOverviewChange} onClose={onClose} />;
  }
  return <StartScreen onOpen={onOpen} />;
}
