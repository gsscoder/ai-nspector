import { useState } from "react";
import { PanelRightClose, PanelRightOpen, Play } from "lucide-react";
import { Button } from "@/components/ui/button";
import LlmHint from "@/components/LlmHint";
import { useLlmBlocked } from "@/lib/llmConfigured";
import type { AnalysisInfo } from "@/types";

export default function AnalysisPanel({
  analyses,
  overviewMissing,
  running,
  onRun,
}: {
  analyses: AnalysisInfo[];
  overviewMissing: boolean;
  running: boolean;
  onRun: (analysisId: string) => void;
}) {
  const [open, setOpen] = useState(true);
  const llmBlocked = useLlmBlocked();

  if (!open) {
    return (
      <Button variant="ghost" size="icon-sm" aria-label="Show analyses" onClick={() => setOpen(true)}>
        <PanelRightOpen />
      </Button>
    );
  }
  return (
    <aside className="w-64 shrink-0 overflow-y-auto">
      <div className="mb-2 flex items-center justify-between">
        <h3 className="text-xs font-medium tracking-wider text-muted-foreground uppercase">Analyses</h3>
        <Button variant="ghost" size="icon-sm" aria-label="Hide analyses" onClick={() => setOpen(false)}>
          <PanelRightClose />
        </Button>
      </div>
      {overviewMissing && (
        <p className="mb-2 text-xs text-muted-foreground">Generate the repo overview first. The analysis needs it.</p>
      )}
      <LlmHint className="mb-2" />
      <ul className="flex flex-col gap-2">
        {analyses.map((a) => (
          <li key={a.id} className="rounded-xl border bg-card p-3">
            <div className="text-sm font-medium">{a.name}</div>
            <div className="mt-1 text-sm text-muted-foreground">{a.description}</div>
            <Button className="mt-3" size="sm" disabled={running || llmBlocked} onClick={() => onRun(a.id)}>
              <Play />
              Run
            </Button>
          </li>
        ))}
      </ul>
    </aside>
  );
}
