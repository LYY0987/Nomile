import type { AiProviderKind } from "../lib/types";

export interface AiPreset {
  id: string;
  name: string;
  kind: AiProviderKind;
  baseUrl: string;
  model: string;
}

/** 内置服务预设；密钥仍然只存在系统钥匙串里 */
export const AI_PRESETS: AiPreset[] = [
  {
    id: "ollama",
    name: "Ollama 本地",
    kind: "ollama",
    baseUrl: "http://localhost:11434",
    model: "",
  },
  {
    id: "openai",
    name: "OpenAI",
    kind: "openai",
    baseUrl: "https://api.openai.com/v1",
    model: "gpt-4o-mini",
  },
  {
    id: "deepseek",
    name: "DeepSeek",
    kind: "openai",
    baseUrl: "https://api.deepseek.com/v1",
    model: "deepseek-chat",
  },
  {
    id: "moonshot",
    name: "Kimi / Moonshot",
    kind: "openai",
    baseUrl: "https://api.moonshot.cn/v1",
    model: "moonshot-v1-8k",
  },
  {
    id: "zhipu",
    name: "智谱 GLM",
    kind: "openai",
    baseUrl: "https://open.bigmodel.cn/api/paas/v4",
    model: "glm-4-flash",
  },
  {
    id: "qwen",
    name: "通义千问",
    kind: "openai",
    baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    model: "qwen-plus",
  },
  {
    id: "siliconflow",
    name: "硅基流动",
    kind: "openai",
    baseUrl: "https://api.siliconflow.cn/v1",
    model: "Qwen/Qwen2.5-7B-Instruct",
  },
  {
    id: "openrouter",
    name: "OpenRouter",
    kind: "openai",
    baseUrl: "https://openrouter.ai/api/v1",
    model: "openai/gpt-4o-mini",
  },
  {
    id: "groq",
    name: "Groq",
    kind: "openai",
    baseUrl: "https://api.groq.com/openai/v1",
    model: "llama-3.3-70b-versatile",
  },
  {
    id: "mistral",
    name: "Mistral",
    kind: "openai",
    baseUrl: "https://api.mistral.ai/v1",
    model: "mistral-small-latest",
  },
  {
    id: "xai",
    name: "xAI Grok",
    kind: "openai",
    baseUrl: "https://api.x.ai/v1",
    model: "grok-2-latest",
  },
  {
    id: "custom",
    name: "自定义兼容接口",
    kind: "openai",
    baseUrl: "",
    model: "",
  },
];
