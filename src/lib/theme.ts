import { getCurrentWindow } from "@tauri-apps/api/window";

export type Theme = "light" | "dark";

const STORAGE_KEY = "nomile.theme";

/** 读偏好：本地存过就用存的，否则跟随系统 */
export function loadTheme(): Theme {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === "light" || saved === "dark") return saved;
  } catch {
    // localStorage 不可用（隐私模式等）时忽略
  }
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

/** 只改 DOM，不写盘：启动时用它，免得把系统偏好当成用户的选择存下来 */
export function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
  // 让下拉框、复选框、滚动条这些原生控件也跟着变深浅
  document.documentElement.style.colorScheme = theme;

  // 系统标题栏也换成深色/浅色；不在 Tauri 里（比如纯网页预览）会失败，忽略即可
  void getCurrentWindow()
    .setTheme(theme)
    .catch(() => {});
}

/** 用户主动切换时才写盘 */
export function saveTheme(theme: Theme) {
  try {
    localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // 存不进去不影响本次使用
  }
}
