//! 流式调用 Ollama / OpenAI 兼容接口。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::ai::{AiProviderConfig, AiProviderKind, ChatMessage};
use crate::error::{AppError, Result};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiDelta {
    request_id: String,
    text: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiStatus {
    request_id: String,
    phase: String,
}

pub fn emit_status(app: &AppHandle, request_id: &str, phase: &str) {
    let _ = app.emit(
        "ai:status",
        AiStatus {
            request_id: request_id.to_string(),
            phase: phase.to_string(),
        },
    );
}

fn emit_delta(app: &AppHandle, request_id: &str, text: &str) {
    let _ = app.emit(
        "ai:delta",
        AiDelta {
            request_id: request_id.to_string(),
            text: text.to_string(),
        },
    );
}

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|e| AppError::Message(format!("网络客户端初始化失败：{e}")))
}

pub async fn chat_stream(
    app: &AppHandle,
    request_id: &str,
    provider: &AiProviderConfig,
    key: Option<&str>,
    messages: &[ChatMessage],
    cancel: Arc<AtomicBool>,
) -> Result<String> {
    let app = app.clone();
    let request_id = request_id.to_string();
    chat_stream_with(provider, key, messages, cancel, move |text| {
        emit_delta(&app, &request_id, text);
    })
    .await
}

/// 与 `chat_stream` 相同，但把流式增量交给调用方，便于脱离 Tauri 单测。
async fn chat_stream_with<F>(
    provider: &AiProviderConfig,
    key: Option<&str>,
    messages: &[ChatMessage],
    cancel: Arc<AtomicBool>,
    mut on_delta: F,
) -> Result<String>
where
    F: FnMut(&str) + Send,
{
    if provider.model.trim().is_empty() {
        return Err(AppError::Message(
            "还没有选择模型，请先在 AI 设置里拉取并选择模型".into(),
        ));
    }

    match provider.kind {
        AiProviderKind::Ollama => {
            ollama_stream(provider, messages, cancel, &mut on_delta).await
        }
        AiProviderKind::OpenAi => {
            openai_stream(provider, key, messages, cancel, &mut on_delta).await
        }
    }
}

async fn ollama_stream<F>(
    provider: &AiProviderConfig,
    messages: &[ChatMessage],
    cancel: Arc<AtomicBool>,
    on_delta: &mut F,
) -> Result<String>
where
    F: FnMut(&str) + Send,
{
    let url = format!(
        "{}/api/chat",
        provider.base_url.trim_end_matches('/')
    );
    let body = json!({
        "model": provider.model,
        "messages": messages,
        "stream": true,
        "format": "json",
        "options": { "temperature": 0.1 }
    });

    let response = client()?
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(net_error)?;

    if !response.status().is_success() {
        return Err(http_error(response).await);
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut full = String::new();

    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            return Err(AppError::Message("已取消".into()));
        }
        let bytes = chunk.map_err(net_error)?;
        buffer.push_str(&String::from_utf8_lossy(&bytes));

        while let Some(pos) = buffer.find('\n') {
            let line: String = buffer.drain(..=pos).collect();
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if let Some(err) = value.get("error").and_then(|e| e.as_str()) {
                return Err(AppError::Message(format!("AI 服务返回错误：{err}")));
            }
            if let Some(content) = value
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
            {
                if !content.is_empty() {
                    full.push_str(content);
                    on_delta(content);
                }
            }
            if value
                .get("done")
                .and_then(|d| d.as_bool())
                .unwrap_or(false)
            {
                break;
            }
        }
    }

    if full.trim().is_empty() {
        return Err(AppError::Message(
            "AI 没有返回内容，请检查模型是否可用后重试".into(),
        ));
    }
    Ok(full)
}

async fn openai_stream<F>(
    provider: &AiProviderConfig,
    key: Option<&str>,
    messages: &[ChatMessage],
    cancel: Arc<AtomicBool>,
    on_delta: &mut F,
) -> Result<String>
where
    F: FnMut(&str) + Send,
{
    let key = key.ok_or_else(|| AppError::Message("请先在 AI 设置里保存 API Key".into()))?;
    let url = format!(
        "{}/chat/completions",
        provider.base_url.trim_end_matches('/')
    );
    let body = json!({
        "model": provider.model,
        "messages": messages,
        "stream": true,
        "temperature": 0.1
    });

    let response = client()?
        .post(&url)
        .bearer_auth(key)
        .json(&body)
        .send()
        .await
        .map_err(net_error)?;

    if !response.status().is_success() {
        return Err(http_error(response).await);
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut full = String::new();

    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            return Err(AppError::Message("已取消".into()));
        }
        let bytes = chunk.map_err(net_error)?;
        buffer.push_str(&String::from_utf8_lossy(&bytes));

        while let Some(pos) = buffer.find('\n') {
            let line: String = buffer.drain(..=pos).collect();
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data == "[DONE]" {
                break;
            }
            let Ok(value) = serde_json::from_str::<Value>(data) else {
                continue;
            };
            if let Some(err) = value.get("error") {
                let message = err
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("未知错误");
                return Err(AppError::Message(format!("AI 服务返回错误：{message}")));
            }
            let choice = &value["choices"][0];
            let content = choice["delta"]["content"]
                .as_str()
                .or_else(|| choice["message"]["content"].as_str());
            if let Some(content) = content {
                if !content.is_empty() {
                    full.push_str(content);
                    on_delta(content);
                }
            }
        }
    }

    if full.trim().is_empty() {
        return Err(AppError::Message(
            "AI 没有返回内容，请检查模型是否可用后重试".into(),
        ));
    }
    Ok(full)
}

pub async fn list_models(
    provider: &AiProviderConfig,
    key: Option<&str>,
) -> Result<Vec<String>> {
    let client = client()?;
    let base = provider.base_url.trim_end_matches('/');

    match provider.kind {
        AiProviderKind::Ollama => {
            let url = format!("{base}/api/tags");
            let response = client.get(&url).send().await.map_err(net_error)?;
            if !response.status().is_success() {
                return Err(http_error(response).await);
            }
            let value: Value = response
                .json()
                .await
                .map_err(|e| AppError::Message(format!("模型列表解析失败：{e}")))?;
            let mut out = Vec::new();
            if let Some(models) = value.get("models").and_then(|m| m.as_array()) {
                for model in models {
                    if let Some(name) = model
                        .get("name")
                        .and_then(|n| n.as_str())
                        .or_else(|| model.get("model").and_then(|n| n.as_str()))
                    {
                        out.push(name.to_string());
                    }
                }
            }
            Ok(normalize_models(out))
        }
        AiProviderKind::OpenAi => {
            let key = key.ok_or_else(|| AppError::Message("请先保存 API Key".into()))?;
            let url = format!("{base}/models");
            let response = client
                .get(&url)
                .bearer_auth(key)
                .send()
                .await
                .map_err(net_error)?;
            if !response.status().is_success() {
                return Err(http_error(response).await);
            }
            let value: Value = response
                .json()
                .await
                .map_err(|e| AppError::Message(format!("模型列表解析失败：{e}")))?;
            let mut out = Vec::new();
            if let Some(data) = value.get("data").and_then(|d| d.as_array()) {
                for model in data {
                    if let Some(id) = model.get("id").and_then(|i| i.as_str()) {
                        out.push(id.to_string());
                    }
                }
            }
            Ok(normalize_models(out))
        }
    }
}

fn normalize_models(mut models: Vec<String>) -> Vec<String> {
    models.sort();
    models.dedup();
    models
}

fn net_error(e: reqwest::Error) -> AppError {
    if e.is_timeout() {
        AppError::Message("AI 服务响应超时，请稍后重试".into())
    } else if e.is_connect() {
        AppError::Message(format!("连接 AI 服务失败：{e}"))
    } else {
        AppError::Message(format!("请求 AI 服务失败：{e}"))
    }
}

async fn http_error(response: reqwest::Response) -> AppError {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    AppError::Message(format!(
        "AI 服务返回 {}：{}",
        status.as_u16(),
        truncate(&text, 300)
    ))
}

fn truncate(text: &str, max: usize) -> String {
    let cleaned = text.replace('\n', " ").trim().to_string();
    cleaned.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn serve_once(body: &'static str, content_type: &'static str) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 8192];
                let _ = stream.read(&mut buffer);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    content_type,
                    body.as_bytes().len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });
        port
    }

    fn provider(kind: AiProviderKind, base_url: String) -> AiProviderConfig {
        AiProviderConfig {
            id: "test".into(),
            name: "test".into(),
            kind,
            base_url,
            model: "mock-model".into(),
        }
    }

    fn messages() -> Vec<ChatMessage> {
        vec![ChatMessage::user("test".into())]
    }

    #[tokio::test]
    async fn parses_ollama_ndjson() {
        let body = "{\"message\":{\"content\":\"hel\"},\"done\":false}\n{\"message\":{\"content\":\"lo\"},\"done\":true}\n";
        let port = serve_once(body, "application/x-ndjson");
        let p = provider(AiProviderKind::Ollama, format!("http://127.0.0.1:{port}"));

        let mut deltas = String::new();
        let out = chat_stream_with(
            &p,
            None,
            &messages(),
            Arc::new(AtomicBool::new(false)),
            |text| deltas.push_str(text),
        )
        .await
        .unwrap();

        assert_eq!(out, "hello");
        assert_eq!(deltas, "hello");
    }

    #[tokio::test]
    async fn parses_openai_sse() {
        let body = "data: {\"choices\":[{\"delta\":{\"content\":\"hel\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\ndata: [DONE]\n\n";
        let port = serve_once(body, "text/event-stream");
        let p = provider(
            AiProviderKind::OpenAi,
            format!("http://127.0.0.1:{port}/v1"),
        );

        let mut deltas = String::new();
        let out = chat_stream_with(
            &p,
            Some("key"),
            &messages(),
            Arc::new(AtomicBool::new(false)),
            |text| deltas.push_str(text),
        )
        .await
        .unwrap();

        assert_eq!(out, "hello");
        assert_eq!(deltas, "hello");
    }
}
