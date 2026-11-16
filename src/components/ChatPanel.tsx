import { useEffect, useRef, useState } from "react";
import { ChevronDown, ChevronRight, Sparkles, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import GenerationProgress from "@/components/GenerationProgress";
import type { ChatFile, ChatMessage } from "@/types";

// The turn the backend is answering: the user message (null for the first analysis) and the streamed reply so far.
export interface PendingTurn {
  question: string | null;
  reply: string;
}

function Message({ message }: { message: ChatMessage }) {
  if (message.role === "assistant") return <p className="text-sm whitespace-pre-wrap">{message.content}</p>;
  return <p className="ml-auto max-w-[85%] rounded-lg bg-muted px-3 py-2 text-sm whitespace-pre-wrap">{message.content}</p>;
}

export default function ChatPanel({
  chat,
  title,
  pending,
  progress,
  error,
  open,
  onToggle,
  onSend,
  onDiscard,
  onCreateAgentPrompt,
}: {
  chat: ChatFile | null; // null while the first analysis runs or after it failed
  title: string;
  pending: PendingTurn | null;
  progress: { stage: string | null; seconds: number } | null; // null when idle
  error: string | null;
  open: boolean;
  onToggle: () => void;
  onSend: (text: string) => Promise<boolean>; // resolves to false when the message was not accepted
  onDiscard: () => Promise<void>;
  onCreateAgentPrompt: () => void;
}) {
  const [text, setText] = useState("");
  const [confirming, setConfirming] = useState(false);
  const [discarding, setDiscarding] = useState(false);
  const [discardError, setDiscardError] = useState<string | null>(null);
  const log = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = log.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [chat, pending, open]);

  const locked = progress !== null || chat === null || chat.outdated;

  async function send() {
    const question = text.trim();
    if (question === "") return;
    setText("");
    if (!(await onSend(question))) setText(question);
  }

  async function discard() {
    setDiscarding(true);
    setDiscardError(null);
    try {
      await onDiscard();
      setConfirming(false);
    } catch (e) {
      setDiscardError(String(e));
    } finally {
      setDiscarding(false);
    }
  }

  return (
    <section className={`flex shrink-0 flex-col rounded-xl border bg-card ${open ? "h-[40vh]" : ""}`}>
      <div className="flex items-center gap-2 px-3 py-2">
        <Button variant="ghost" size="sm" aria-expanded={open} onClick={onToggle}>
          {open ? <ChevronDown /> : <ChevronRight />}
          {title}
        </Button>
        {chat && (
          <span className="truncate text-xs text-muted-foreground">
            {new Date(chat.createdAt).toLocaleString()} · {chat.model}
          </span>
        )}
        {chat?.outdated && (
          <span className="shrink-0 rounded-md bg-amber-500/15 px-2 py-0.5 text-xs text-amber-700 dark:text-amber-300">
            Outdated
          </span>
        )}
        <div className="ml-auto flex items-center gap-2">
          {progress && <GenerationProgress stage={progress.stage} seconds={progress.seconds} />}
          {chat && (
            <Button variant="ghost" size="sm" disabled={progress !== null} onClick={() => setConfirming(true)}>
              <Trash2 />
              Discard
            </Button>
          )}
        </div>
      </div>
      {open && (
        <>
          <div ref={log} className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto border-t px-3 py-3">
            {chat?.messages.map((m, i) => <Message key={i} message={m} />)}
            {pending?.question && <Message message={{ role: "user", content: pending.question }} />}
            {pending && pending.reply !== "" && <Message message={{ role: "assistant", content: pending.reply }} />}
          </div>
          {error && (
            <p role="alert" className="px-3 pb-2 text-sm text-destructive">
              {error}
            </p>
          )}
          {chat?.outdated && (
            <p className="px-3 pb-2 text-sm text-muted-foreground">
              The module files changed. Discard the chat and run the analysis again.
            </p>
          )}
          <div className="flex items-end gap-2 border-t p-3">
            <textarea
              className="min-h-16 w-full resize-none rounded-lg border border-input bg-background px-2.5 py-1.5 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:opacity-50"
              aria-label="Message"
              placeholder="Ask about the findings. Enter sends, Shift+Enter adds a line."
              value={text}
              disabled={locked}
              onChange={(e) => setText(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
                  e.preventDefault();
                  send();
                }
              }}
            />
            <Button variant="outline" disabled={locked} onClick={onCreateAgentPrompt}>
              <Sparkles />
              Create Agent Prompt
            </Button>
          </div>
        </>
      )}
      {confirming && (
        <Dialog open onOpenChange={(o) => !o && !discarding && setConfirming(false)}>
          <DialogContent>
            <div>
              <DialogTitle>Discard chat</DialogTitle>
              <DialogDescription>
                The findings and all messages are deleted. Run the analysis again to get new findings.
              </DialogDescription>
            </div>
            {discardError && (
              <p role="alert" className="text-sm text-destructive">
                {discardError}
              </p>
            )}
            <div className="flex items-center gap-2 self-end">
              <Button variant="outline" disabled={discarding} onClick={() => setConfirming(false)}>
                Cancel
              </Button>
              <Button variant="destructive" disabled={discarding} onClick={discard}>
                <Trash2 />
                Discard
              </Button>
            </div>
          </DialogContent>
        </Dialog>
      )}
    </section>
  );
}
