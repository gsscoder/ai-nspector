import { useState } from "react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import ModuleView from "@/components/ModuleView";
import ProjectView from "@/components/ProjectView";
import { STACK, projectLabel } from "@/lib/project";
import type { ModulesFile, Repo } from "@/types";

function Crumb({
  label,
  items,
  selected,
  onSelect,
}: {
  label: string;
  items: { id: string; label: string; hint?: string }[];
  selected: string;
  onSelect: (id: string) => void;
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger render={<Button variant="ghost" />}>{label}</DropdownMenuTrigger>
      <DropdownMenuContent className="w-auto min-w-64">
        {items.map((i) => (
          <DropdownMenuCheckboxItem key={i.id} checked={i.id === selected} onCheckedChange={() => onSelect(i.id)}>
            {i.label}
            {i.hint && <span className="text-muted-foreground">{i.hint}</span>}
          </DropdownMenuCheckboxItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export default function RepoScreen({ repo, onClose }: { repo: Repo; onClose: () => void }) {
  const [projectId, setProjectId] = useState<string | null>(
    repo.projects.length === 1 ? repo.projects[0].id : null,
  );
  const [moduleId, setModuleId] = useState<string | null>(null);
  const [data, setData] = useState<ModulesFile | null>(null);
  const project = repo.projects.find((p) => p.id === projectId);
  const module = data?.modules.find((m) => m.id === moduleId);

  return (
    <div className="flex flex-col gap-4">
      <nav className="flex items-center gap-1 text-sm">
        <Button
          variant="ghost"
          onClick={() => {
            setProjectId(null);
            setModuleId(null);
            setData(null);
          }}
        >
          {repo.name}
        </Button>
        {project && <span className="text-muted-foreground">›</span>}
        {project && (
          <Crumb
            label={projectLabel(project)}
            selected={project.id}
            items={repo.projects.map((p) => ({
              id: p.id,
              label: projectLabel(p),
              hint: `${STACK[p.language]} · ${p.files.length} ${p.files.length === 1 ? "file" : "files"}`,
            }))}
            onSelect={(id) => {
              setProjectId(id);
              setModuleId(null);
              setData(null);
            }}
          />
        )}
        {module && data && (
          <>
            <span className="text-muted-foreground">›</span>
            <Crumb
              label={module.name}
              selected={module.id}
              items={data.modules.map((m) => ({ id: m.id, label: m.name, hint: `${m.files.length} files` }))}
              onSelect={setModuleId}
            />
          </>
        )}
        <Button variant="outline" className="ml-auto" onClick={onClose}>
          Close repo
        </Button>
      </nav>
      {repo.projects.length === 0 ? (
        <p className="text-sm text-muted-foreground">No projects found in {repo.root}.</p>
      ) : !project ? (
        <section className="flex flex-col gap-3">
          <h2 className="text-xl font-semibold">Projects</h2>
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
            {repo.projects.map((p) => (
              <button
                key={p.id}
                className="rounded-lg border p-4 text-left hover:bg-muted"
                onClick={() => setProjectId(p.id)}
              >
                <div className="font-medium">{projectLabel(p)}</div>
                <div className="truncate text-sm text-muted-foreground">{p.root === "" ? "repo root" : p.root}</div>
                <div className="mt-2 flex items-center gap-2 text-xs text-muted-foreground">
                  <span className="rounded bg-muted px-2 py-0.5 text-foreground">{STACK[p.language]}</span>
                  {p.files.length} {p.files.length === 1 ? "file" : "files"}
                </div>
              </button>
            ))}
          </div>
        </section>
      ) : module ? (
        <ModuleView key={module.id} repoRoot={repo.root} module={module} />
      ) : (
        <ProjectView
          key={project.id}
          repoRoot={repo.root}
          project={project}
          data={data}
          setData={setData}
          onOpenModule={setModuleId}
        />
      )}
    </div>
  );
}
