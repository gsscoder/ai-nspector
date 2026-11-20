import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, Square, X } from "lucide-react";
import { cn } from "@/lib/utils";

const buttonClass = "inline-flex h-full w-[46px] items-center justify-center text-muted-foreground transition-colors";

// Minimise, maximise and close for the frameless window. Renders nothing while the OS draws its own frame (Linux).
function WindowControls() {
  const [decorated, setDecorated] = useState<boolean | null>(null);
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const win = getCurrentWindow();
    const sync = () =>
      win
        .isMaximized()
        .then(setMaximized)
        .catch(() => {});
    win
      .isDecorated()
      .then(setDecorated)
      .catch(() => setDecorated(true));
    sync();
    // Snap and Win+Up change the maximised state, and each one fires a resize.
    const unlisten = win.onResized(sync);
    return () => {
      unlisten.then((stop) => stop()).catch(() => {});
    };
  }, []);

  if (decorated !== false) return null;
  const win = getCurrentWindow();
  return (
    <div className="flex h-full shrink-0">
      <button
        type="button"
        aria-label="Minimise"
        title="Minimise"
        className={cn(buttonClass, "hover:bg-accent hover:text-foreground")}
        onClick={() => void win.minimize()}
      >
        <Minus className="size-3.5" />
      </button>
      <button
        type="button"
        aria-label={maximized ? "Restore" : "Maximise"}
        title={maximized ? "Restore" : "Maximise"}
        className={cn(buttonClass, "hover:bg-accent hover:text-foreground")}
        onClick={() => void win.toggleMaximize()}
      >
        {maximized ? <Copy className="size-3 -scale-x-100" /> : <Square className="size-3" />}
      </button>
      <button
        type="button"
        aria-label="Close"
        title="Close"
        className={cn(buttonClass, "hover:bg-[#c42b1c] hover:text-white")}
        onClick={() => void win.close()}
      >
        <X className="size-3.5" />
      </button>
    </div>
  );
}

export default WindowControls;
