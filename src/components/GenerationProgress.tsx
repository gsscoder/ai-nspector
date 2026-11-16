import { Button } from "@/components/ui/button";
import { cancelGeneration } from "@/api";

export default function GenerationProgress({ stage, seconds }: { stage: string | null; seconds: number }) {
  return (
    <div className="flex items-center gap-2">
      <span className="size-4 animate-spin rounded-full border-2 border-current border-t-transparent" />
      <span role="status" className="text-sm text-muted-foreground">
        {stage}… {seconds}s
      </span>
      <Button variant="outline" size="sm" onClick={cancelGeneration}>
        Cancel
      </Button>
    </div>
  );
}
