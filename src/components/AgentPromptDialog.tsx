import { useEffect, useState } from "react";
import { Check, Copy, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import GenerationProgress from "@/components/GenerationProgress";
import LlmHint from "@/components/LlmHint";
import { useLlmBlocked } from "@/lib/llmConfigured";

export default function AgentPromptDialog({
  prompt,
  progress,
  error,
  onChange,
  onRegenerate,
  onClose,
}: {
  prompt: string | null; // null until the first generation succeeds
  progress: { stage: string | null; seconds: number } | null; // null when idle
  error: string | null;
  onChange: (text: string) => void;
  onRegenerate: () => void;
  onClose: () => void;
}) {
  const [copied, setCopied] = useState(false);
  const [copyError, setCopyError] = useState<string | null>(null);
  const generating = progress !== null;
  const llmBlocked = useLlmBlocked();
  const shownError = copyError ?? error;

  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), 2000);
    return () => clearTimeout(timer);
  }, [copied]);

  async function copy() {
    try {
      await navigator.clipboard.writeText(prompt ?? "");
      setCopyError(null);
      setCopied(true);
    } catch (e) {
      setCopyError(`Cannot copy the prompt: ${String(e)}`);
    }
  }

  return (
    <Dialog open onOpenChange={(open) => !open && !generating && onClose()}>
      <DialogContent>
        <div>
          <DialogTitle>Agent prompt</DialogTitle>
          <DialogDescription>
            Edit the prompt if needed, then copy it into your coding agent. It lists the changes the findings call for.
          </DialogDescription>
        </div>
        <textarea
          className="field h-80 resize-none py-2 font-mono text-xs leading-relaxed"
          aria-label="Agent prompt"
          value={prompt ?? ""}
          disabled={generating}
          onChange={(e) => onChange(e.target.value)}
        />
        {shownError && (
          <p role="alert" className="text-sm text-destructive">
            {shownError}
          </p>
        )}
        <div className="flex flex-wrap items-center justify-end gap-2">
          {generating ? (
            <GenerationProgress stage={progress.stage} seconds={progress.seconds} />
          ) : (
            <>
              <LlmHint />
              <Button variant="outline" disabled={llmBlocked} onClick={onRegenerate}>
                <Sparkles />
                {prompt === null ? "Generate" : "Regenerate"}
              </Button>
              <Button variant="outline" onClick={onClose}>
                Close
              </Button>
              <Button disabled={prompt === null || prompt === ""} onClick={copy}>
                {copied ? <Check /> : <Copy />}
                {copied ? "Copied" : "Copy"}
              </Button>
            </>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
