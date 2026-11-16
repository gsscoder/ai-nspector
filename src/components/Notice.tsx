import type { ReactNode } from "react";
import { Info, X } from "lucide-react";
import { Button } from "@/components/ui/button";

export default function Notice({ children, onDismiss }: { children: ReactNode; onDismiss: () => void }) {
  return (
    <div
      role="status"
      className="flex shrink-0 items-start gap-2 border-b border-notice-foreground/15 bg-notice px-6 py-1.5 text-sm text-notice-foreground"
    >
      <Info className="mt-1 size-4 shrink-0" />
      <p className="min-w-0 flex-1 py-0.5">{children}</p>
      <Button
        variant="ghost"
        size="icon-xs"
        aria-label="Dismiss"
        className="-mr-1.5 hover:bg-notice-foreground/10 hover:text-notice-foreground dark:hover:bg-notice-foreground/10"
        onClick={onDismiss}
      >
        <X />
      </Button>
    </div>
  );
}
