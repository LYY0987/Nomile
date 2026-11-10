import {
  Braces,
  Brackets,
  CaseSensitive,
  ChevronDown,
  ChevronUp,
  ListOrdered,
  Plus,
  Replace,
  Save,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";
import { useState } from "react";

import type {
  CaseKind,
  CleanupRule,
  RuleInstance,
  RuleKind,
  SeqPosition,
  SpaceMode,
  Target,
} from "../lib/types";
import { useApp } from "../store/appStore";

const KIND_LABEL: Record<RuleKind, string> = {
  replace: "查找替换",
  sequence: "序号",
  template: "模板",
  case: "大小写",
  affix: "前后缀",
  cleanup: "清理",
};

/** 每类规则配一个图标，一列卡片快速扫一眼就能分辨 */
const KIND_ICON: Record<RuleKind, typeof Replace> = {
  replace: Replace,
  sequence: ListOrdered,
  template: Braces,
  case: CaseSensitive,
  affix: Brackets,
  cleanup: Sparkles,
};

function KindIcon({ kind }: { kind: RuleKind }) {
  const Icon = KIND_ICON[kind];
  return <Icon />;
}

/** 模板里最常用的几个变量，点一下追加到模板末尾 */
const QUICK_TOKENS: [string, string][] = [
  ["拍摄时间", "${shot}"],
  ["序号", "${n:3}"],
  ["相机", "${camera}"],
  ["标题", "${title}"],
  ["时长", "${duration:mmss}"],
  ["扩展名", "${ext}"],
];

const ADD_ORDER: RuleKind[] = [
  "affix",
  "sequence",
  "template",
  "replace",
  "case",
  "cleanup",
];

export function RulePanel() {
  const rules = useApp((s) => s.rules);
  const addRule = useApp((s) => s.addRule);
  const clearRules = useApp((s) => s.clearRules);
  const presets = useApp((s) => s.presets);
  const savePreset = useApp((s) => s.savePreset);
  const applyPreset = useApp((s) => s.applyPreset);
  const deletePreset = useApp((s) => s.deletePreset);

  const [presetName, setPresetName] = useState("");

  return (
    <div className="sidebar">
      <div className="panel-head">
        <span className="panel-title">规则栈</span>
        <span className="panel-count">{rules.length} 条</span>
        <div className="spacer" />
        {rules.length > 0 && (
          <button className="btn icon" title="清空规则" onClick={clearRules}>
            <Trash2 />
          </button>
        )}
      </div>

      <div className="rule-list">
        {rules.length === 0 && (
          <div className="empty" style={{ padding: "24px 8px" }}>
            <div className="big">还没有规则</div>
            <div className="sub">
              从下方添加一条规则，右边会立刻显示每个文件改名前后的对比。
              <br />
              规则从上往下依次生效，前一条的结果是后一条的输入。
            </div>
          </div>
        )}

        {rules.map((inst, index) => (
          <RuleCard
            key={inst.id}
            inst={inst}
            index={index}
            total={rules.length}
          />
        ))}
      </div>

      <div className="add-bar">
        <div className="add-grid">
          {ADD_ORDER.map((kind) => (
            <button
              key={kind}
              className="add-btn"
              onClick={() => addRule(kind)}
            >
              <Plus />
              {KIND_LABEL[kind]}
            </button>
          ))}
        </div>

        <div className="preset-bar">
          <input
            className="input small"
            placeholder="预设名称"
            value={presetName}
            onChange={(e) => setPresetName(e.currentTarget.value)}
            style={{ flex: 1 }}
          />
          <button
            className="btn small"
            style={{ height: 26 }}
            onClick={() => {
              void savePreset(presetName.trim() || `预设 ${presets.length + 1}`);
              setPresetName("");
            }}
            disabled={rules.length === 0}
          >
            <Save />
            存为预设
          </button>
        </div>

        {presets.length > 0 && (
          <div className="preset-list">
            {presets.map((p) => (
              <span className="chip" key={p.id}>
                <button
                  style={{
                    background: "transparent",
                    border: "none",
                    padding: 0,
                    color: "inherit",
                    fontSize: 11,
                  }}
                  onClick={() => applyPreset(p.id)}
                  title={`应用「${p.name}」`}
                >
                  {p.name}
                </button>
                <button onClick={() => void deletePreset(p.id)} title="删除">
                  <X size={12} />
                </button>
              </span>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function RuleCard({
  inst,
  index,
  total,
}: {
  inst: RuleInstance;
  index: number;
  total: number;
}) {
  const patchRule = useApp((s) => s.patchRule);
  const patchInstance = useApp((s) => s.patchInstance);
  const removeRule = useApp((s) => s.removeRule);
  const moveRule = useApp((s) => s.moveRule);

  const patch = (p: Record<string, unknown>) => patchRule(inst.id, p);

  return (
    <div className={`rule-card${inst.enabled ? "" : " is-off"}`}>
      <div className="rule-head">
        <input
          type="checkbox"
          checked={inst.enabled}
          title="启用/停用这条规则"
          onChange={(e) =>
            patchInstance(inst.id, { enabled: e.currentTarget.checked })
          }
        />
        <span className="rule-index">{index + 1}</span>
        <span className="rule-kind">
          <KindIcon kind={inst.rule.kind} />
        </span>
        <span className="rule-title">{KIND_LABEL[inst.rule.kind]}</span>
        <div className="spacer" />
        <button
          className="btn icon"
          title="上移"
          disabled={index === 0}
          onClick={() => moveRule(inst.id, -1)}
        >
          <ChevronUp />
        </button>
        <button
          className="btn icon"
          title="下移"
          disabled={index === total - 1}
          onClick={() => moveRule(inst.id, 1)}
        >
          <ChevronDown />
        </button>
        <button
          className="btn icon danger"
          title="删除"
          onClick={() => removeRule(inst.id)}
        >
          <X />
        </button>
      </div>

      <div className="rule-body">
        {inst.rule.kind !== "template" && (
          <div className="field">
            <span className="label">作用于</span>
            <select
              className="input small"
              value={inst.target}
              onChange={(e) =>
                patchInstance(inst.id, {
                  target: e.currentTarget.value as Target,
                })
              }
            >
              <option value="name">文件名</option>
              <option value="extension">扩展名</option>
            </select>
          </div>
        )}

        {inst.rule.kind === "replace" && (
          <>
            <div className="field">
              <span className="label">查找</span>
              <input
                className="input small mono"
                value={inst.rule.find}
                placeholder={inst.rule.regex ? "正则表达式" : "要查找的文本"}
                onChange={(e) => patch({ find: e.currentTarget.value })}
              />
            </div>
            <div className="field">
              <span className="label">替换为</span>
              <input
                className="input small mono"
                value={inst.rule.replaceWith}
                placeholder="留空表示删除"
                onChange={(e) => patch({ replaceWith: e.currentTarget.value })}
              />
            </div>
            <div className="field tight" style={{ flexWrap: "wrap" }}>
              <label className="check">
                <input
                  type="checkbox"
                  checked={inst.rule.regex}
                  onChange={(e) => patch({ regex: e.currentTarget.checked })}
                />
                正则
              </label>
              <label className="check">
                <input
                  type="checkbox"
                  checked={inst.rule.caseSensitive}
                  onChange={(e) =>
                    patch({ caseSensitive: e.currentTarget.checked })
                  }
                />
                区分大小写
              </label>
              <label className="check">
                <input
                  type="checkbox"
                  checked={inst.rule.all}
                  onChange={(e) => patch({ all: e.currentTarget.checked })}
                />
                全部匹配
              </label>
            </div>
            {inst.rule.regex && (
              <div className="rule-hint">
                捕获组写成 <code>{"${1}"}</code>、<code>{"${2}"}</code>。
                注意 <code>$1_</code> 这种写法会被当成组名，必须加花括号。
                例如 <code>IMG_(\d+)_(\d+)</code> → <code>{"${2}-${1}"}</code>
              </div>
            )}
          </>
        )}

        {inst.rule.kind === "sequence" && (
          <>
            <div className="field tight" style={{ flexWrap: "wrap" }}>
              <span className="label">起始</span>
              <input
                className="input small num"
                type="number"
                min={0}
                value={inst.rule.start}
                onChange={(e) =>
                  patch({ start: Number(e.currentTarget.value) || 0 })
                }
              />
              <span className="label">步长</span>
              <input
                className="input small num"
                type="number"
                min={1}
                value={inst.rule.step}
                onChange={(e) =>
                  patch({ step: Number(e.currentTarget.value) || 1 })
                }
              />
              <span className="label">补零</span>
              <input
                className="input small num"
                type="number"
                min={0}
                max={10}
                value={inst.rule.pad}
                onChange={(e) =>
                  patch({ pad: Number(e.currentTarget.value) || 0 })
                }
              />
            </div>
            <div className="field">
              <span className="label">位置</span>
              <select
                className="input small"
                value={inst.rule.position}
                onChange={(e) =>
                  patch({ position: e.currentTarget.value as SeqPosition })
                }
              >
                <option value="prefix">前缀</option>
                <option value="suffix">后缀</option>
                <option value="replace">替换整个名字</option>
              </select>
              <input
                className="input small mono"
                style={{ width: 64 }}
                value={inst.rule.separator}
                placeholder="分隔符"
                onChange={(e) => patch({ separator: e.currentTarget.value })}
              />
            </div>
            <div className="rule-hint">
              序号按当前排序结果分配，改排序方式会改变编号顺序。
            </div>
          </>
        )}

        {inst.rule.kind === "template" && (
          <>
            <div className="field">
              <span className="label">模板</span>
              <input
                className="input small mono"
                value={inst.rule.pattern}
                onChange={(e) => patch({ pattern: e.currentTarget.value })}
              />
            </div>
            <div className="rule-hint">
              常用：{"${name}"} {"${ext}"} {"${orig}"} {"${n:3}"} {"${date}"}{" "}
              {"${parent}"} {"${size}"} {"${rand:6}"}
              <br />
              元数据：{"${shot}"} {"${shot:yyyy-MM-dd}"} {"${camera}"} {"${title}"}{" "}
              {"${artist}"} {"${album}"} {"${track}"} {"${duration:mmss}"}
              <br />
              模板作用于完整文件名，不会自动补扩展名，记得写 {"${ext}"}
            </div>
            <div className="chips">
              {QUICK_TOKENS.map(([label, token]) => (
                <button
                  key={token}
                  className="chip"
                  title={`追加 ${token}`}
                  onClick={() => {
                    // 闭包里类型窄化会丢失，这里重新判断一次
                    if (inst.rule.kind === "template") {
                      patch({ pattern: inst.rule.pattern + token });
                    }
                  }}
                >
                  + {label}
                </button>
              ))}
            </div>
          </>
        )}

        {inst.rule.kind === "case" && (
          <div className="field">
            <span className="label">转为</span>
            <select
              className="input small"
              value={inst.rule.mode}
              onChange={(e) =>
                patch({ mode: e.currentTarget.value as CaseKind })
              }
            >
              <option value="lower">全小写</option>
              <option value="upper">全大写</option>
              <option value="title">首字母大写</option>
              <option value="sentence">句首大写</option>
            </select>
          </div>
        )}

        {inst.rule.kind === "affix" && (
          <>
            <div className="field">
              <span className="label">前缀</span>
              <input
                className="input small mono"
                value={inst.rule.prefix}
                placeholder="加到名字前面"
                onChange={(e) => patch({ prefix: e.currentTarget.value })}
              />
            </div>
            <div className="field">
              <span className="label">后缀</span>
              <input
                className="input small mono"
                value={inst.rule.suffix}
                placeholder="加到名字后面"
                onChange={(e) => patch({ suffix: e.currentTarget.value })}
              />
            </div>
          </>
        )}

        {inst.rule.kind === "cleanup" && (
          <CleanupFields rule={inst.rule} patch={patch} />
        )}
      </div>
    </div>
  );
}

function CleanupFields({
  rule,
  patch,
}: {
  rule: CleanupRule;
  patch: (p: Record<string, unknown>) => void;
}) {
  return (
    <>
      <div className="field">
        <span className="label">空格</span>
        <select
          className="input small"
          value={rule.spaces}
          onChange={(e) =>
            patch({ spaces: e.currentTarget.value as SpaceMode })
          }
        >
          <option value="keep">保留</option>
          <option value="underscore">换成下划线</option>
          <option value="remove">直接删除</option>
          <option value="collapse">合并为一个</option>
        </select>
      </div>
      <div className="field tight" style={{ flexWrap: "wrap" }}>
        <label className="check">
          <input
            type="checkbox"
            checked={rule.stripIllegal}
            onChange={(e) => patch({ stripIllegal: e.currentTarget.checked })}
          />
          去掉非法字符
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={rule.trim}
            onChange={(e) => patch({ trim: e.currentTarget.checked })}
          />
          去首尾空格和点
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={rule.collapseRepeat}
            onChange={(e) =>
              patch({ collapseRepeat: e.currentTarget.checked })
            }
          />
          合并连续重复字符
        </label>
      </div>
    </>
  );
}
