//! LLM settings. Base URL and model live in `settings.json` in the app data folder.
//! The API key lives only in the OS credential store: it is never written to a file,
//! logged, returned to the frontend or put into an error text.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::error::AppError;

const SETTINGS_FILE: &str = "settings.json";
const KEY_SERVICE: &str = "ai-nspector";
const KEY_USER: &str = "llm-api-key";
const NO_STORE_HINT: &str = "No OS credential store available. On Linux, start a Secret Service provider \
(for example GNOME Keyring or KWallet). The API key is never saved as plain text";

/// How the client talks to the server: plain OpenAI-compatible, or Azure Foundry.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    Openai,
    Azure,
}

/// What the user edits in the Settings screen. Empty strings mean "not set".
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LlmSettings {
    pub provider: Provider,
    pub base_url: String,
    pub model: String,
}

impl LlmSettings {
    fn normalized(self) -> Self {
        Self {
            provider: self.provider,
            base_url: self.base_url.trim().trim_end_matches('/').to_string(),
            model: self.model.trim().to_string(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    provider: Provider,
    base_url: String,
    model: String,
    has_api_key: bool,
}

/// Complete connection data for one LLM call. No `Debug`: it holds the API key.
pub struct LlmConfig {
    pub provider: Provider,
    pub base_url: String,
    pub model: String,
    /// Empty for servers without authentication (for example a local Ollama); never empty for Azure.
    pub api_key: String,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, AppError> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Platform("Cannot find the app data folder".into(), e.into()))?;
    Ok(dir.join(SETTINGS_FILE))
}

fn read_settings(path: &Path) -> Result<LlmSettings, AppError> {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| {
            AppError::Json(format!("Settings file {} is not valid", path.display()), e)
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(LlmSettings::default()),
        Err(e) => Err(AppError::Io(format!("Cannot read {}", path.display()), e)),
    }
}

fn write_settings(path: &Path, settings: &LlmSettings) -> Result<(), AppError> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)
            .map_err(|e| AppError::Io(format!("Cannot create {}", dir.display()), e))?;
    }
    let text = serde_json::to_string_pretty(settings)
        .map_err(|e| AppError::Json("Cannot serialize settings".into(), e))?;
    fs::write(path, text).map_err(|e| AppError::Io(format!("Cannot write {}", path.display()), e))
}

/// The credential store is installed by `keyring` itself on the first `Entry::new`
/// (feature `v1`, once per process); here only its failure is turned into a clear error.
fn key_entry(user: &str) -> Result<keyring::Entry, AppError> {
    if let Err(e) = keyring::Entry::store_status() {
        return Err(AppError::Platform(NO_STORE_HINT.into(), e.into()));
    }
    keyring::Entry::new(KEY_SERVICE, user).map_err(|e| {
        AppError::Platform("Cannot open the OS credential store entry".into(), e.into())
    })
}

/// `Ok(None)` when no key is stored. Also `Ok(None)` when there is no credential store:
/// then no key can exist, and local servers need none.
fn read_api_key(user: &str) -> Result<Option<String>, AppError> {
    if keyring::Entry::store_status().is_err() {
        return Ok(None);
    }
    match key_entry(user)?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AppError::Platform(
            "Cannot read the API key from the OS credential store".into(),
            e.into(),
        )),
    }
}

/// `None` keeps the stored key, an empty string deletes it, other text replaces it.
fn apply_api_key_change(user: &str, change: Option<&str>) -> Result<(), AppError> {
    match change {
        None => Ok(()),
        Some("") => match key_entry(user)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AppError::Platform(
                "Cannot delete the API key from the OS credential store".into(),
                e.into(),
            )),
        },
        Some(key) => key_entry(user)?.set_password(key).map_err(|e| {
            AppError::Platform(
                "Cannot save the API key in the OS credential store".into(),
                e.into(),
            )
        }),
    }
}

fn require_complete(settings: LlmSettings, api_key: String) -> Result<LlmConfig, AppError> {
    if settings.base_url.is_empty() {
        return Err(AppError::Invalid("Set the base URL in Settings.".into()));
    }
    if settings.model.is_empty() {
        let name = match settings.provider {
            Provider::Openai => "model name",
            Provider::Azure => "deployment name",
        };
        return Err(AppError::Invalid(format!("Set the {name} in Settings.")));
    }
    if settings.provider == Provider::Azure && api_key.is_empty() {
        return Err(AppError::Invalid("Set the API key in Settings.".into()));
    }
    Ok(LlmConfig {
        provider: settings.provider,
        base_url: settings.base_url,
        model: settings.model,
        api_key,
    })
}

pub(crate) fn load_llm_config(app: &AppHandle) -> Result<LlmConfig, AppError> {
    let settings = read_settings(&settings_path(app)?)?;
    let api_key = read_api_key(KEY_USER)?.unwrap_or_default();
    require_complete(settings, api_key)
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Result<SettingsView, AppError> {
    let settings = read_settings(&settings_path(&app)?)?;
    Ok(SettingsView {
        provider: settings.provider,
        base_url: settings.base_url,
        model: settings.model,
        has_api_key: read_api_key(KEY_USER)?.is_some(),
    })
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    settings: LlmSettings,
    api_key: Option<String>,
) -> Result<(), AppError> {
    let path = settings_path(&app)?;
    // The key goes first: when the credential store fails, nothing is saved.
    apply_api_key_change(KEY_USER, api_key.as_deref().map(str::trim))?;
    write_settings(&path, &settings.normalized())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(base_url: &str, model: &str) -> LlmSettings {
        LlmSettings {
            provider: Provider::Openai,
            base_url: base_url.into(),
            model: model.into(),
        }
    }

    #[test]
    fn normalized_trims_and_strips_trailing_slash() {
        let result = settings("  https://api.openai.com/v1/ ", " gpt-4o \n").normalized();
        assert_eq!(result, settings("https://api.openai.com/v1", "gpt-4o"));
    }

    #[test]
    fn missing_file_gives_empty_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let result = read_settings(&dir.path().join(SETTINGS_FILE)).unwrap();
        assert_eq!(result, LlmSettings::default());
    }

    #[test]
    fn write_then_read_round_trips_in_camel_case_without_key_field() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join(SETTINGS_FILE);
        write_settings(&path, &settings("http://localhost:11434/v1", "llama3")).unwrap();

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"baseUrl\""));
        assert!(!text.to_lowercase().contains("key"));
        assert_eq!(
            read_settings(&path).unwrap(),
            settings("http://localhost:11434/v1", "llama3")
        );
    }

    #[test]
    fn invalid_file_names_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SETTINGS_FILE);
        fs::write(&path, "not json").unwrap();
        let error = read_settings(&path).unwrap_err().to_string();
        assert!(error.contains(SETTINGS_FILE), "{error}");
    }

    #[test]
    fn config_requires_base_url_then_model() {
        let no_url = require_complete(settings("", "m"), String::new())
            .err()
            .unwrap();
        assert!(no_url.to_string().contains("base URL"));
        let no_model = require_complete(settings("http://x", ""), String::new())
            .err()
            .unwrap();
        assert!(no_model.to_string().contains("model"));
    }

    #[test]
    fn config_accepts_an_empty_key() {
        let config = require_complete(settings("http://x", "m"), String::new())
            .ok()
            .unwrap();
        assert_eq!(config.api_key, "");
    }

    #[test]
    fn old_file_without_provider_loads_as_openai() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SETTINGS_FILE);
        fs::write(&path, r#"{"baseUrl":"http://x","model":"m"}"#).unwrap();
        assert_eq!(read_settings(&path).unwrap().provider, Provider::Openai);
    }

    #[test]
    fn provider_is_saved_as_a_lowercase_string() {
        let azure = LlmSettings {
            provider: Provider::Azure,
            ..settings("https://r.cognitiveservices.azure.com", "dep")
        };
        let text = serde_json::to_string(&azure).unwrap();
        assert!(text.contains(r#""provider":"azure""#), "{text}");
    }

    #[test]
    fn azure_config_requires_a_key_and_names_the_deployment() {
        let azure = |model: &str| LlmSettings {
            provider: Provider::Azure,
            ..settings("https://r.cognitiveservices.azure.com", model)
        };
        let no_key = require_complete(azure("dep"), String::new()).err().unwrap();
        assert!(no_key.to_string().contains("API key"));
        let no_deployment = require_complete(azure(""), "k".into()).err().unwrap();
        assert!(no_deployment.to_string().contains("deployment name"));
        let config = require_complete(azure("dep"), "k".into()).ok().unwrap();
        assert_eq!(config.provider, Provider::Azure);
    }

    /// Touches the real OS credential store, so it is off by default:
    /// `cargo test -- --ignored credential_store_round_trip`
    #[test]
    #[ignore = "uses the real OS credential store"]
    fn credential_store_round_trip() {
        let user = "llm-api-key-test-entry";
        apply_api_key_change(user, Some("secret-1")).unwrap();
        assert_eq!(read_api_key(user).unwrap().as_deref(), Some("secret-1"));
        apply_api_key_change(user, None).unwrap();
        assert_eq!(read_api_key(user).unwrap().as_deref(), Some("secret-1"));
        apply_api_key_change(user, Some("")).unwrap();
        assert_eq!(read_api_key(user).unwrap(), None);
        apply_api_key_change(user, Some("")).unwrap();
    }
}
