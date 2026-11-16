import { useState } from "react";
import { BookOpen, ChevronDown, ChevronRight, FolderGit2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import ModuleView from "@/components/ModuleView";
import OverviewDialog from "@/components/OverviewDialog";
import ProjectView from "@/components/ProjectView";
import { STACK, projectLabel } from "@/lib/project";
import type { ModulesFile, OverviewInfo, Repo } from "@/types";

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
      <DropdownMenuTrigger render={<Button variant="ghost" size="sm" />}>
        {label}
        <ChevronDown className="text-muted-foreground" />
      </DropdownMenuTrigger>
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

export default function RepoScreen({
  repo,
  overview,
  onOverviewChange,
  onClose,
}: {
  repo: Repo;
  overview: OverviewInfo | null | undefined;
  onOverviewChange: (info: OverviewInfo) => void;
  onClose: () => void;
}) {
  const [projectId, setProjectId] = useState<string | null>(
    repo.projects.length === 1 ? repo.projects[0].id : null,
  );
  const [moduleId, setModuleId] = useState<string | null>(null);
  const [data, setData] = useState<ModulesFile | null>(null);
  const [overviewOpen, setOverviewOpen] = useState(false);
  const project = repo.projects.find((p) => p.id === projectId);
  const module = data?.modules.find((m) => m.id === moduleId);

  return (
    <div className="flex flex-col gap-6">
      <nav className="-mx-2 flex items-center gap-1 text-sm">
        <Button variant="ghost" size="sm" onClick={() => setOverviewOpen(true)}>
          <span className="relative">
            <BookOpen />
            {overview && <span className="absolute -top-0.5 -right-0.5 size-1.5 rounded-full bg-primary" />}
          </span>
          Overview
        </Button>
        <span aria-hidden className="mx-1 h-4 w-px bg-border" />
        <Button
          variant="ghost"
          size="sm"
          onClick={() => {
            setProjectId(null);
            setModuleId(null);
            setData(null);
          }}
        >
          <FolderGit2 className="text-primary" />
          {repo.name}
        </Button>
        {project && <ChevronRight className="size-4 text-muted-foreground" />}
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
            <ChevronRight className="size-4 text-muted-foreground" />
            <Crumb
              label={module.name}
              selected={module.id}
              items={data.modules.map((m) => ({ id: m.id, label: m.name, hint: `${m.files.length} files` }))}
              onSelect={setModuleId}
            />
          </>
        )}
        <Button variant="ghost" size="sm" className="ml-auto text-muted-foreground" onClick={onClose}>
          <X />
          Close repo
        </Button>
      </nav>
      {repo.projects.length === 0 ? (
        <p className="text-sm text-muted-foreground">No projects found in {repo.root}.</p>
      ) : !project ? (
        <section className="flex flex-col gap-4">
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
            {repo.projects.map((p) => (
              <button
                key={p.id}
                className="rounded-xl border bg-card p-4 text-left transition-colors hover:border-primary/50 hover:bg-muted/40"
                onClick={() => setProjectId(p.id)}
              >
                <div className="font-medium">{projectLabel(p)}</div>
                <div className="truncate text-sm text-muted-foreground">{p.root === "" ? "repo root" : p.root}</div>
                <div className="mt-3 flex items-center gap-2 text-xs text-muted-foreground">
                  <span className="rounded-md bg-primary/15 px-2 py-0.5 font-medium text-primary">{STACK[p.language]}</span>
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
      {overviewOpen && (
        <OverviewDialog
          repo={repo}
          overview={overview ?? null}
          onGenerated={(info) => {
            onOverviewChange(info);
            setOverviewOpen(false);
          }}
          onClose={() => setOverviewOpen(false)}
        />
      )}
    </div>
  );
}
