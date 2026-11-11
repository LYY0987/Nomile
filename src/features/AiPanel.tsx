import {
  AlertTriangle,
  Bot,
  Check,
  LayersPlus,
  Loader,
  RefreshCw,
  Save,
  Settings,
  SlidersHorizontal,
  Sparkles,
  Square,
  Trash2,
} from "lucide-react";
import { useEffect, useState } from "react";

import type { AiSpec, RuleInstance } from "../lib/types";
import { useAi } from "../store/aiStore";
import { useApp } from "../store/appStore";
import { segmentLabel, transformLabel } from "./aiSpecLabels";
import { AiContentPanel } from "./AiContentPanel";
import { AiSettings } from "./AiSettings";
import { AiSpecEditor } from "./AiSpecEditor";
import { ModeSwitch } from "./ModeSwitch";

const EXAMPLES = [
  "照片按拍摄时间 + 3 位序号",
  "去掉 IMG_ 前缀和括号内容",
  "全小写，空格换下划线",
  "视频用标题，其他保持原名",
];

function extLabel(spec: AiSpec): string {
  switch (spec.extension.mode) {
    case "keep":
      return "原扩展名";
    case "lower":
      return "扩展名小写";
    case "upper":
      return "扩展名大写";
    case "set":
      return spec.extension.value ? `.${spec.extension.value}` : "去掉扩展名";
  }
}

export function AiPanel() {
  const sessionId = useApp((s) => s.sessionId);
  const aiSpec = useApp((s) => s.aiSpec);
  const rules = useApp((s) => s.rules);
  const presets = useApp((s) => s.presets);
  const setRules = useApp((s) => s.setRules);
  const setPanelMode = useApp((s) => s.setPanelMode);
  const saveAiPreset = useApp((s) => s.saveAiPreset);
  const notify = useApp((s) => s.notify);

  const providers = useAi((s) => s.providers);
  const activeId = useAi((s) => s.activeId);
  const generating = useAi((s) => s.generating);
  const status = useAi((s) => s.status);
  const streamText = useAi((s) => s.streamText);
  const error = useAi((s) => s.error);
  const result = useAi((s) => s.result);
  const draft = useAi((s) => s.draft);
  const setDraft = useAi((s) => s.setDraft);
  const aiMode = useAi((s) => s.aiMode);
  const setAiMode = useAi((s) => s.setAiMode);
  const loadProviders = useAi((s) => s.loadProviders);
  const generate = useAi((s) => s.generate);
  const cancel = useAi((s) => s.cancel);
  const apply = useAi((s) => s.apply);
  const updateSpec = useAi((s) => s.updateSpec);
  const loadSpec = useAi((s) => s.loadSpec);
  const clearResult = useAi((s) => s.clearResult);

  const [settingsOpen, setSettingsOpen] = useState(false);
  const [editing, setEditing] = useState(false);
  const [presetOpen, setPresetOpen] = useState(false);
  const [presetName, setPresetName] = useState("");
  const [presetPick, setPresetPick] = useState("");

  useEffect(() => {
    void loadProviders();
  }, [loadProviders]);

  const active = providers.find((p) => p.id === activeId);
  const applied = !!result && aiSpec === result.spec;
  const canGenerate = !!sessionId && draft.trim().length > 0 && !generating;
  const aiPresets = presets.filter((preset) =>
    preset.rules.some((rule) => rule.rule.kind === "ai")
  );

  function applyAiPreset(id: string) {
    setPresetPick("");
    const preset = aiPresets.find((p) => p.id === id);
    if (!preset) return;
    const rule = preset.rules.find((r) => r.rule.kind === "ai");
    if (!rule || rule.rule.kind !== "ai") return;
    loadSpec(rule.rule.spec, `预设：${preset.name}`);
  }

  async function savePresetNow() {
    if (!result) return;
    const name = presetName.trim() || `AI 方案 ${aiPresets.length + 1}`;
    await saveAiPreset(name, result.spec);
    setPresetName("");
    setPresetOpen(false);
  }

  function addToStack() {
    if (!result) return;
    const instance: RuleInstance = {
      id: `ai-${Date.now().toString(36)}`,
      enabled: true,
      target: "name",
      rule: { kind: "ai", spec: result.spec },
    };
    setRules([...rules, instance]);
    setPanelMode("rules");
    notify("AI 方案已加入手动规则栈", "ok");
  }

  return (
    <div className="sidebar">
      <ModeSwitch />

      <div className="panel-head">
        <Bot />
        <span className="panel-title">AI 重命名</span>
        <div className="spacer" />
        <button
          className="btn icon"
          title="AI 服务设置"
          onClick={() => setSettingsOpen(true)}
        >
          <Settings />
        </button>
      </div>

      <div className="ai-mode-switch">
        <button
          className={aiMode === "template" ? "is-active" : ""}
          onClick={() => {
            setAiMode("template");
            setEditing(false);
          }}
        >
          规则命名
        </button>
        <button
          className={aiMode === "content" ? "is-active" : ""}
          onClick={() => {
            setAiMode("content");
            setEditing(false);
          }}
        >
          按内容命名
        </button>
      </div>

      {aiMode === "template" ? (
        <div className="ai-scroll">
        {aiPresets.length > 0 && (
          <div className="ai-preset-row">
            <span className="ai-preset-label">我的 AI 预设</span>
            <select
              className="input small"
              value={presetPick}
              onChange={(event) => applyAiPreset(event.currentTarget.value)}
            >
              <option value="">选择预设…</option>
              {aiPresets.map((preset) => (
                <option key={preset.id} value={preset.id}>
                  {preset.name}
                </option>
              ))}
            </select>
          </div>
        )}

        {!sessionId && (
          <div className="ai-hint">
            先在上方载入一个目录或一批文件，AI 才能参考文件名生成方案。
          </div>
        )}

        <div className="ai-card">
          <div className="ai-label">
            <Sparkles />
            用一句话描述改名需求
          </div>
          <textarea
            className="ai-textarea"
            value={draft}
            placeholder="例如：把照片按拍摄时间命名，格式 20250506，加 3 位序号"
            onChange={(e) => setDraft(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
                e.preventDefault();
                if (canGenerate) void generate(draft);
              }
            }}
          />
          <div className="ai-examples">
            {EXAMPLES.map((example) => (
              <button
                key={example}
                className="ai-example"
                onClick={() => setDraft(example)}
              >
                {example}
              </button>
            ))}
          </div>
          <div className="ai-generate-row">
            <button
              className="btn primary"
              disabled={!canGenerate}
              onClick={() => void generate(draft)}
            >
              {generating ? <Loader className="spin" /> : <Sparkles />}
              {generating ? "生成中…" : "生成方案"}
            </button>
            {generating && (
              <button className="btn" onClick={() => void cancel()}>
                <Square />
                停止
              </button>
            )}
            <div className="spacer" />
            <button
              className="ai-provider"
              title="切换 AI 服务"
              onClick={() => setSettingsOpen(true)}
            >
              {active
                ? `${active.name}${active.model ? ` · ${active.model}` : " · 未选模型"}`
                : "未配置服务"}
            </button>
          </div>
        </div>

        {generating && (
          <div className="ai-stream">
            <div className="ai-stream-head">
              <Loader className="spin" />
              {status || "正在生成…"}
            </div>
            <pre>{streamText || "正在等待模型输出…"}</pre>
          </div>
        )}

        {error && (
          <div className="ai-error">
            <AlertTriangle />
            <span>{error}</span>
            <button className="btn small" onClick={() => clearResult()}>
              知道了
            </button>
          </div>
        )}

        {result && (
          <div className="ai-result">
            <div className="ai-result-head">
              <span className="ai-badge">{applied ? "已应用" : "方案已生成"}</span>
              <div className="spacer" />
              <Check className="ai-ok" />
            </div>

            <div className="ai-summary">{result.summary}</div>

            <div className="ai-section">
              <span className="ai-section-label">命名结构</span>
              <div className="ai-structure">
                {result.spec.segments.map((segment, index) => (
                  <span className="ai-seg" key={index}>
                    {segmentLabel(segment)}
                  </span>
                ))}
                <span className="ai-seg ai-seg-ext">{extLabel(result.spec)}</span>
              </div>
            </div>

            {result.spec.transforms.length > 0 && (
              <div className="ai-section">
                <span className="ai-section-label">预处理</span>
                <div className="ai-structure">
                  {result.spec.transforms.map((transform, index) => (
                    <span className="ai-seg ai-seg-t" key={index}>
                      {transformLabel(transform)}
                    </span>
                  ))}
                </div>
              </div>
            )}

            {result.assumptions.length > 0 && (
              <div className="ai-section">
                <span className="ai-section-label">假设</span>
                <ul className="ai-list">
                  {result.assumptions.map((item, index) => (
                    <li key={index}>{item}</li>
                  ))}
                </ul>
              </div>
            )}

            {result.unsupported.length > 0 && (
              <div className="ai-section ai-unsupported">
                <span className="ai-section-label">母模板暂不支持</span>
                <ul className="ai-list">
                  {result.unsupported.map((item, index) => (
                    <li key={index}>{item}</li>
                  ))}
                </ul>
              </div>
            )}

            <div className="ai-actions">
              <button className="btn primary ai-action-main" onClick={apply}>
                <Check />
                应用到预览
              </button>
              <button
                className="btn"
                disabled={generating || !draft.trim()}
                onClick={() => void generate(draft)}
              >
                <RefreshCw />
                重新生成
              </button>
              <button
                className="btn"
                onClick={() => setEditing(true)}
                title="直接修改母模板槽位"
              >
                <SlidersHorizontal />
                编辑槽位
              </button>
              <button
                className="btn"
                onClick={() => setPresetOpen((open) => !open)}
                title="把当前 AI 方案保存为可复用预设"
              >
                <Save />
                存为预设
              </button>
              <button
                className="btn"
                onClick={addToStack}
                title="把这条 AI 方案放进手动规则栈"
              >
                <LayersPlus />
                加入规则栈
              </button>
              <button
                className="btn ai-action-remove"
                onClick={() => clearResult()}
              >
                <Trash2 />
                移除方案
              </button>
            </div>

            {presetOpen && (
              <div className="ai-save-preset">
                <input
                  className="input small"
                  placeholder="预设名称"
                  value={presetName}
                  onChange={(event) => setPresetName(event.currentTarget.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") void savePresetNow();
                  }}
                />
                <button
                  className="btn small"
                  onClick={() => void savePresetNow()}
                >
                  <Save />
                  保存
                </button>
              </div>
            )}
          </div>
        )}
        </div>
      ) : (
        <AiContentPanel />
      )}

      {settingsOpen && <AiSettings onClose={() => setSettingsOpen(false)} />}
      {editing && result && (
        <AiSpecEditor
          spec={result.spec}
          onSave={(spec) => {
            updateSpec(spec);
            setEditing(false);
          }}
          onClose={() => setEditing(false)}
        />
      )}
    </div>
  );
}
