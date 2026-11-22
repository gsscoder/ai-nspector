const NAME_FIELD = /"name"\s*:\s*"((?:[^"\\]|\\.)*)"/g;

// Names of the modules found so far in the partial JSON the LLM streams.
export function parseModuleNames(partialJson: string): string[] {
  const names: string[] = [];
  for (const match of partialJson.matchAll(NAME_FIELD)) {
    try {
      const name: unknown = JSON.parse(`"${match[1]}"`);
      if (typeof name === "string") names.push(name);
    } catch {
      // The model wrote an invalid escape sequence; the validation step reports it later.
    }
  }
  return names;
}
