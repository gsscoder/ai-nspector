import { useEffect, useState, type ReactNode } from "react";
import { Button } from "@/components/ui/button";
import {
  getSettings,
  saveSettings,
  testConnection,
  type LlmSettings,
  type Provider,
} from "@/settingsApi";

type Busy = "load" | "save" | "test" | "removeKey";
type Status = { ok: boolean; text: string };

const PROVIDERS: { value: Provider; label: string }[] = [
  { value: "openai", label: "OpenAI-compatible" },
  { value: "azure", label: "Azure Foundry" },
];
const LOCAL_HOSTS = ["localhost", "127.0.0.1", "[::1]"];
const inputClass = "field h-8";

/** True when the key would travel unencrypted to a machine other than this one. */
function isUnencryptedRemote(baseUrl: string): boolean {
  try {
    const url = new URL(baseUrl.trim());
    return url.protocol === "http:" && !LOCAL_HOSTS.includes(url.hostname);
  } catch {
    return false;
  }
}

function Field(props: { label: string; hint?: string; action?: ReactNode; children: ReactNode }) {
  return (
    <div className="space-y-1.5">
      <div className="flex items-end gap-2">
        <label className="block flex-1 space-y-1.5">
          <span className="text-sm font-medium">{props.label}</span>
          {props.children}
        </label>
        {props.action}
      </div>
      {props.hint && <span className="block text-xs text-muted-foreground">{props.hint}</span>}
    </div>
  );
}

export default function Settings({ onSaved }: { onSaved?: () => void }) {
  const [saved, setSaved] = useState<LlmSettings>({ provider: "openai", baseUrl: "", model: "" });
  const [form, setForm] = useState<LlmSettings>({ provider: "openai", baseUrl: "", model: "" });
  const [apiKey, setApiKey] = useState("");
  const [hasApiKey, setHasApiKey] = useState(false);
  const [busy, setBusy] = useState<Busy | null>("load");
  const [status, setStatus] = useState<Status | null>(null);

  const azure = form.provider === "azure";
  const dirty =
    form.provider !== saved.provider ||
    form.baseUrl.trim() !== saved.baseUrl ||
    form.model.trim() !== saved.model ||
    apiKey.trim() !== "";

  async function reload() {
    const { hasApiKey: keyStored, ...values } = await getSettings();
    setSaved(values);
    setForm(values);
    setHasApiKey(keyStored);
    setApiKey("");
  }

  async function persist() {
    await saveSettings(form, apiKey.trim() === "" ? undefined : apiKey);
    await reload();
    onSaved?.();
  }

  async function run(kind: Busy, action: () => Promise<string | void>) {
    setBusy(kind);
    setStatus(null);
    try {
      const text = await action();
      if (text) setStatus({ ok: true, text });
    } catch (error) {
      setStatus({ ok: false, text: String(error) });
    } finally {
      setBusy(null);
    }
  }

  useEffect(() => {
    void run("load", reload);
  }, []);

  const onSave = () =>
    run("save", async () => {
      await persist();
      return "Settings saved.";
    });

  const onTest = () =>
    run("test", async () => {
      if (dirty) await persist();
      return testConnection();
    });

  const onRemoveKey = () =>
    run("removeKey", async () => {
      await saveSettings(saved, "");
      setHasApiKey(false);
      onSaved?.();
      return "API key removed.";
    });

  const disabled = busy !== null;
  const keyMissing = azure && !hasApiKey && apiKey.trim() === "";

  return (
    <form
      className="mx-auto max-w-xl space-y-5"
      onSubmit={(event) => {
        event.preventDefault();
        void onSave();
      }}
    >
      <div>
        <h2 className="text-xl font-semibold tracking-tight">LLM settings</h2>
        <p className="mt-1 text-sm text-muted-foreground">
          Works with any OpenAI-compatible API: OpenAI, OpenRouter, Azure, Ollama, LM Studio.
        </p>
      </div>

      <div role="group" aria-label="Connection mode" className="flex w-fit gap-0.5 rounded-lg border bg-card p-0.5">
        {PROVIDERS.map(({ value, label }) => (
          <Button
            key={value}
            type="button"
            size="sm"
            variant={form.provider === value ? "secondary" : "ghost"}
            className={form.provider === value ? "" : "text-muted-foreground"}
            aria-pressed={form.provider === value}
            disabled={disabled}
            onClick={() => setForm({ ...form, provider: value })}
          >
            {label}
          </Button>
        ))}
      </div>

      <Field
        label="Base URL"
        hint={
          azure
            ? "Resource endpoint. The full Target URI from the Azure portal also works."
            : undefined
        }
      >
        <input
          className={inputClass}
          placeholder={
            azure ? "https://<resource>.cognitiveservices.azure.com" : "https://api.openai.com/v1"
          }
          value={form.baseUrl}
          disabled={disabled}
          onChange={(e) => setForm({ ...form, baseUrl: e.target.value })}
        />
      </Field>
      {isUnencryptedRemote(form.baseUrl) && (
        <p className="text-xs text-amber-600 dark:text-amber-400">
          Warning: this URL uses http:// on a remote host. The API key and your code structure
          would travel unencrypted.
        </p>
      )}

      <Field label={azure ? "Deployment name" : "Model"}>
        <input
          className={inputClass}
          placeholder="gpt-4o-mini"
          value={form.model}
          disabled={disabled}
          onChange={(e) => setForm({ ...form, model: e.target.value })}
        />
      </Field>

      <Field
        label={azure ? "API key (required)" : "API key"}
        hint={
          hasApiKey
            ? "A key is stored in the OS credential store. Leave empty to keep it."
            : azure
              ? "Required. Saved in the OS credential store."
              : "Saved in the OS credential store. Leave empty for local servers without a key."
        }
        action={
          hasApiKey && (
            <Button type="button" variant="outline" disabled={disabled} onClick={onRemoveKey}>
              Remove key
            </Button>
          )
        }
      >
        <input
          className={inputClass}
          type="password"
          autoComplete="off"
          placeholder={hasApiKey ? "Key stored" : ""}
          value={apiKey}
          disabled={disabled}
          onChange={(e) => setApiKey(e.target.value)}
        />
      </Field>

      <div className="flex items-center gap-2 pt-1">
        <Button type="submit" disabled={disabled || !dirty || keyMissing}>
          Save
        </Button>
        <Button type="button" variant="outline" disabled={disabled} onClick={onTest}>
          {busy === "test" && (
            <span className="spinner size-3.5" />
          )}
          {busy === "test" ? "Testing…" : "Test connection"}
        </Button>
      </div>

      {status && (
        <p
          role={status.ok ? "status" : "alert"}
          className={status.ok ? "text-sm text-green-600 dark:text-green-400" : "text-sm text-destructive"}
        >
          {status.text}
        </p>
      )}
    </form>
  );
}
