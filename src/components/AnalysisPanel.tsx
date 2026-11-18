import { useState } from "react";
import { PanelRightClose, PanelRightOpen, Play } from "lucide-react";
import { Button } from "@/components/ui/button";
import LlmHint from "@/components/LlmHint";
import { useLlmBlocked } from "@/lib/llmConfigured";
import type { AnalysisInfo, ChatFile } from "@/types";

export default function AnalysisPanel({
  analyses,
  chat,
  overviewMissing,
  running,
  onRun,
  onOpenChat,
}: {
  analyses: AnalysisInfo[];
  chat: ChatFile | null | undefined; // undefined: still loading
  overviewMissing: boolean;
  running: boolean;
  onRun: (analysisId: string) => void;
  onOpenChat: () => void;
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
        <p className="mb-2 text-xs text-muted-foreground">Generate the repo overview for better results.</p>
      )}
      <LlmHint className="mb-2" />
      <ul className="flex flex-col gap-2">
        {analyses.map((a) => {
          const hasChat = chat?.analysis === a.id;
          return (
            <li key={a.id} className="rounded-xl border bg-card p-3">
              <div className="text-sm font-medium">{a.name}</div>
              <div className="mt-1 text-sm text-muted-foreground">{a.description}</div>
              <Button
                className="mt-3"
                variant={hasChat ? "outline" : "default"}
                size="sm"
                disabled={!hasChat && (chat !== null || running || llmBlocked)}
                onClick={hasChat ? onOpenChat : () => onRun(a.id)}
              >
                {!hasChat && <Play />}
                {hasChat ? "Open chat" : "Run"}
              </Button>
              {chat && !hasChat && <p className="mt-2 text-xs text-muted-foreground">Discard the current chat first</p>}
            </li>
          );
        })}
      </ul>
    </aside>
  );
}
