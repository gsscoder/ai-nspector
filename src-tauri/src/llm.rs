//! Client for OpenAI-compatible Chat Completions APIs.

use std::error::Error;
use std::time::{Duration, Instant};

use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::AppError;
use crate::settings::{LlmConfig, load_llm_config};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const ERROR_BODY_MAX_CHARS: usize = 300;

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct Message {
    content: Option<String>,
}

fn endpoint_url(base_url: &str) -> String {
    format!("{}/chat/completions", base_url.trim_end_matches('/'))
}

fn request_body(model: &str, system: &str, user: &str, json_mode: bool) -> Value {
    let mut body = json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user },
        ],
        "stream": false,
    });
    if json_mode {
        body["response_format"] = json!({ "type": "json_object" });
    }
    body
}

/// Joins the error with its causes: reqwest keeps the useful part ("connection refused") in the source chain.
fn describe_network_error(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        return "Network error: the request timed out.".into();
    }
    let mut text = format!("Network error: {error}");
    let mut cause = error.source();
    while let Some(inner) = cause {
        text.push_str(&format!(": {inner}"));
        cause = inner.source();
    }
    text
}

fn extract_content(body: &str) -> Result<String, AppError> {
    let response: ChatResponse = serde_json::from_str(body).map_err(|e| {
        AppError::Json("Unexpected response from the LLM server (no chat completion)".into(), e)
    })?;
    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| AppError::Llm("The LLM server returned no choices.".into()))?;
    // A retry with the same prompt would be cut off at the same place.
    if choice.finish_reason.as_deref() == Some("length") {
        return Err(AppError::Llm(
            "The model output was cut off because it reached the output limit. Use a model with a larger output limit or a smaller project.".into(),
        ));
    }
    let content = choice.message.content.unwrap_or_default();
    if content.trim().is_empty() {
        return Err(AppError::Llm("The LLM server returned an empty answer.".into()));
    }
    Ok(content)
}

/// Sends one chat completion request and returns the text of the first choice.
/// Cancel-safe: nothing is spawned, so dropping the future drops the request.
pub async fn complete(
    cfg: &LlmConfig,
    system: &str,
    user: &str,
    json_mode: bool,
) -> Result<String, AppError> {
    let client = Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| AppError::Llm(format!("Cannot create the HTTP client: {e}")))?;

    let mut request = client
        .post(endpoint_url(&cfg.base_url))
        .json(&request_body(&cfg.model, system, user, json_mode));
    if !cfg.api_key.is_empty() {
        request = request.bearer_auth(&cfg.api_key);
    }

    let response = request
        .send()
        .await
        .map_err(|e| AppError::Llm(describe_network_error(&e)))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| AppError::Llm(describe_network_error(&e)))?;
    if !status.is_success() {
        let excerpt: String = body.trim().chars().take(ERROR_BODY_MAX_CHARS).collect();
        return Err(AppError::Llm(format!("The LLM server answered HTTP {status}: {excerpt}")));
    }
    extract_content(&body)
}

#[tauri::command]
pub async fn test_connection(app: tauri::AppHandle) -> Result<String, AppError> {
    let cfg = load_llm_config(&app)?;
    let started = Instant::now();
    complete(
        &cfg,
        "You are a connection test.",
        "Reply with the word OK",
        false,
    )
    .await?;
    Ok(format!(
        "OK: {} answered in {:.1} s",
        cfg.model,
        started.elapsed().as_secs_f32()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    const OK_BODY: &str = r#"{"choices":[{"message":{"role":"assistant","content":"hello"}}]}"#;

    /// Serves one canned HTTP response. Returns the base URL and the raw request that arrived.
    async fn serve_once(status_line: &str, body: &str) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
        let reply = format!(
            "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let request = read_request(&mut socket).await;
            socket.write_all(reply.as_bytes()).await.unwrap();
            socket.shutdown().await.unwrap();
            request
        });
        (base_url, server)
    }

    async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
        let mut received = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let read = socket.read(&mut chunk).await.unwrap();
            received.extend_from_slice(&chunk[..read]);
            let text = String::from_utf8_lossy(&received);
            if let Some((head, body)) = text.split_once("\r\n\r\n") {
                let length = head
                    .to_lowercase()
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap()))
                    .unwrap_or(0);
                if body.len() >= length {
                    return text.into_owned();
                }
            }
            assert!(read > 0, "client closed before the request was complete");
        }
    }

    fn config(base_url: &str, api_key: &str) -> LlmConfig {
        LlmConfig {
            base_url: base_url.into(),
            model: "test-model".into(),
            api_key: api_key.into(),
        }
    }

    async fn call(
        status_line: &str,
        body: &str,
        api_key: &str,
        json_mode: bool,
    ) -> (Result<String, AppError>, String) {
        let (base_url, server) = serve_once(status_line, body).await;
        let result = complete(&config(&base_url, api_key), "be brief", "say hi", json_mode).await;
        (result, server.await.unwrap())
    }

    fn json_body(raw_request: &str) -> Value {
        let (_, body) = raw_request.split_once("\r\n\r\n").unwrap();
        serde_json::from_str(body).unwrap()
    }

    #[test]
    fn url_join_works_with_and_without_trailing_slash() {
        assert_eq!(endpoint_url("http://h/v1"), "http://h/v1/chat/completions");
        assert_eq!(endpoint_url("http://h/v1/"), "http://h/v1/chat/completions");
    }

    #[tokio::test]
    async fn posts_to_chat_completions_and_returns_first_choice() {
        let (result, request) = call("200 OK", OK_BODY, "k", false).await;
        assert_eq!(result.unwrap(), "hello");
        assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"), "{request}");
    }

    #[tokio::test]
    async fn body_has_model_messages_and_no_response_format_without_json_mode() {
        let (_, request) = call("200 OK", OK_BODY, "k", false).await;
        assert_eq!(
            json_body(&request),
            json!({
                "model": "test-model",
                "messages": [
                    { "role": "system", "content": "be brief" },
                    { "role": "user", "content": "say hi" },
                ],
                "stream": false,
            })
        );
    }

    #[tokio::test]
    async fn json_mode_adds_response_format() {
        let (_, request) = call("200 OK", OK_BODY, "k", true).await;
        assert_eq!(json_body(&request)["response_format"], json!({ "type": "json_object" }));
    }

    #[tokio::test]
    async fn sends_bearer_header_only_when_key_is_set() {
        let (_, with_key) = call("200 OK", OK_BODY, "secret", false).await;
        assert!(with_key.to_lowercase().contains("authorization: bearer secret\r\n"), "{with_key}");

        let (_, without_key) = call("200 OK", OK_BODY, "", false).await;
        assert!(!without_key.to_lowercase().contains("authorization"), "{without_key}");
    }

    #[tokio::test]
    async fn non_success_status_reports_code_and_first_300_chars_of_body() {
        let long_body = "x".repeat(500);
        let (result, _) = call("401 Unauthorized", &long_body, "k", false).await;
        let error = result.unwrap_err().to_string();
        assert!(error.contains("HTTP 401 Unauthorized"), "{error}");
        assert_eq!(error.matches('x').count(), 300);
    }

    #[tokio::test]
    async fn malformed_json_is_a_shape_error() {
        let (result, _) = call("200 OK", "<html>not json</html>", "k", false).await;
        assert!(result.unwrap_err().to_string().contains("Unexpected response"));
    }

    #[tokio::test]
    async fn empty_choices_and_empty_content_are_errors() {
        let (no_choices, _) = call("200 OK", r#"{"choices":[]}"#, "k", false).await;
        assert!(no_choices.unwrap_err().to_string().contains("no choices"));

        let empty = r#"{"choices":[{"message":{"content":""}}]}"#;
        let (empty_content, _) = call("200 OK", empty, "k", false).await;
        assert!(empty_content.unwrap_err().to_string().contains("empty answer"));
    }

    #[tokio::test]
    async fn answer_cut_off_by_the_output_limit_is_an_error() {
        let cut = r#"{"choices":[{"message":{"content":"{\"modules\":[{\"id\""},"finish_reason":"length"}]}"#;
        let (result, _) = call("200 OK", cut, "k", true).await;
        assert!(result.unwrap_err().to_string().contains("cut off"));

        let stopped = r#"{"choices":[{"message":{"content":"hello"},"finish_reason":"stop"}]}"#;
        let (result, _) = call("200 OK", stopped, "k", true).await;
        assert_eq!(result.unwrap(), "hello");
    }

    #[tokio::test]
    async fn unreachable_server_is_a_network_error() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
        drop(listener);
        let error = complete(&config(&base_url, ""), "s", "u", false).await.unwrap_err().to_string();
        assert!(error.starts_with("Network error"), "{error}");
    }
}
