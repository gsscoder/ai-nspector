import "@xyflow/react/dist/style.css";
import { useEffect, useMemo, useState, type CSSProperties } from "react";
import dagre from "dagre";
import { Handle, Position, ReactFlow, type Edge, type Node, type NodeProps } from "@xyflow/react";
import { Network } from "lucide-react";
import { moduleRelations } from "@/api";
import type { Module, ModuleRelation } from "@/types";

const NODE_WIDTH = 240; // w-60
const NODE_HEIGHT = 128; // h-32
const DIMMED_OPACITY = 0.2;
const HIDDEN_HANDLE: CSSProperties = { visibility: "hidden" };

type ModuleNode = Node<{ module: Module; dimmed: boolean; onOpen: () => void }, "module">;

function ModuleCard({ data: { module: m, dimmed, onOpen } }: NodeProps<ModuleNode>) {
  return (
    <>
      <Handle type="target" position={Position.Top} style={HIDDEN_HANDLE} />
      <button
        className="flex h-32 w-60 flex-col rounded-xl border bg-card p-3 text-left outline-none transition-colors hover:border-foreground/20 hover:bg-muted/40 focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
        style={{ opacity: dimmed ? DIMMED_OPACITY : 1 }}
        onClick={onOpen}
      >
        <div className="truncate font-medium">{m.name}</div>
        <div className="mt-1 line-clamp-2 text-sm text-muted-foreground">{m.purpose}</div>
        <div className="mt-auto pt-2 text-xs text-muted-foreground">
          {m.files.length} {m.files.length === 1 ? "file" : "files"}
        </div>
      </button>
      <Handle type="source" position={Position.Bottom} style={HIDDEN_HANDLE} />
    </>
  );
}

const nodeTypes = { module: ModuleCard };

// Importers end up on top, the modules they import below.
function layoutPositions(modules: Module[], relations: ModuleRelation[]) {
  const graph = new dagre.graphlib.Graph();
  graph.setGraph({ rankdir: "TB", nodesep: 40, ranksep: 80 });
  graph.setDefaultEdgeLabel(() => ({}));
  for (const m of modules) graph.setNode(m.id, { width: NODE_WIDTH, height: NODE_HEIGHT });
  for (const r of relations) graph.setEdge(r.from, r.to);
  dagre.layout(graph);
  return new Map(modules.map((m) => [m.id, graph.node(m.id)]));
}

function Graph({
  modules,
  relations,
  onOpenModule,
}: {
  modules: Module[];
  relations: ModuleRelation[];
  onOpenModule: (id: string) => void;
}) {
  const [hovered, setHovered] = useState<string | null>(null);
  const positions = useMemo(() => layoutPositions(modules, relations), [modules, relations]);

  const nodes = useMemo(() => {
    const linked = new Set([hovered]);
    for (const r of relations) {
      if (r.from === hovered) linked.add(r.to);
      if (r.to === hovered) linked.add(r.from);
    }
    return modules.map<ModuleNode>((m) => {
      const position = positions.get(m.id);
      return {
        id: m.id,
        type: "module",
        position: { x: (position?.x ?? 0) - NODE_WIDTH / 2, y: (position?.y ?? 0) - NODE_HEIGHT / 2 },
        data: { module: m, dimmed: hovered !== null && !linked.has(m.id), onOpen: () => onOpenModule(m.id) },
      };
    });
  }, [modules, relations, positions, hovered, onOpenModule]);

  const edges = useMemo(() => {
    const pairs = new Set(relations.map((r) => `${r.from}>${r.to}`));
    return relations.map<Edge>((r) => {
      const cyclic = pairs.has(`${r.to}>${r.from}`);
      return {
        id: `${r.from}>${r.to}`,
        source: r.from,
        target: r.to,
        markerEnd: cyclic ? "graph-arrow-cycle" : "graph-arrow",
        style: {
          stroke: cyclic ? "var(--destructive)" : "var(--muted-foreground)",
          strokeWidth: Math.min(1 + Math.log2(r.count), 5),
          opacity: hovered === null || r.from === hovered || r.to === hovered ? 1 : DIMMED_OPACITY,
        },
      };
    });
  }, [relations, hovered]);

  return (
    <div className="h-[70vh] min-h-96 overflow-hidden rounded-xl border">
      <svg className="absolute size-0" aria-hidden>
        <defs>
          {[
            ["graph-arrow", "var(--muted-foreground)"],
            ["graph-arrow-cycle", "var(--destructive)"],
          ].map(([id, fill]) => (
            <marker
              key={id}
              id={id}
              viewBox="0 0 10 10"
              refX="10"
              refY="5"
              markerWidth="10"
              markerHeight="10"
              markerUnits="userSpaceOnUse"
              orient="auto"
            >
              <path d="M0 0L10 5L0 10z" style={{ fill }} />
            </marker>
          ))}
        </defs>
      </svg>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        fitView
        minZoom={0.1}
        nodesDraggable={false}
        nodesConnectable={false}
        elementsSelectable={false}
        nodesFocusable={false}
        edgesFocusable={false}
        style={{ background: "transparent" }}
        onNodeMouseEnter={(_, node) => setHovered(node.id)}
        onNodeMouseLeave={() => setHovered(null)}
      />
    </div>
  );
}

export default function ModuleGraph({
  repoRoot,
  projectId,
  modules,
  onOpenModule,
}: {
  repoRoot: string;
  projectId: string;
  modules: Module[];
  onOpenModule: (id: string) => void;
}) {
  const [relations, setRelations] = useState<ModuleRelation[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let stale = false;
    setRelations(null);
    setError(null);
    moduleRelations(repoRoot, projectId)
      .then((r) => !stale && setRelations(r))
      .catch((e) => !stale && setError(String(e)));
    return () => {
      stale = true;
    };
  }, [repoRoot, projectId, modules]);

  if (error)
    return (
      <p role="alert" className="text-sm text-destructive">
        {error}
      </p>
    );
  if (!relations)
    return (
      <div className="flex items-center gap-2">
        <span className="spinner" />
        <span role="status" className="text-sm text-muted-foreground">
          Loading relations…
        </span>
      </div>
    );
  if (relations.length === 0)
    return (
      <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed p-12 text-center">
        <Network className="size-6 text-muted-foreground/60" />
        <p className="text-sm text-muted-foreground">No relations found</p>
      </div>
    );
  return <Graph modules={modules} relations={relations} onOpenModule={onOpenModule} />;
}
