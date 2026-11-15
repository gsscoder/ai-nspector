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
  const [projectId, setProjectId] = useState(repo.projects[0]?.id);
  const [moduleId, setModuleId] = useState<string | null>(null);
  const [data, setData] = useState<ModulesFile | null>(null);
  const project = repo.projects.find((p) => p.id === projectId);
  const module = data?.modules.find((m) => m.id === moduleId);

  return (
    <div className="flex flex-col gap-4">
      <nav className="flex items-center gap-1 text-sm">
        <Button variant="ghost" onClick={onClose}>
          {repo.name}
        </Button>
        <span className="text-muted-foreground">›</span>
        {project ? (
          <Crumb
            label={project.name}
            selected={project.id}
            items={repo.projects.map((p) => ({
              id: p.id,
              label: p.name,
              hint: `${p.language} · ${p.files.length} ${p.files.length === 1 ? "file" : "files"}`,
            }))}
            onSelect={(id) => {
              setProjectId(id);
              setModuleId(null);
              setData(null);
            }}
          />
        ) : (
          <span className="text-muted-foreground">no projects</span>
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
      </nav>
      {!project ? (
        <p className="text-sm text-muted-foreground">No projects found in {repo.root}.</p>
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
