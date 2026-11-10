//! AI 命名服务：Provider 配置、钥匙串、流式生成与自然语言提取。

pub mod commands;

mod content;
mod parse;
mod prompt;
mod provider;
mod secrets;
mod service;

pub use prompt::ChatMessage;
pub use service::{
    AiProviderConfig, AiProviderKind, AiProvidersView, AiService,
};
