//! AI 相关命令。只做参数整理与编排，网络细节在 provider 里。

use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::ai::prompt;
use crate::ai::provider;
use crate::ai::{AiProviderConfig, AiProvidersView};
use crate::error::{AppError, Result};
use crate::model::{AiSpec, FileEntry};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiGenerateRequest {
    #[serde(default)]
    pub request_id: String,
    pub prompt: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub ids: Option<Vec<u64>>,
    #[serde(default)]
    pub provider_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSchemeResult {
    pub summary: String,
    pub spec: AiSpec,
    pub assumptions: Vec<String>,
    pub unsupported: Vec<String>,
}

#[tauri::command]
pub fn ai_list_providers(state: State<'_, AppState>) -> AiProvidersView {
    state.ai.view()
}

#[tauri::command]
pub fn ai_save_provider(
    state: State<'_, AppState>,
    provider: AiProviderConfig,
) -> Result<AiProvidersView> {
    state.ai.save_provider(provider)
}

#[tauri::command]
pub fn ai_delete_provider(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<AiProvidersView> {
    state.ai.delete_provider(&provider_id)
}

#[tauri::command]
pub fn ai_set_active_provider(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<AiProvidersView> {
    state.ai.set_active(&provider_id)
}

#[tauri::command]
pub fn ai_set_key(
    state: State<'_, AppState>,
    provider_id: String,
    key: String,
) -> Result<()> {
    state.ai.set_key(&provider_id, &key)
}

#[tauri::command]
pub fn ai_delete_key(state: State<'_, AppState>, provider_id: String) -> Result<()> {
    state.ai.delete_key(&provider_id)
}

#[tauri::command]
pub async fn ai_list_models(
    state: State<'_, AppState>,
    provider_id: Option<String>,
) -> Result<Vec<String>> {
    let (provider, key) = state.ai.provider(provider_id.as_deref())?;
    provider::list_models(&provider, key.as_deref()).await
}

#[tauri::command]
pub async fn ai_test_provider(
    state: State<'_, AppState>,
    provider_id: Option<String>,
) -> Result<String> {
    let (provider, key) = state.ai.provider(provider_id.as_deref())?;
    let models = provider::list_models(&provider, key.as_deref()).await?;

    if models.is_empty() {
        return Ok("连接成功，但服务没有返回模型列表".into());
    }
    if !provider.model.trim().is_empty() && !models.iter().any(|m| m == &provider.model) {
        return Ok(format!(
            "连接成功，但当前模型「{}」不在服务返回的列表里",
            provider.model
        ));
    }
    Ok(format!("连接成功，发现 {} 个可用模型", models.len()))
}

#[tauri::command]
pub async fn ai_generate(
    app: AppHandle,
    state: State<'_, AppState>,
    req: AiGenerateRequest,
) -> Result<AiSchemeResult> {
    let prompt_text = req.prompt.trim();
    if prompt_text.is_empty() {
        return Err(AppError::Message("请先描述你的改名需求".into()));
    }
    if prompt_text.chars().count() > 2000 {
        return Err(AppError::Message("需求描述过长，请控制在 2000 字以内".into()));
    }

    let (provider, key) = state.ai.provider(req.provider_id.as_deref())?;
    let (samples, total, extensions) =
        sample_entries(&state, req.session_id.as_deref(), req.ids.as_deref());
    let messages = prompt::build_messages(prompt_text, &samples, total, &extensions);

    let request_id = if req.request_id.trim().is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        req.request_id.clone()
    };
    let cancel = Arc::new(AtomicBool::new(false));
    state.ai.set_cancel(cancel.clone());

    provider::emit_status(&app, &request_id, "正在理解需求…");

    let first = provider::chat_stream(
        &app,
        &request_id,
        &provider,
        key.as_deref(),
        &messages,
        cancel.clone(),
    )
    .await;

    let text = match first {
        Ok(text) => text,
        Err(e) => {
            state.ai.clear_cancel();
            return Err(e);
        }
    };

    provider::emit_status(&app, &request_id, "正在校验方案…");

    let parsed = match crate::ai::parse::parse_scheme(&text) {
        Ok(parsed) => parsed,
        Err(error) => {
            provider::emit_status(&app, &request_id, "正在修正方案…");
            let repair = prompt::repair_messages(&messages, &text, &error);
            let second = provider::chat_stream(
                &app,
                &request_id,
                &provider,
                key.as_deref(),
                &repair,
                cancel.clone(),
            )
            .await;

            let second = match second {
                Ok(text) => text,
                Err(e) => {
                    state.ai.clear_cancel();
                    return Err(e);
                }
            };

            match crate::ai::parse::parse_scheme(&second) {
                Ok(parsed) => parsed,
                Err(error) => {
                    state.ai.clear_cancel();
                    return Err(AppError::Message(format!(
                        "AI 方案无法通过校验：{error}"
                    )));
                }
            }
        }
    };

    state.ai.clear_cancel();

    Ok(AiSchemeResult {
        summary: parsed.summary,
        spec: parsed.spec,
        assumptions: parsed.assumptions,
        unsupported: parsed.unsupported,
    })
}

#[tauri::command]
pub fn ai_cancel(state: State<'_, AppState>) {
    state.ai.cancel();
}

/// 从当前扫描会话里取一段文件名样本，默认不上传路径。
fn sample_entries(
    state: &AppState,
    session_id: Option<&str>,
    ids: Option<&[u64]>,
) -> (Vec<String>, usize, Vec<(String, usize)>) {
    let sessions = state
        .sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(session) = session_id.and_then(|id| sessions.get(id)) else {
        return (Vec::new(), 0, Vec::new());
    };

    let picked: Vec<&FileEntry> = match ids {
        Some(list) => {
            let wanted: HashSet<u64> = list.iter().copied().collect();
            session
                .entries
                .iter()
                .filter(|e| wanted.contains(&e.id))
                .collect()
        }
        None => session.entries.iter().collect(),
    };

    let total = picked.len();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for entry in &picked {
        *counts.entry(entry.ext.to_lowercase()).or_default() += 1;
    }

    let step = (total / 80).max(1);
    let names = picked
        .iter()
        .step_by(step)
        .take(80)
        .map(|e| e.name.chars().take(120).collect())
        .collect();

    let mut extensions: Vec<(String, usize)> = counts.into_iter().collect();
    extensions.sort_by(|a, b| b.1.cmp(&a.1));
    extensions.truncate(8);

    (names, total, extensions)
}
