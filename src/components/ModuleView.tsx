import { useState } from "react";
import { ExternalLink } from "lucide-react";
import { Button } from "@/components/ui/button";
import CodeView from "@/components/CodeView";
import { openInEditor } from "@/api";
import type { Module } from "@/types";

const split = (path: string) => {
  const i = path.lastIndexOf("/");
  return { name: path.slice(i + 1), dir: i < 0 ? "" : path.slice(0, i) };
};

export default function ModuleView({ repoRoot, module }: { repoRoot: string; module: Module }) {
  // Primary files first, then secondary.
  const files = [...module.files].sort((a, b) => Number(b.primary) - Number(a.primary));
  const [selected, setSelected] = useState(files[0]?.path);
  const [openError, setOpenError] = useState<string | null>(null);

  return (
    <div className="flex gap-6">
      <aside className="w-72 shrink-0">
        <h2 className="text-2xl font-semibold tracking-tight">{module.name}</h2>
        <p className="mt-1 mb-4 text-sm text-muted-foreground">{module.purpose}</p>
        <ul className="flex flex-col gap-0.5">
          {files.map((f) => {
            const { name, dir } = split(f.path);
            return (
              <li key={f.path}>
                <button
                  className={`w-full rounded-md border-l-2 px-2.5 py-1.5 text-left text-sm transition-colors hover:bg-muted/60 ${f.path === selected ? "border-primary bg-muted" : "border-transparent"}`}
                  aria-current={f.path === selected ? "true" : undefined}
                  onClick={() => {
                    setSelected(f.path);
                    setOpenError(null);
                  }}
                >
                  <div className="flex items-center gap-2">
                    <span className="truncate">{name}</span>
                    {!f.primary && (
                      <span className="shrink-0 rounded bg-muted px-1.5 text-[11px] text-muted-foreground">secondary</span>
                    )}
                  </div>
                  <div className="truncate text-xs text-muted-foreground">{dir}</div>
                </button>
              </li>
            );
          })}
        </ul>
      </aside>
      <section className="min-w-0 flex-1">
        {selected ? (
          <>
            <div className="mb-2 flex items-center justify-between gap-2">
              <span className="truncate font-mono text-xs text-muted-foreground">{selected}</span>
              <Button
                variant="outline"
                size="sm"
                onClick={() =>
                  openInEditor(repoRoot, selected).then(
                    () => setOpenError(null),
                    (e) => setOpenError(`Cannot open the file: ${String(e)}`),
                  )
                }
              >
                <ExternalLink />
                Open in editor
              </Button>
            </div>
            {openError && (
              <p role="alert" className="mb-2 text-sm text-destructive">
                {openError}
              </p>
            )}
            <CodeView repoPath={repoRoot} path={selected} />
          </>
        ) : (
          <p className="text-sm text-muted-foreground">This module has no files.</p>
        )}
      </section>
    </div>
  );
}
