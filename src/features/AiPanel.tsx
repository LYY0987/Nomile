import {
  AlertTriangle,
  Bot,
  Check,
  Loader,
  RefreshCw,
  Settings,
  Sparkles,
  Square,
  X,
} from "lucide-react";
import { useEffect, useState } from "react";

import type {
  AiDateSource,
  AiField,
  AiMetaKey,
  AiSegment,
  AiSpec,
  AiTransform,
  RuleInstance,
} from "../lib/types";
import { useAi } from "../store/aiStore";
import { useApp } from "../store/appStore";
import { AiSettings } from "./AiSettings";
import { ModeSwitch } from "./ModeSwitch";

const FIELD_LABEL: Record<AiField, string> = {
  stem: "原文件名",
  parent: "父目录",
  size: "大小",
  index: "序号",
  total: "总数",
};

const DATE_LABEL: Record<AiDateSource, string> = {
  modified: "修改时间",
  created: "创建时间",
  now: "当前时间",
  shot: "拍摄时间",
};

const META_LABEL: Record<AiMetaKey, string> = {
  camera: "相机",
  make: "厂商",
  model: "型号",
  lens: "镜头",
  title: "标题",
  artist: "艺术家",
  album: "专辑",
  track: "曲目",
  duration: "时长",
  width: "宽度",
  height: "高度",
};

const EXAMPLES = [
  "照片按拍摄时间 + 3 位序号",
  "去掉 IMG_ 前缀和括号内容",
  "全小写，空格换下划线",
  "视频用标题，其他保持原名",
];

function segmentLabel(segment: AiSegment): string {
  switch (segment.type) {
    case "literal":
      return segment.text === " " ? "␣" : segment.text || "∅";
    case "field":
      return FIELD_LABEL[segment.field];
    case "sequence":
      return segment.pad > 0 ? `序号 ${segment.pad} 位` : "序号";
    case "date":
      return DATE_LABEL[segment.source];
    case "meta":
      return META_LABEL[segment.key];
    case "extract":
      return `提取 ${segment.pattern}`;
    case "condition":
      return "条件分支";
    case "random":
      return `随机 ${segment.len} 位`;
  }
}

function transformLabel(transform: AiTransform): string {
  switch (transform.type) {
    case "replace":
      return `替换「${transform.pattern}」`;
    case "stripPrefix":
      return `删前缀「${transform.value}」`;
    case "stripSuffix":
      return `删后缀「${transform.value}」`;
    case "removeBrackets":
      return "删括号内容";
    case "stripCopyMarks":
      return "去副本标记";
    case "slice":
      return `截取 ${transform.start}~${transform.end || "末"}`;
    case "case":
      return { lower: "转小写", upper: "转大写", title: "首字母大写", sentence: "句首大写" }[
        transform.mode
      ];
    case "cleanup":
      return "清理字符";
    case "normalizeSeparators":
      return `统一分隔符「${transform.to}」`;
    case "limit":
      return `限长 ${transform.max}`;
  }
}

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
  const setRules = useApp((s) => s.setRules);
  const setPanelMode = useApp((s) => s.setPanelMode);
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
  const loadProviders = useAi((s) => s.loadProviders);
  const generate = useAi((s) => s.generate);
  const cancel = useAi((s) => s.cancel);
  const apply = useAi((s) => s.apply);
  const clearResult = useAi((s) => s.clearResult);

  const [settingsOpen, setSettingsOpen] = useState(false);

  useEffect(() => {
    void loadProviders();
  }, [loadProviders]);

  const active = providers.find((p) => p.id === activeId);
  const applied = !!result && aiSpec === result.spec;
  const canGenerate = !!sessionId && draft.trim().length > 0 && !generating;

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

      <div className="ai-scroll">
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

        {(generating || streamText) && (
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
              {applied && <Check className="ai-ok" />}
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
              <button className="btn primary" onClick={apply}>
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
                onClick={addToStack}
                title="把这条 AI 方案放进手动规则栈"
              >
                加入规则栈
              </button>
              <button className="btn ghost" onClick={() => clearResult()}>
                <X />
                移除方案
              </button>
            </div>
          </div>
        )}
      </div>

      {settingsOpen && <AiSettings onClose={() => setSettingsOpen(false)} />}
    </div>
  );
}
