import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";

import { api, errorText } from "../lib/ipc";
import type {
  AiProviderConfig,
  AiProviderView,
  AiSchemeResult,
} from "../lib/types";
import { useApp } from "./appStore";

interface DeltaPayload {
  requestId: string;
  text: string;
}

interface StatusPayload {
  requestId: string;
  phase: string;
}

interface AiState {
  providers: AiProviderView[];
  activeId: string;
  generating: boolean;
  status: string;
  streamText: string;
  error: string | null;
  result: AiSchemeResult | null;
  /** 输入框草稿，切换面板后不丢 */
  draft: string;

  setDraft: (draft: string) => void;
  loadProviders: () => Promise<void>;
  saveProvider: (provider: AiProviderConfig) => Promise<boolean>;
  deleteProvider: (providerId: string) => Promise<void>;
  setActive: (providerId: string) => Promise<void>;
  setKey: (providerId: string, key: string) => Promise<boolean>;
  deleteKey: (providerId: string) => Promise<void>;
  loadModels: (providerId?: string | null) => Promise<string[]>;
  testProvider: (providerId?: string | null) => Promise<string | null>;
  generate: (prompt: string) => Promise<void>;
  cancel: () => Promise<void>;
  apply: () => void;
  clearResult: () => void;
}

export const useAi = create<AiState>((set, get) => ({
  providers: [],
  activeId: "",
  generating: false,
  status: "",
  streamText: "",
  error: null,
  result: null,
  draft: "",

  setDraft: (draft) => set({ draft }),

  loadProviders: async () => {
    try {
      const view = await api.aiListProviders();
      set({ providers: view.providers, activeId: view.activeId });
    } catch (e) {
      useApp.getState().notify(errorText(e), "err");
    }
  },

  saveProvider: async (provider) => {
    try {
      const view = await api.aiSaveProvider(provider);
      set({ providers: view.providers, activeId: view.activeId });
      return true;
    } catch (e) {
      useApp.getState().notify(errorText(e), "err");
      return false;
    }
  },

  deleteProvider: async (providerId) => {
    try {
      const view = await api.aiDeleteProvider(providerId);
      set({ providers: view.providers, activeId: view.activeId });
    } catch (e) {
      useApp.getState().notify(errorText(e), "err");
    }
  },

  setActive: async (providerId) => {
    try {
      const view = await api.aiSetActiveProvider(providerId);
      set({ providers: view.providers, activeId: view.activeId });
    } catch (e) {
      useApp.getState().notify(errorText(e), "err");
    }
  },

  setKey: async (providerId, key) => {
    try {
      await api.aiSetKey(providerId, key);
      await get().loadProviders();
      useApp.getState().notify("API Key 已保存到系统钥匙串", "ok");
      return true;
    } catch (e) {
      useApp.getState().notify(errorText(e), "err");
      return false;
    }
  },

  deleteKey: async (providerId) => {
    try {
      await api.aiDeleteKey(providerId);
      await get().loadProviders();
      useApp.getState().notify("已删除该服务的 API Key", "ok");
    } catch (e) {
      useApp.getState().notify(errorText(e), "err");
    }
  },

  loadModels: async (providerId) => {
    try {
      return await api.aiListModels(providerId ?? null);
    } catch (e) {
      useApp.getState().notify(errorText(e), "err");
      return [];
    }
  },

  testProvider: async (providerId) => {
    try {
      const message = await api.aiTestProvider(providerId ?? null);
      useApp.getState().notify(message, "ok");
      return message;
    } catch (e) {
      useApp.getState().notify(errorText(e), "err");
      return null;
    }
  },

  generate: async (prompt) => {
    const app = useApp.getState();
    if (!app.sessionId) {
      app.notify("请先载入文件列表，AI 需要参考文件名", "err");
      return;
    }

    const provider = get().providers.find((p) => p.id === get().activeId);
    if (!provider) {
      app.notify("还没有配置 AI 服务", "err");
      return;
    }
    if (!provider.model.trim()) {
      app.notify("请先在 AI 设置里选择模型", "err");
      return;
    }

    const requestId =
      typeof crypto !== "undefined" && "randomUUID" in crypto
        ? crypto.randomUUID()
        : `ai-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;

    set({
      generating: true,
      status: "正在准备…",
      streamText: "",
      error: null,
      result: null,
    });

    let offDelta = () => {};
    let offStatus = () => {};
    try {
      offDelta = await listen<DeltaPayload>("ai:delta", (event) => {
        if (event.payload.requestId !== requestId) return;
        set((state) => {
          const next = state.streamText + event.payload.text;
          return { streamText: next.length > 6000 ? next.slice(-6000) : next };
        });
      });
      offStatus = await listen<StatusPayload>("ai:status", (event) => {
        if (event.payload.requestId === requestId) {
          set({ status: event.payload.phase });
        }
      });
    } catch {
      // 事件通道不可用时仍然能拿到最终的 invoke 结果
    }

    try {
      const ids = app.selected.length === app.entries.length ? null : app.selected;
      const result = await api.aiGenerate({
        requestId,
        prompt: prompt.trim(),
        sessionId: app.sessionId,
        ids,
      });
      set({ result, status: "方案已生成" });
    } catch (e) {
      const text = errorText(e);
      if (text.includes("取消")) {
        set({ status: "已取消" });
      } else {
        set({ error: text, status: "生成失败" });
      }
    } finally {
      offDelta();
      offStatus();
      set({ generating: false });
    }
  },

  cancel: async () => {
    try {
      await api.aiCancel();
      set({ status: "正在停止…" });
    } catch {
      // 没有正在执行的任务时忽略
    }
  },

  apply: () => {
    const result = get().result;
    if (!result) return;
    const app = useApp.getState();
    app.setAiSpec(result.spec);
    app.setPanelMode("ai");
    app.notify("AI 方案已应用到预览", "ok");
  },

  clearResult: () => {
    const { result } = get();
    const app = useApp.getState();
    // 只移除「当前结果对应的」已应用方案，避免误清别的状态
    if (result && app.aiSpec === result.spec) {
      app.setAiSpec(null);
    }
    set({ result: null, streamText: "", error: null, status: "" });
  },
}));
