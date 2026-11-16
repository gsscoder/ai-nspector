// Tauri invoke wrappers for the LLM settings (Step 3). The API key is write-only:
// the backend never returns it, only whether one is stored.
import { invoke } from "@tauri-apps/api/core";

export type Provider = "openai" | "azure";

export interface LlmSettings {
  provider: Provider;
  baseUrl: string;
  model: string;
}

export interface SettingsView extends LlmSettings {
  hasApiKey: boolean;
}

export const getSettings = () => invoke<SettingsView>("get_settings");

/** apiKey: `undefined` keeps the stored key, `""` deletes it, other text replaces it. */
export const saveSettings = (settings: LlmSettings, apiKey?: string) =>
  invoke<void>("save_settings", { settings, apiKey });

/** Tests the SAVED settings. Resolves with a short success text; rejects with an error text. */
export const testConnection = () => invoke<string>("test_connection");
