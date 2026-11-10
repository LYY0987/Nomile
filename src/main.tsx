import React from "react";
import ReactDOM from "react-dom/client";

import App from "./App";
import { applyTheme, loadTheme } from "./lib/theme";
import "./styles.css";

// 在渲染前就把主题定下来，避免暗色用户先看到一帧白屏
applyTheme(loadTheme());

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
