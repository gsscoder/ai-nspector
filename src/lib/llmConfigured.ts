import { createContext, useContext } from "react";

// undefined: not known yet or the load failed; the actions stay enabled and the backend reports its own errors.
export const LlmConfiguredContext = createContext<boolean | undefined>(undefined);

// True when the LLM is known to be not configured: every action that calls it must be disabled.
export function useLlmBlocked() {
  return useContext(LlmConfiguredContext) === false;
}
