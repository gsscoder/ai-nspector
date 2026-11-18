import { useLlmBlocked } from "@/lib/llmConfigured";

// Tells why the actions that call the LLM are disabled. A disabled button shows no tooltip, so the reason is inline text.
export default function LlmHint({ className = "" }: { className?: string }) {
  if (!useLlmBlocked()) return null;
  return <p className={`text-xs text-muted-foreground ${className}`}>Configure the LLM in Settings first.</p>;
}
