import type { ReactNode } from "react";
import { Info, X } from "lucide-react";
import { Button } from "@/components/ui/button";

const TONES = {
  info: {
    box: "border-notice-foreground/15 bg-notice text-notice-foreground",
    dismiss: "hover:bg-notice-foreground/10 hover:text-notice-foreground dark:hover:bg-notice-foreground/10",
  },
  warning: {
    box: "border-amber-700/20 bg-amber-500/15 text-amber-700 dark:border-amber-300/20 dark:text-amber-300",
    dismiss:
      "hover:bg-amber-500/20 hover:text-amber-700 dark:hover:bg-amber-500/20 dark:hover:text-amber-300",
  },
  error: {
    box: "border-destructive/20 bg-destructive/10 text-destructive dark:bg-destructive/20",
    dismiss: "hover:bg-destructive/20 hover:text-destructive dark:hover:bg-destructive/30 dark:hover:text-destructive",
  },
};

export default function Notice({
  children,
  tone = "info",
  onDismiss,
}: {
  children: ReactNode;
  tone?: keyof typeof TONES;
  onDismiss: () => void;
}) {
  return (
    <div role="status" className={`flex shrink-0 items-start gap-2 border-b px-6 py-1.5 text-sm ${TONES[tone].box}`}>
      <Info className="mt-1 size-4 shrink-0" />
      <p className="min-w-0 flex-1 py-0.5">{children}</p>
      <Button
        variant="ghost"
        size="icon-xs"
        aria-label="Dismiss"
        className={`-mr-1.5 ${TONES[tone].dismiss}`}
        onClick={onDismiss}
      >
        <X />
      </Button>
    </div>
  );
}
