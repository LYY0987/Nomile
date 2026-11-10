import { ChevronDown, ChevronUp, Plus, Wand2, X } from "lucide-react";
import { useState } from "react";
import type { ReactNode } from "react";

import type {
  AiCondition,
  AiDateSource,
  AiField,
  AiMetaKey,
  AiSegment,
  AiSpec,
  AiTransform,
  CaseKind,
  SpaceMode,
} from "../lib/types";
import {
  META_LABEL,
  SEGMENT_LABEL,
  TRANSFORM_LABEL,
  conditionLabel,
  segmentLabel,
  transformLabel,
} from "./aiSpecLabels";

const SEGMENT_TYPES: AiSegment["type"][] = [
  "literal",
  "field",
  "sequence",
  "date",
  "meta",
  "extract",
  "condition",
  "random",
];

const CONDITION_TYPES: AiCondition["type"][] = [
  "extIn",
  "extNotIn",
  "metaPresent",
  "nameMatches",
  "isDir",
  "sizeRange",
];

const CONDITION_LABEL: Record<AiCondition["type"], string> = {
  extIn: "扩展名属于",
  extNotIn: "扩展名不属于",
  metaPresent: "有元数据",
  nameMatches: "名称匹配",
  isDir: "是否文件夹",
  sizeRange: "大小范围",
};

const TRANSFORM_TYPES: AiTransform["type"][] = [
  "replace",
  "stripPrefix",
  "stripSuffix",
  "removeBrackets",
  "stripCopyMarks",
  "slice",
  "case",
  "cleanup",
  "normalizeSeparators",
  "limit",
];

const FIELD_OPTIONS: AiField[] = ["stem", "parent", "size", "index", "total"];
const DATE_OPTIONS: AiDateSource[] = ["modified", "created", "now", "shot"];
const META_OPTIONS: AiMetaKey[] = [
  "camera",
  "make",
  "model",
  "lens",
  "title",
  "artist",
  "album",
  "track",
  "duration",
  "width",
  "height",
  "shot",
];

function cloneSpec(spec: AiSpec): AiSpec {
  return JSON.parse(JSON.stringify(spec)) as AiSpec;
}

function makeSegment(type: AiSegment["type"]): AiSegment {
  switch (type) {
    case "literal":
      return { type: "literal", text: "" };
    case "field":
      return { type: "field", field: "stem", format: "", fallback: "" };
    case "sequence":
      return { type: "sequence", start: 1, step: 1, pad: 3 };
    case "date":
      return {
        type: "date",
        source: "modified",
        format: "yyyyMMdd",
        fallbackSource: null,
        fallback: "",
      };
    case "meta":
      return { type: "meta", key: "camera", format: "", fallback: "" };
    case "extract":
      return { type: "extract", pattern: "", group: 1, fallback: "" };
    case "random":
      return { type: "random", len: 6 };
    case "condition":
      return {
        type: "condition",
        when: makeCondition("extIn"),
        then: [],
        otherwise: [],
      };
  }
}

function makeCondition(type: AiCondition["type"]): AiCondition {
  switch (type) {
    case "extIn":
      return { type: "extIn", values: [] };
    case "extNotIn":
      return { type: "extNotIn", values: [] };
    case "metaPresent":
      return { type: "metaPresent", key: "camera" };
    case "nameMatches":
      return { type: "nameMatches", pattern: "" };
    case "isDir":
      return { type: "isDir", value: true };
    case "sizeRange":
      return { type: "sizeRange", min: null, max: null };
  }
}

function parseExtList(value: string): string[] {
  return value
    .split(/[,，、\s]+/)
    .map((item) => item.trim().replace(/^\./, ""))
    .filter(Boolean);
}

function makeTransform(type: AiTransform["type"]): AiTransform {
  switch (type) {
    case "replace":
      return {
        type: "replace",
        pattern: "",
        with: "",
        regex: false,
        all: true,
        caseSensitive: true,
      };
    case "stripPrefix":
      return { type: "stripPrefix", value: "" };
    case "stripSuffix":
      return { type: "stripSuffix", value: "" };
    case "removeBrackets":
      return {
        type: "removeBrackets",
        round: true,
        square: true,
        curly: true,
        cjk: true,
        keepBrackets: false,
      };
    case "stripCopyMarks":
      return { type: "stripCopyMarks", trailingNumber: true };
    case "slice":
      return { type: "slice", start: 1, end: 0, keep: true };
    case "case":
      return { type: "case", mode: "lower" };
    case "cleanup":
      return {
        type: "cleanup",
        spaces: "collapse",
        stripIllegal: true,
        trim: true,
        collapseRepeat: false,
      };
    case "normalizeSeparators":
      return {
        type: "normalizeSeparators",
        to: "_",
        space: true,
        underscore: true,
        hyphen: true,
        dot: true,
      };
    case "limit":
      return { type: "limit", max: 60, middle: false };
  }
}

function moveItem<T>(list: T[], index: number, delta: number): T[] {
  const target = index + delta;
  if (target < 0 || target >= list.length) return list;
  const next = [...list];
  const [item] = next.splice(index, 1);
  next.splice(target, 0, item);
  return next;
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="field">
      <span className="label">{label}</span>
      {children}
    </div>
  );
}

export function AiSpecEditor({
  spec,
  onSave,
  onClose,
}: {
  spec: AiSpec;
  onSave: (spec: AiSpec) => void;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState<AiSpec>(() => cloneSpec(spec));
  const [newTransform, setNewTransform] =
    useState<AiTransform["type"]>("replace");

  const patch = (patch: Partial<AiSpec>) =>
    setDraft((current) => ({ ...current, ...patch }));

  const canSave = draft.segments.length > 0 || draft.transforms.length > 0;

  return (
    <div className="modal-mask" onClick={onClose}>
      <div
        className="modal spec-editor-modal"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="modal-head">
          <Wand2 />
          <span className="panel-title">AI 方案参数</span>
          <div className="spacer" />
          <button className="btn icon" title="关闭" onClick={onClose}>
            <X />
          </button>
        </div>

        <div className="spec-editor-body">
          <div className="spec-toolbar">
            <Row label="扩展名">
              <select
                className="input small"
                value={draft.extension.mode}
                onChange={(event) =>
                  patch({
                    extension: {
                      ...draft.extension,
                      mode: event.currentTarget.value as AiSpec["extension"]["mode"],
                    },
                  })
                }
              >
                <option value="keep">保留</option>
                <option value="lower">转小写</option>
                <option value="upper">转大写</option>
                <option value="set">指定</option>
              </select>
              {draft.extension.mode === "set" && (
                <input
                  className="input small mono"
                  value={draft.extension.value}
                  placeholder="如 jpg，留空去掉扩展名"
                  onChange={(event) =>
                    patch({
                      extension: {
                        ...draft.extension,
                        value: event.currentTarget.value,
                      },
                    })
                  }
                />
              )}
            </Row>

            <Row label="空结果">
              <select
                className="input small"
                value={draft.fallback.onEmpty}
                onChange={(event) =>
                  patch({
                    fallback: {
                      onEmpty: event.currentTarget.value as AiSpec["fallback"]["onEmpty"],
                    },
                  })
                }
              >
                <option value="keepOriginal">保留原名</option>
                <option value="empty">留空</option>
              </select>
            </Row>
          </div>

          <div className="spec-section">
            <div className="spec-section-title">命名结构</div>
            <SegmentListEditor
              segments={draft.segments}
              onChange={(segments) => patch({ segments })}
              depth={0}
              allowCondition
            />
          </div>

          <div className="spec-section">
            <div className="spec-section-title">预处理</div>
            {draft.transforms.length === 0 && (
              <div className="spec-empty">没有预处理，直接按原文件名组装。</div>
            )}
            {draft.transforms.map((transform, index) => (
              <TransformCard
                key={index}
                transform={transform}
                index={index}
                total={draft.transforms.length}
                onChange={(next) =>
                  patch({
                    transforms: draft.transforms.map((item, i) =>
                      i === index ? next : item
                    ),
                  })
                }
                onMove={(delta) =>
                  patch({ transforms: moveItem(draft.transforms, index, delta) })
                }
                onRemove={() =>
                  patch({
                    transforms: draft.transforms.filter((_, i) => i !== index),
                  })
                }
              />
            ))}
            <div className="spec-actions">
              <select
                className="input small"
                value={newTransform}
                onChange={(event) =>
                  setNewTransform(
                    event.currentTarget.value as AiTransform["type"]
                  )
                }
              >
                {TRANSFORM_TYPES.map((type) => (
                  <option key={type} value={type}>
                    {TRANSFORM_LABEL[type]}
                  </option>
                ))}
              </select>
              <button
                className="btn small"
                onClick={() =>
                  patch({
                    transforms: [...draft.transforms, makeTransform(newTransform)],
                  })
                }
              >
                <Plus />
                添加预处理
              </button>
            </div>
          </div>

          <div className="ai-settings-hint">
            条件分支由 AI 生成，这里只读展示；需要改条件时请回到面板重新生成。
            保存后预览会立即按新参数更新。
          </div>
        </div>

        <div className="modal-foot spec-foot">
          <span className="ai-settings-hint">
            {canSave ? "参数会立即应用到预览" : "至少保留一个命名段或一条预处理"}
          </span>
          <div className="spacer" />
          <button className="btn" onClick={onClose}>
            取消
          </button>
          <button
            className="btn primary"
            disabled={!canSave}
            onClick={() => onSave(draft)}
          >
            保存并应用
          </button>
        </div>
      </div>
    </div>
  );
}

function SegmentListEditor({
  segments,
  onChange,
  depth,
  allowCondition,
}: {
  segments: AiSegment[];
  onChange: (segments: AiSegment[]) => void;
  depth: number;
  allowCondition: boolean;
}) {
  const [newType, setNewType] = useState<AiSegment["type"]>("literal");
  const types = allowCondition
    ? SEGMENT_TYPES
    : SEGMENT_TYPES.filter((type) => type !== "condition");

  return (
    <>
      {segments.length === 0 && (
        <div className="spec-empty">还没有命名段。</div>
      )}
      {segments.map((segment, index) => (
        <SegmentCard
          key={index}
          segment={segment}
          index={index}
          total={segments.length}
          depth={depth}
          onChange={(next) =>
            onChange(segments.map((item, i) => (i === index ? next : item)))
          }
          onMove={(delta) => onChange(moveItem(segments, index, delta))}
          onRemove={() => onChange(segments.filter((_, i) => i !== index))}
        />
      ))}
      <div className="spec-actions">
        <select
          className="input small"
          value={newType}
          onChange={(event) =>
            setNewType(event.currentTarget.value as AiSegment["type"])
          }
        >
          {types.map((type) => (
            <option key={type} value={type}>
              {SEGMENT_LABEL[type]}
            </option>
          ))}
        </select>
        <button
          className="btn small"
          onClick={() => onChange([...segments, makeSegment(newType)])}
        >
          <Plus />
          添加段
        </button>
      </div>
    </>
  );
}

function ConditionEditor({
  condition,
  then,
  otherwise,
  depth,
  onChange,
}: {
  condition: AiCondition;
  then: AiSegment[];
  otherwise: AiSegment[];
  depth: number;
  onChange: (
    when: AiCondition,
    then: AiSegment[],
    otherwise: AiSegment[]
  ) => void;
}) {
  const patch = (next: AiCondition) => onChange(next, then, otherwise);

  return (
    <div className="spec-cond-edit">
      <Row label="条件">
        <select
          className="input small"
          value={condition.type}
          onChange={(event) =>
            onChange(
              makeCondition(event.currentTarget.value as AiCondition["type"]),
              then,
              otherwise
            )
          }
        >
          {CONDITION_TYPES.map((type) => (
            <option key={type} value={type}>
              {CONDITION_LABEL[type]}
            </option>
          ))}
        </select>
      </Row>

      {(condition.type === "extIn" || condition.type === "extNotIn") && (
        <Row label="扩展名">
          <input
            className="input small mono"
            value={condition.values.join(", ")}
            placeholder="jpg, png，留空表示不限制"
            onChange={(event) =>
              patch({ ...condition, values: parseExtList(event.currentTarget.value) })
            }
          />
        </Row>
      )}

      {condition.type === "metaPresent" && (
        <Row label="元数据">
          <select
            className="input small"
            value={condition.key}
            onChange={(event) =>
              patch({
                ...condition,
                key: event.currentTarget.value as AiMetaKey,
              })
            }
          >
            {META_OPTIONS.map((key) => (
              <option key={key} value={key}>
                {META_LABEL[key]}
              </option>
            ))}
          </select>
        </Row>
      )}

      {condition.type === "nameMatches" && (
        <Row label="正则">
          <input
            className="input small mono"
            value={condition.pattern}
            placeholder="如 ^IMG_"
            onChange={(event) =>
              patch({ ...condition, pattern: event.currentTarget.value })
            }
          />
        </Row>
      )}

      {condition.type === "isDir" && (
        <label className="check">
          <input
            type="checkbox"
            checked={condition.value}
            onChange={(event) =>
              patch({ ...condition, value: event.currentTarget.checked })
            }
          />
          是文件夹
        </label>
      )}

      {condition.type === "sizeRange" && (
        <Row label="大小">
          <input
            className="input small num"
            type="number"
            value={condition.min ?? ""}
            placeholder="最小"
            onChange={(event) =>
              patch({
                ...condition,
                min:
                  event.currentTarget.value === ""
                    ? null
                    : Number(event.currentTarget.value),
              })
            }
          />
          <span className="label">到</span>
          <input
            className="input small num"
            type="number"
            value={condition.max ?? ""}
            placeholder="最大"
            onChange={(event) =>
              patch({
                ...condition,
                max:
                  event.currentTarget.value === ""
                    ? null
                    : Number(event.currentTarget.value),
              })
            }
          />
        </Row>
      )}

      <div className="spec-cond-branch">
        <span className="spec-cond-key">则</span>
        <div className="spec-cond-list">
          <SegmentListEditor
            segments={then}
            onChange={(next) => onChange(condition, next, otherwise)}
            depth={depth + 1}
            allowCondition={false}
          />
        </div>
      </div>

      <div className="spec-cond-branch">
        <span className="spec-cond-key">否则</span>
        <div className="spec-cond-list">
          <SegmentListEditor
            segments={otherwise}
            onChange={(next) => onChange(condition, then, next)}
            depth={depth + 1}
            allowCondition={false}
          />
        </div>
      </div>
    </div>
  );
}

function SegmentCard({
  segment,
  index,
  total,
  depth,
  onChange,
  onMove,
  onRemove,
}: {
  segment: AiSegment;
  index: number;
  total: number;
  depth: number;
  onChange: (segment: AiSegment) => void;
  onMove: (delta: number) => void;
  onRemove: () => void;
}) {
  const patch = (patch: Record<string, unknown>) =>
    onChange({ ...segment, ...patch } as AiSegment);

  return (
    <div className="spec-card">
      <div className="spec-card-head">
        <span className="spec-card-title">{SEGMENT_LABEL[segment.type]}</span>
        <span className="spec-card-summary">{segmentLabel(segment)}</span>
        <div className="spacer" />
        <button
          className="btn icon"
          title="上移"
          disabled={index === 0}
          onClick={() => onMove(-1)}
        >
          <ChevronUp />
        </button>
        <button
          className="btn icon"
          title="下移"
          disabled={index === total - 1}
          onClick={() => onMove(1)}
        >
          <ChevronDown />
        </button>
        <button className="btn icon danger" title="删除" onClick={onRemove}>
          <X />
        </button>
      </div>

      <div className="spec-card-body">
        {segment.type === "literal" && (
          <Row label="文字">
            <input
              className="input small mono"
              value={segment.text}
              placeholder="要原样输出的文字"
              onChange={(event) => patch({ text: event.currentTarget.value })}
            />
          </Row>
        )}

        {segment.type === "field" && (
          <>
            <Row label="字段">
              <select
                className="input small"
                value={segment.field}
                onChange={(event) =>
                  patch({ field: event.currentTarget.value as AiField })
                }
              >
                {FIELD_OPTIONS.map((field) => (
                  <option key={field} value={field}>
                    {field === "stem"
                      ? "原文件名"
                      : field === "parent"
                        ? "父目录"
                        : field === "size"
                          ? "大小"
                          : field === "index"
                            ? "序号"
                            : "总数"}
                  </option>
                ))}
              </select>
            </Row>
            {segment.field === "index" && (
              <Row label="补零">
                <input
                  className="input small num"
                  type="number"
                  min={0}
                  max={10}
                  value={Number(segment.format || 0)}
                  onChange={(event) =>
                    patch({ format: event.currentTarget.value })
                  }
                />
              </Row>
            )}
            <Row label="缺失时">
              <input
                className="input small mono"
                value={segment.fallback ?? ""}
                placeholder="留空表示不补"
                onChange={(event) =>
                  patch({ fallback: event.currentTarget.value })
                }
              />
            </Row>
          </>
        )}

        {segment.type === "sequence" && (
          <>
            <Row label="起始">
              <input
                className="input small num"
                type="number"
                min={0}
                value={segment.start}
                onChange={(event) =>
                  patch({ start: Number(event.currentTarget.value) || 0 })
                }
              />
              <span className="label">步长</span>
              <input
                className="input small num"
                type="number"
                min={1}
                value={segment.step}
                onChange={(event) =>
                  patch({ step: Number(event.currentTarget.value) || 1 })
                }
              />
              <span className="label">补零</span>
              <input
                className="input small num"
                type="number"
                min={0}
                max={10}
                value={segment.pad}
                onChange={(event) =>
                  patch({ pad: Number(event.currentTarget.value) || 0 })
                }
              />
            </Row>
          </>
        )}

        {segment.type === "date" && (
          <>
            <Row label="来源">
              <select
                className="input small"
                value={segment.source}
                onChange={(event) =>
                  patch({ source: event.currentTarget.value as AiDateSource })
                }
              >
                {DATE_OPTIONS.map((source) => (
                  <option key={source} value={source}>
                    {source === "modified"
                      ? "修改时间"
                      : source === "created"
                        ? "创建时间"
                        : source === "now"
                          ? "当前时间"
                          : "拍摄时间"}
                  </option>
                ))}
              </select>
            </Row>
            <Row label="格式">
              <input
                className="input small mono"
                value={segment.format}
                placeholder="yyyyMMdd"
                onChange={(event) => patch({ format: event.currentTarget.value })}
              />
            </Row>
            <Row label="缺失时">
              <select
                className="input small"
                value={segment.fallbackSource ?? ""}
                onChange={(event) =>
                  patch({ fallbackSource: event.currentTarget.value || null })
                }
              >
                <option value="">不换来源</option>
                {DATE_OPTIONS.map((source) => (
                  <option key={source} value={source}>
                    改用{source === "modified" ? "修改时间" : source === "created" ? "创建时间" : source === "now" ? "当前时间" : "拍摄时间"}
                  </option>
                ))}
              </select>
              <input
                className="input small mono"
                value={segment.fallback ?? ""}
                placeholder="兜底文字"
                onChange={(event) =>
                  patch({ fallback: event.currentTarget.value })
                }
              />
            </Row>
          </>
        )}

        {segment.type === "meta" && (
          <>
            <Row label="字段">
              <select
                className="input small"
                value={segment.key}
                onChange={(event) =>
                  patch({ key: event.currentTarget.value as AiMetaKey })
                }
              >
                {META_OPTIONS.map((key) => (
                  <option key={key} value={key}>
                    {META_LABEL[key]}
                  </option>
                ))}
              </select>
            </Row>
            {(segment.key === "track" ||
              segment.key === "duration" ||
              segment.key === "shot") && (
              <Row label="格式">
                <input
                  className="input small mono"
                  value={segment.format ?? ""}
                  placeholder={
                    segment.key === "track"
                      ? "2"
                      : segment.key === "duration"
                        ? "mmss"
                        : "yyyyMMdd"
                  }
                  onChange={(event) =>
                    patch({ format: event.currentTarget.value })
                  }
                />
              </Row>
            )}
            <Row label="缺失时">
              <input
                className="input small mono"
                value={segment.fallback ?? ""}
                placeholder="兜底文字"
                onChange={(event) =>
                  patch({ fallback: event.currentTarget.value })
                }
              />
            </Row>
          </>
        )}

        {segment.type === "extract" && (
          <>
            <Row label="正则">
              <input
                className="input small mono"
                value={segment.pattern}
                placeholder="如 IMG_(\d+)"
                onChange={(event) =>
                  patch({ pattern: event.currentTarget.value })
                }
              />
            </Row>
            <Row label="捕获组">
              <input
                className="input small num"
                type="number"
                min={0}
                max={9}
                value={segment.group}
                onChange={(event) =>
                  patch({ group: Number(event.currentTarget.value) || 0 })
                }
              />
            </Row>
            <Row label="缺失时">
              <input
                className="input small mono"
                value={segment.fallback ?? ""}
                placeholder="兜底文字"
                onChange={(event) =>
                  patch({ fallback: event.currentTarget.value })
                }
              />
            </Row>
          </>
        )}

        {segment.type === "random" && (
          <Row label="长度">
            <input
              className="input small num"
              type="number"
              min={1}
              max={32}
              value={segment.len}
              onChange={(event) =>
                patch({ len: Number(event.currentTarget.value) || 1 })
              }
            />
          </Row>
        )}

        {segment.type === "condition" &&
          (depth > 0 ? (
            <div className="spec-cond">
              <div className="spec-cond-row">
                <span className="spec-cond-key">当</span>
                {conditionLabel(segment.when)}
              </div>
              <div className="spec-cond-row">
                <span className="spec-cond-key">则</span>
                {segment.then.map(segmentLabel).join(" + ") || "（空）"}
              </div>
              <div className="spec-cond-row">
                <span className="spec-cond-key">否则</span>
                {segment.otherwise.map(segmentLabel).join(" + ") || "（空）"}
              </div>
              <div className="ai-settings-hint">
                嵌套条件只读展示，需要调整请在外面重新生成。
              </div>
            </div>
          ) : (
            <ConditionEditor
              condition={segment.when}
              then={segment.then}
              otherwise={segment.otherwise}
              depth={depth}
              onChange={(when, then, otherwise) =>
                patch({ when, then, otherwise })
              }
            />
          ))}
      </div>
    </div>
  );
}

function TransformCard({
  transform,
  index,
  total,
  onChange,
  onMove,
  onRemove,
}: {
  transform: AiTransform;
  index: number;
  total: number;
  onChange: (transform: AiTransform) => void;
  onMove: (delta: number) => void;
  onRemove: () => void;
}) {
  const patch = (patch: Record<string, unknown>) =>
    onChange({ ...transform, ...patch } as AiTransform);

  return (
    <div className="spec-card">
      <div className="spec-card-head">
        <span className="spec-card-title">{TRANSFORM_LABEL[transform.type]}</span>
        <span className="spec-card-summary">{transformLabel(transform)}</span>
        <div className="spacer" />
        <button
          className="btn icon"
          title="上移"
          disabled={index === 0}
          onClick={() => onMove(-1)}
        >
          <ChevronUp />
        </button>
        <button
          className="btn icon"
          title="下移"
          disabled={index === total - 1}
          onClick={() => onMove(1)}
        >
          <ChevronDown />
        </button>
        <button className="btn icon danger" title="删除" onClick={onRemove}>
          <X />
        </button>
      </div>

      <div className="spec-card-body">
        {transform.type === "replace" && (
          <>
            <Row label="查找">
              <input
                className="input small mono"
                value={transform.pattern}
                placeholder={transform.regex ? "正则表达式" : "要查找的文本"}
                onChange={(event) =>
                  patch({ pattern: event.currentTarget.value })
                }
              />
            </Row>
            <Row label="替换为">
              <input
                className="input small mono"
                value={transform.with}
                onChange={(event) =>
                  patch({ with: event.currentTarget.value })
                }
              />
            </Row>
            <div className="field tight" style={{ flexWrap: "wrap" }}>
              <label className="check">
                <input
                  type="checkbox"
                  checked={transform.regex}
                  onChange={(event) =>
                    patch({ regex: event.currentTarget.checked })
                  }
                />
                正则
              </label>
              <label className="check">
                <input
                  type="checkbox"
                  checked={transform.all}
                  onChange={(event) =>
                    patch({ all: event.currentTarget.checked })
                  }
                />
                全部匹配
              </label>
              <label className="check">
                <input
                  type="checkbox"
                  checked={transform.caseSensitive}
                  onChange={(event) =>
                    patch({ caseSensitive: event.currentTarget.checked })
                  }
                />
                区分大小写
              </label>
            </div>
          </>
        )}

        {(transform.type === "stripPrefix" ||
          transform.type === "stripSuffix") && (
          <Row label="文字">
            <input
              className="input small mono"
              value={transform.value}
              onChange={(event) =>
                patch({ value: event.currentTarget.value })
              }
            />
          </Row>
        )}

        {transform.type === "removeBrackets" && (
          <div className="field tight" style={{ flexWrap: "wrap" }}>
            <label className="check">
              <input
                type="checkbox"
                checked={transform.round}
                onChange={(event) =>
                  patch({ round: event.currentTarget.checked })
                }
              />
              （）
            </label>
            <label className="check">
              <input
                type="checkbox"
                checked={transform.square}
                onChange={(event) =>
                  patch({ square: event.currentTarget.checked })
                }
              />
              【】
            </label>
            <label className="check">
              <input
                type="checkbox"
                checked={transform.curly}
                onChange={(event) =>
                  patch({ curly: event.currentTarget.checked })
                }
              />
              {"{}"}
            </label>
            <label className="check">
              <input
                type="checkbox"
                checked={transform.cjk}
                onChange={(event) =>
                  patch({ cjk: event.currentTarget.checked })
                }
              />
              中文括号
            </label>
            <label className="check">
              <input
                type="checkbox"
                checked={transform.keepBrackets}
                onChange={(event) =>
                  patch({ keepBrackets: event.currentTarget.checked })
                }
              />
              只删内容
            </label>
          </div>
        )}

        {transform.type === "stripCopyMarks" && (
          <label className="check">
            <input
              type="checkbox"
              checked={transform.trailingNumber}
              onChange={(event) =>
                patch({ trailingNumber: event.currentTarget.checked })
              }
            />
            也去掉结尾的 (1)(2) 编号
          </label>
        )}

        {transform.type === "slice" && (
          <Row label="位置">
            <input
              className="input small num"
              type="number"
              value={transform.start}
              onChange={(event) =>
                patch({ start: Number(event.currentTarget.value) })
              }
            />
            <span className="label">到</span>
            <input
              className="input small num"
              type="number"
              value={transform.end}
              onChange={(event) =>
                patch({ end: Number(event.currentTarget.value) })
              }
            />
            <select
              className="input small"
              value={transform.keep ? "keep" : "cut"}
              onChange={(event) =>
                patch({ keep: event.currentTarget.value === "keep" })
              }
            >
              <option value="keep">保留这一段</option>
              <option value="cut">删掉这一段</option>
            </select>
          </Row>
        )}

        {transform.type === "case" && (
          <Row label="转为">
            <select
              className="input small"
              value={transform.mode}
              onChange={(event) =>
                patch({ mode: event.currentTarget.value as CaseKind })
              }
            >
              <option value="lower">全小写</option>
              <option value="upper">全大写</option>
              <option value="title">首字母大写</option>
              <option value="sentence">句首大写</option>
            </select>
          </Row>
        )}

        {transform.type === "cleanup" && (
          <>
            <Row label="空格">
              <select
                className="input small"
                value={transform.spaces}
                onChange={(event) =>
                  patch({ spaces: event.currentTarget.value as SpaceMode })
                }
              >
                <option value="keep">保留</option>
                <option value="underscore">换成下划线</option>
                <option value="remove">直接删除</option>
                <option value="collapse">合并为一个</option>
              </select>
            </Row>
            <div className="field tight" style={{ flexWrap: "wrap" }}>
              <label className="check">
                <input
                  type="checkbox"
                  checked={transform.stripIllegal}
                  onChange={(event) =>
                    patch({ stripIllegal: event.currentTarget.checked })
                  }
                />
                去非法字符
              </label>
              <label className="check">
                <input
                  type="checkbox"
                  checked={transform.trim}
                  onChange={(event) =>
                    patch({ trim: event.currentTarget.checked })
                  }
                />
                去首尾空格
              </label>
              <label className="check">
                <input
                  type="checkbox"
                  checked={transform.collapseRepeat}
                  onChange={(event) =>
                    patch({ collapseRepeat: event.currentTarget.checked })
                  }
                />
                合并重复字符
              </label>
            </div>
          </>
        )}

        {transform.type === "normalizeSeparators" && (
          <>
            <Row label="统一成">
              <input
                className="input small mono"
                style={{ width: 64 }}
                value={transform.to}
                onChange={(event) => patch({ to: event.currentTarget.value })}
              />
            </Row>
            <div className="field tight" style={{ flexWrap: "wrap" }}>
              <label className="check">
                <input
                  type="checkbox"
                  checked={transform.space}
                  onChange={(event) =>
                    patch({ space: event.currentTarget.checked })
                  }
                />
                空格
              </label>
              <label className="check">
                <input
                  type="checkbox"
                  checked={transform.underscore}
                  onChange={(event) =>
                    patch({ underscore: event.currentTarget.checked })
                  }
                />
                下划线
              </label>
              <label className="check">
                <input
                  type="checkbox"
                  checked={transform.hyphen}
                  onChange={(event) =>
                    patch({ hyphen: event.currentTarget.checked })
                  }
                />
                连字符
              </label>
              <label className="check">
                <input
                  type="checkbox"
                  checked={transform.dot}
                  onChange={(event) =>
                    patch({ dot: event.currentTarget.checked })
                  }
                />
                点
              </label>
            </div>
          </>
        )}

        {transform.type === "limit" && (
          <>
            <Row label="最多">
              <input
                className="input small num"
                type="number"
                min={1}
                max={260}
                value={transform.max}
                onChange={(event) =>
                  patch({ max: Number(event.currentTarget.value) || 1 })
                }
              />
            </Row>
            <label className="check">
              <input
                type="checkbox"
                checked={transform.middle}
                onChange={(event) =>
                  patch({ middle: event.currentTarget.checked })
                }
              />
              中间留省略号
            </label>
          </>
        )}
      </div>
    </div>
  );
}
