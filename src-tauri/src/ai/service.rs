//! Provider 配置与运行期取消令牌。

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::ai::secrets;
use crate::error::{AppError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiProviderKind {
    Ollama,
    OpenAi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderConfig {
    pub id: String,
    pub name: String,
    pub kind: AiProviderKind,
    pub base_url: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiConfig {
    pub providers: Vec<AiProviderConfig>,
    pub active_id: String,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            providers: vec![AiProviderConfig {
                id: "ollama".into(),
                name: "Ollama 本地".into(),
                kind: AiProviderKind::Ollama,
                base_url: "http://localhost:11434".into(),
                model: String::new(),
            }],
            active_id: "ollama".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderView {
    pub id: String,
    pub name: String,
    pub kind: AiProviderKind,
    pub base_url: String,
    pub model: String,
    pub has_key: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProvidersView {
    pub providers: Vec<AiProviderView>,
    pub active_id: String,
}

pub struct AiService {
    path: PathBuf,
    config: Mutex<AiConfig>,
    cancel: Mutex<Option<Arc<AtomicBool>>>,
}

impl AiService {
    pub fn new(root: PathBuf) -> Self {
        let path = root.join("ai-providers.json");
        let config = load_config(&path);
        Self {
            path,
            config: Mutex::new(config),
            cancel: Mutex::new(None),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, AiConfig> {
        self.config
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn persist(&self, config: &AiConfig) -> Result<()> {
        let text = serde_json::to_string_pretty(config)?;
        fs::write(&self.path, text)?;
        Ok(())
    }

    pub fn view(&self) -> AiProvidersView {
        let config = self.lock();
        AiProvidersView {
            providers: config
                .providers
                .iter()
                .map(|p| AiProviderView {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    kind: p.kind,
                    base_url: p.base_url.clone(),
                    model: p.model.clone(),
                    has_key: secrets::has(&p.id),
                })
                .collect(),
            active_id: config.active_id.clone(),
        }
    }

    pub fn save_provider(&self, mut provider: AiProviderConfig) -> Result<AiProvidersView> {
        provider.id = provider.id.trim().to_string();
        provider.name = provider.name.trim().to_string();
        provider.base_url = provider.base_url.trim().trim_end_matches('/').to_string();
        provider.model = provider.model.trim().to_string();

        if provider.id.is_empty() {
            return Err(AppError::Message("服务 id 不能为空".into()));
        }
        if provider.name.is_empty() {
            return Err(AppError::Message("服务名称不能为空".into()));
        }
        if provider.base_url.is_empty() {
            return Err(AppError::Message("接口地址不能为空".into()));
        }

        {
            let mut config = self.lock();
            match config.providers.iter_mut().find(|p| p.id == provider.id) {
                Some(existing) => *existing = provider.clone(),
                None => config.providers.push(provider.clone()),
            }
            if config.active_id.is_empty() {
                config.active_id = provider.id.clone();
            }
            self.persist(&config)?;
        }
        Ok(self.view())
    }

    pub fn delete_provider(&self, provider_id: &str) -> Result<AiProvidersView> {
        {
            let mut config = self.lock();
            config.providers.retain(|p| p.id != provider_id);
            if config.active_id == provider_id {
                config.active_id = config
                    .providers
                    .first()
                    .map(|p| p.id.clone())
                    .unwrap_or_default();
            }
            self.persist(&config)?;
        }
        secrets::delete(provider_id);
        Ok(self.view())
    }

    pub fn set_active(&self, provider_id: &str) -> Result<AiProvidersView> {
        {
            let mut config = self.lock();
            if !config.providers.iter().any(|p| p.id == provider_id) {
                return Err(AppError::Message(format!("找不到 AI 服务：{provider_id}")));
            }
            config.active_id = provider_id.to_string();
            self.persist(&config)?;
        }
        Ok(self.view())
    }

    pub fn set_key(&self, provider_id: &str, key: &str) -> Result<()> {
        let key = key.trim();
        if key.is_empty() {
            return Err(AppError::Message("密钥不能为空".into()));
        }
        {
            let config = self.lock();
            if !config.providers.iter().any(|p| p.id == provider_id) {
                return Err(AppError::Message(format!("找不到 AI 服务：{provider_id}")));
            }
        }
        secrets::set(provider_id, key)
    }

    pub fn delete_key(&self, provider_id: &str) -> Result<()> {
        {
            let config = self.lock();
            if !config.providers.iter().any(|p| p.id == provider_id) {
                return Err(AppError::Message(format!("找不到 AI 服务：{provider_id}")));
            }
        }
        secrets::delete(provider_id);
        Ok(())
    }

    /// 取出 provider 与密钥。未指定时用当前默认服务。
    pub fn provider(&self, provider_id: Option<&str>) -> Result<(AiProviderConfig, Option<String>)> {
        let config = self.lock();
        let provider = match provider_id {
            Some(id) => config
                .providers
                .iter()
                .find(|p| p.id == id)
                .cloned()
                .ok_or_else(|| AppError::Message(format!("找不到 AI 服务：{id}")))?,
            None => config
                .providers
                .iter()
                .find(|p| p.id == config.active_id)
                .or_else(|| config.providers.first())
                .cloned()
                .ok_or_else(|| AppError::Message("还没有配置 AI 服务".into()))?,
        };
        let key = match provider.kind {
            AiProviderKind::Ollama => None,
            AiProviderKind::OpenAi => secrets::get(&provider.id),
        };
        Ok((provider, key))
    }

    pub fn set_cancel(&self, flag: Arc<AtomicBool>) {
        let mut guard = self
            .cancel
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = Some(flag);
    }

    pub fn clear_cancel(&self) {
        let mut guard = self
            .cancel
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = None;
    }

    pub fn cancel(&self) {
        let guard = self
            .cancel
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(flag) = guard.as_ref() {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

fn load_config(path: &PathBuf) -> AiConfig {
    let mut config = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<AiConfig>(&text).ok())
        .filter(|c| !c.providers.is_empty())
        .unwrap_or_default();

    if !config.providers.iter().any(|p| p.id == config.active_id) {
        config.active_id = config
            .providers
            .first()
            .map(|p| p.id.clone())
            .unwrap_or_default();
    }
    config
}
