import { useState } from "react";
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

  return (
    <div className="flex gap-4">
      <aside className="w-72 shrink-0">
        <h2 className="text-xl font-semibold">{module.name}</h2>
        <p className="mb-3 text-sm text-muted-foreground">{module.purpose}</p>
        <ul className="flex flex-col">
          {files.map((f) => {
            const { name, dir } = split(f.path);
            return (
              <li key={f.path}>
                <button
                  className={`w-full rounded px-2 py-1 text-left text-sm hover:bg-muted ${f.path === selected ? "bg-muted" : ""}`}
                  onClick={() => setSelected(f.path)}
                >
                  {name}
                  {!f.primary && <span className="ml-1 text-xs text-muted-foreground">(secondary)</span>}
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
              <span className="truncate text-sm text-muted-foreground">{selected}</span>
              <Button variant="outline" onClick={() => openInEditor(repoRoot, selected)}>
                Open in editor
              </Button>
            </div>
            <CodeView repoPath={repoRoot} path={selected} />
          </>
        ) : (
          <p className="text-sm text-muted-foreground">This module has no files.</p>
        )}
      </section>
    </div>
  );
}
