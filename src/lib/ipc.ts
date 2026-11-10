// 所有后端调用的唯一入口。命令名与 src-tauri/src/lib.rs 里注册的名字一致；
// 参数名按 Tauri 默认规则由 snake_case 转成 camelCase。

import { invoke } from "@tauri-apps/api/core";

import type {
  ExecuteRequest,
  ExecuteResult,
  OperationSummary,
  PresetFile,
  PreviewRequest,
  PreviewResult,
  ScanRequest,
  ScanResponse,
  Settings,
  UndoResult,
} from "./types";

export const api = {
  scan: (req: ScanRequest) => invoke<ScanResponse>("scan", { req }),

  /** 释放后端扫描会话（前端「清空列表」时调用） */
  clearSession: () => invoke<void>("clear_session"),

  preview: (req: PreviewRequest) => invoke<PreviewResult>("preview", { req }),

  execute: (req: ExecuteRequest) => invoke<ExecuteResult>("execute", { req }),

  undo: (operationId: string) => invoke<UndoResult>("undo", { operationId }),

  listOperations: () => invoke<OperationSummary[]>("list_operations"),

  getSettings: () => invoke<Settings>("get_settings"),

  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),

  getPresets: () => invoke<PresetFile>("get_presets"),

  savePresets: (presets: PresetFile) => invoke<void>("save_presets", { presets }),
};

export function errorText(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return String(error);
}
