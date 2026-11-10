//! AI 相关命令。只做参数整理与编排，网络细节在 provider 里。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::ai::content;
use crate::ai::prompt;
use crate::ai::prompt::MapDoc;
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMapRequest {
    #[serde(default)]
    pub request_id: String,
    pub prompt: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub ids: Option<Vec<u64>>,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub allow_content: bool,
    #[serde(default)]
    pub max_files: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMapEntryView {
    pub id: u64,
    pub src_name: String,
    pub stem: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMapSkipped {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMapResult {
    pub summary: String,
    pub entries: Vec<AiMapEntryView>,
    pub assumptions: Vec<String>,
    pub unsupported: Vec<String>,
    pub skipped: Vec<AiMapSkipped>,
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

/// 按文件内容逐文件命名。必须显式允许读取内容，且只读常见文本文件。
#[tauri::command]
pub async fn ai_generate_map(
    app: AppHandle,
    state: State<'_, AppState>,
    req: AiMapRequest,
) -> Result<AiMapResult> {
    if !req.allow_content {
        return Err(AppError::Message(
            "需要先勾选「允许读取文件内容」才能按内容命名".into(),
        ));
    }

    let prompt_text = req.prompt.trim();
    if prompt_text.is_empty() {
        return Err(AppError::Message("请先描述你希望怎样按内容命名".into()));
    }
    if prompt_text.chars().count() > 2000 {
        return Err(AppError::Message("需求描述过长，请控制在 2000 字以内".into()));
    }

    let (provider, key) = state.ai.provider(req.provider_id.as_deref())?;
    let max_files = req.max_files.unwrap_or(30).clamp(1, 60);
    let (docs, ext_of, mut skipped, selected_count) = collect_docs(
        &state,
        req.session_id.as_deref(),
        req.ids.as_deref(),
        max_files,
    );

    if docs.is_empty() {
        return Err(AppError::Message(
            "选中的文件里没有可读取的文本内容".into(),
        ));
    }

    let request_id = if req.request_id.trim().is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        req.request_id.clone()
    };
    let cancel = Arc::new(AtomicBool::new(false));
    state.ai.set_cancel(cancel.clone());

    let total_batches = docs.len().div_ceil(8);
    let mut names: Vec<(u64, String)> = Vec::new();
    let mut summary = String::new();
    let mut assumptions: Vec<String> = Vec::new();
    let mut unsupported: Vec<String> = Vec::new();

    for (index, batch) in docs.chunks(8).enumerate() {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            state.ai.clear_cancel();
            return Err(AppError::Message("已取消".into()));
        }

        provider::emit_status(
            &app,
            &request_id,
            &format!("正在按内容命名（{}/{}）…", index + 1, total_batches),
        );

        let messages = prompt::build_map_messages(prompt_text, batch);
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

        let batch_ids: HashSet<u64> = batch.iter().map(|doc| doc.id).collect();
        let parsed = match crate::ai::parse::parse_map_scheme(&text, &batch_ids, &ext_of) {
            Ok(parsed) => parsed,
            Err(error) => {
                provider::emit_status(&app, &request_id, "正在修正输出…");
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

                match crate::ai::parse::parse_map_scheme(&second, &batch_ids, &ext_of) {
                    Ok(parsed) => parsed,
                    Err(error) => {
                        state.ai.clear_cancel();
                        return Err(AppError::Message(format!(
                            "按内容命名失败：{error}"
                        )));
                    }
                }
            }
        };

        if summary.is_empty() {
            summary = parsed.summary;
        }
        names.extend(parsed.names);
        assumptions.extend(parsed.assumptions);
        unsupported.extend(parsed.unsupported);
    }

    state.ai.clear_cancel();

    let returned: HashSet<u64> = names.iter().map(|(id, _)| *id).collect();
    let by_id: HashMap<u64, (String, String)> = docs
        .iter()
        .map(|doc| (doc.id, (doc.name.clone(), doc.ext.clone())))
        .collect();

    for doc in &docs {
        if !returned.contains(&doc.id) {
            skipped.push(AiMapSkipped {
                name: doc.name.clone(),
                reason: "AI 没有返回这个名字".into(),
            });
        }
    }

    let entries = names
        .into_iter()
        .filter_map(|(id, stem)| {
            by_id
                .get(&id)
                .map(|(name, _)| AiMapEntryView {
                    id,
                    src_name: name.clone(),
                    stem,
                })
        })
        .collect();

    dedup(&mut assumptions);
    dedup(&mut unsupported);

    Ok(AiMapResult {
        summary: if summary.is_empty() {
            format!("按内容命名（{selected_count} 个文件）")
        } else {
            summary
        },
        entries,
        assumptions,
        unsupported,
        skipped,
    })
}

fn dedup(items: &mut Vec<String>) {
    let mut seen = HashSet::new();
    items.retain(|item| seen.insert(item.clone()));
    items.truncate(8);
}

fn collect_docs(
    state: &AppState,
    session_id: Option<&str>,
    ids: Option<&[u64]>,
    max_files: usize,
) -> (Vec<MapDoc>, HashMap<u64, String>, Vec<AiMapSkipped>, usize) {
    let picked: Vec<(u64, String, String, String, bool)> = {
        let sessions = state
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(session) = session_id.and_then(|id| sessions.get(id)) else {
            return (Vec::new(), HashMap::new(), Vec::new(), 0);
        };
        let wanted: Option<HashSet<u64>> = ids.map(|list| list.iter().copied().collect());
        session
            .entries
            .iter()
            .filter(|entry| {
                wanted
                    .as_ref()
                    .map_or(true, |wanted| wanted.contains(&entry.id))
            })
            .map(|entry| {
                (
                    entry.id,
                    entry.name.clone(),
                    entry.ext.clone(),
                    entry.dir.clone(),
                    entry.is_dir,
                )
            })
            .collect()
    };

    let selected_count = picked.len();
    let mut docs = Vec::new();
    let mut ext_of = HashMap::new();
    let mut skipped = Vec::new();
    let mut overflow = 0usize;

    for (id, name, ext, dir, is_dir) in picked {
        if docs.len() >= max_files {
            overflow += 1;
            continue;
        }
        if is_dir {
            skipped.push(AiMapSkipped {
                name,
                reason: "文件夹不读取内容".into(),
            });
            continue;
        }
        if !content::supports(&ext) {
            skipped.push(AiMapSkipped {
                name,
                reason: "不是文本文件".into(),
            });
            continue;
        }

        let path = Path::new(&dir).join(&name);
        match content::read_snippet(&path, content::MAX_CHARS) {
            Ok(Some(snippet)) => {
                ext_of.insert(id, ext.clone());
                docs.push(MapDoc {
                    id,
                    name,
                    ext,
                    text: snippet.text,
                    truncated: snippet.truncated,
                });
            }
            Ok(None) => skipped.push(AiMapSkipped {
                name,
                reason: "内容为空或不是文本".into(),
            }),
            Err(error) => skipped.push(AiMapSkipped {
                name,
                reason: format!("读取失败：{error}"),
            }),
        }
    }

    if overflow > 0 {
        skipped.insert(
            0,
            AiMapSkipped {
                name: format!("其余 {overflow} 个文件"),
                reason: format!("超过单次上限（{max_files} 个）"),
            },
        );
    }
    skipped.truncate(20);

    (docs, ext_of, skipped, selected_count)
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
