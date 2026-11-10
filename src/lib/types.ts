// 与 src-tauri/src/model.rs 逐字段对应。
// 改动任一侧时必须同步另一侧（Rust 用 rename_all = "camelCase"）。

/** 与 model.rs 的 MediaMeta 对应；读不到就全是 null，error 里是原因 */
export interface MediaMeta {
  shot: number | null;
  make: string | null;
  model: string | null;
  lens: string | null;
  title: string | null;
  artist: string | null;
  album: string | null;
  track: number | null;
  duration: number | null;
  width: number | null;
  height: number | null;
  error: string | null;
}

/** 文件类型分类，仅用于展示，不参与命名规则 */
export type FileKind =
  | "folder"
  | "image"
  | "video"
  | "audio"
  | "document"
  | "sheet"
  | "slide"
  | "archive"
  | "code"
  | "program"
  | "font"
  | "other";

export interface FileEntry {
  id: number;
  dir: string;
  name: string;
  stem: string;
  ext: string;
  kind: FileKind;
  size: number;
  modified: number;
  created: number;
  isDir: boolean;
  readonly: boolean;
  /** 按需读取，未读取或无需读取时为 null */
  meta: MediaMeta | null;
}

export type SortBy = "name" | "modified" | "created" | "size" | "path";

export interface ScanRequest {
  dir: string;
  /** 「选文件」模式下直接点名的路径；非空时后端不再遍历 dir */
  files?: string[] | null;
  recursive: boolean;
  includeFiles: boolean;
  includeDirs: boolean;
  includeHidden: boolean;
  includePattern: string;
  excludePattern: string;
  sortBy: SortBy;
  sortDesc: boolean;
}

export interface ScanResponse {
  sessionId: string;
  dir: string;
  entries: FileEntry[];
  skipped: number;
  truncated: boolean;
}

export type Target = "name" | "extension";
export type SeqPosition = "prefix" | "suffix" | "replace";
export type CaseKind = "lower" | "upper" | "title" | "sentence";
export type SpaceMode = "keep" | "underscore" | "remove" | "collapse";

export interface ReplaceRule {
  kind: "replace";
  find: string;
  replaceWith: string;
  regex: boolean;
  caseSensitive: boolean;
  all: boolean;
}

export interface SequenceRule {
  kind: "sequence";
  start: number;
  step: number;
  pad: number;
  position: SeqPosition;
  separator: string;
}

export interface TemplateRule {
  kind: "template";
  pattern: string;
}

export interface CaseRule {
  kind: "case";
  mode: CaseKind;
}

export interface AffixRule {
  kind: "affix";
  prefix: string;
  suffix: string;
}

export interface CleanupRule {
  kind: "cleanup";
  spaces: SpaceMode;
  stripIllegal: boolean;
  trim: boolean;
  collapseRepeat: boolean;
}

/** 区间截取：保留一段，还是删掉一段 */
export type SliceMode = "keep" | "cut";
/** 超长时的截断方式 */
export type LimitMode = "truncate" | "middle";
/** 插入时间戳的取值来源 */
export type TimeSource = "modified" | "created" | "now";

export interface SliceRule {
  kind: "slice";
  start: number;
  end: number;
  mode: SliceMode;
}

export interface InsertRule {
  kind: "insert";
  at: number;
  text: string;
}

export interface RemoveBracketsRule {
  kind: "removeBrackets";
  round: boolean;
  square: boolean;
  curly: boolean;
  cjk: boolean;
  keepBrackets: boolean;
}

export interface StripCopyMarksRule {
  kind: "stripCopyMarks";
  trailingNumber: boolean;
}

export interface LimitLengthRule {
  kind: "limitLength";
  max: number;
  mode: LimitMode;
}

export interface SetExtensionRule {
  kind: "setExtension";
  ext: string;
}

export interface NormalizeSeparatorsRule {
  kind: "normalizeSeparators";
  to: string;
  space: boolean;
  underscore: boolean;
  hyphen: boolean;
  dot: boolean;
}

export interface StripAffixRule {
  kind: "stripAffix";
  prefix: string;
  suffix: string;
}

export interface InsertTimeRule {
  kind: "insertTime";
  source: TimeSource;
  format: string;
  position: SeqPosition;
  separator: string;
}

export type Rule =
  | ReplaceRule
  | SequenceRule
  | TemplateRule
  | CaseRule
  | AffixRule
  | CleanupRule
  | SliceRule
  | InsertRule
  | RemoveBracketsRule
  | StripCopyMarksRule
  | LimitLengthRule
  | SetExtensionRule
  | NormalizeSeparatorsRule
  | StripAffixRule
  | InsertTimeRule
  | AiRule
  | AiMapRule;

export type RuleKind = Rule["kind"];

export interface RuleInstance {
  id: string;
  enabled: boolean;
  target: Target;
  rule: Rule;
}

export type ConflictPolicy = "autoSuffix" | "skip";
export type IssueLevel = "error" | "warning" | "info";
export type IssueKind =
  | "emptyName"
  | "illegalChar"
  | "reservedName"
  | "tooLong"
  | "duplicate"
  | "existsOnDisk"
  | "caseOnly"
  | "metaMissing";

export interface Issue {
  level: IssueLevel;
  kind: IssueKind;
  message: string;
}

export interface PlanItem {
  id: number;
  srcName: string;
  dstName: string;
  changed: boolean;
  diffStart: number;
  diffEnd: number;
  issues: Issue[];
}

export interface PreviewResult {
  items: PlanItem[];
  total: number;
  changed: number;
  errors: number;
  warnings: number;
  canExecute: boolean;
}

export interface PreviewRequest {
  sessionId: string;
  ids: number[] | null;
  rules: RuleInstance[];
  conflictPolicy: ConflictPolicy;
}

export interface ExecuteRequest {
  sessionId: string;
  ids: number[] | null;
  rules: RuleInstance[];
  conflictPolicy: ConflictPolicy;
}

export type OperationState =
  | "running"
  | "completed"
  | "partial"
  | "rolledBack"
  | "interrupted";

export interface FailedRename {
  srcName: string;
  dstName: string;
  reason: string;
}

export interface ExecuteResult {
  operationId: string;
  total: number;
  renamed: number;
  skipped: number;
  failed: FailedRename[];
  state: OperationState;
  durationMs: number;
}

export interface OperationSummary {
  id: string;
  dir: string;
  startedAt: number;
  total: number;
  renamed: number;
  failed: number;
  state: OperationState;
  canUndo: boolean;
}

export interface UndoFailure {
  name: string;
  reason: string;
}

export interface UndoResult {
  operationId: string;
  restored: number;
  skipped: number;
  failed: UndoFailure[];
}

export interface Settings {
  recursive: boolean;
  includeHidden: boolean;
  includeFiles: boolean;
  includeDirs: boolean;
  sortBy: SortBy;
  sortDesc: boolean;
  conflictPolicy: ConflictPolicy;
  confirmBeforeExecute: boolean;
  recentDirs: string[];
}

export interface Preset {
  id: string;
  name: string;
  rules: RuleInstance[];
  created: number;
}

export interface PresetFile {
  presets: Preset[];
}

// ---------------------------------------------------------------- AI 母模板

export type AiProviderKind = "ollama" | "openai";

export interface AiProviderView {
  id: string;
  name: string;
  kind: AiProviderKind;
  baseUrl: string;
  model: string;
  hasKey: boolean;
}

export interface AiProvidersView {
  providers: AiProviderView[];
  activeId: string;
}

export interface AiProviderConfig {
  id: string;
  name: string;
  kind: AiProviderKind;
  baseUrl: string;
  model: string;
}

export type AiField = "stem" | "parent" | "size" | "index" | "total";
export type AiDateSource = "modified" | "created" | "now" | "shot";
export type AiMetaKey =
  | "camera"
  | "make"
  | "model"
  | "lens"
  | "title"
  | "artist"
  | "album"
  | "track"
  | "duration"
  | "width"
  | "height"
  | "shot";
export type AiExtMode = "keep" | "lower" | "upper" | "set";
export type AiEmpty = "keepOriginal" | "empty";

export type AiCondition =
  | { type: "extIn"; values: string[] }
  | { type: "extNotIn"; values: string[] }
  | { type: "metaPresent"; key: AiMetaKey }
  | { type: "nameMatches"; pattern: string }
  | { type: "isDir"; value: boolean }
  | { type: "sizeRange"; min?: number | null; max?: number | null };

export type AiSegment =
  | { type: "literal"; text: string }
  | { type: "field"; field: AiField; format?: string; fallback?: string }
  | { type: "sequence"; start: number; step: number; pad: number }
  | {
      type: "date";
      source: AiDateSource;
      format: string;
      fallbackSource?: AiDateSource | null;
      fallback?: string;
    }
  | { type: "meta"; key: AiMetaKey; format?: string; fallback?: string }
  | { type: "extract"; pattern: string; group: number; fallback?: string }
  | {
      type: "condition";
      when: AiCondition;
      then: AiSegment[];
      otherwise: AiSegment[];
    }
  | { type: "random"; len: number };

export type AiTransform =
  | {
      type: "replace";
      pattern: string;
      with: string;
      regex: boolean;
      all: boolean;
      caseSensitive: boolean;
    }
  | { type: "stripPrefix"; value: string }
  | { type: "stripSuffix"; value: string }
  | {
      type: "removeBrackets";
      round: boolean;
      square: boolean;
      curly: boolean;
      cjk: boolean;
      keepBrackets: boolean;
    }
  | { type: "stripCopyMarks"; trailingNumber: boolean }
  | { type: "slice"; start: number; end: number; keep: boolean }
  | { type: "case"; mode: CaseKind }
  | {
      type: "cleanup";
      spaces: SpaceMode;
      stripIllegal: boolean;
      trim: boolean;
      collapseRepeat: boolean;
    }
  | {
      type: "normalizeSeparators";
      to: string;
      space: boolean;
      underscore: boolean;
      hyphen: boolean;
      dot: boolean;
    }
  | { type: "limit"; max: number; middle: boolean };

export interface AiSpec {
  version: number;
  segments: AiSegment[];
  transforms: AiTransform[];
  extension: { mode: AiExtMode; value: string };
  fallback: { onEmpty: AiEmpty };
}

export interface AiRule {
  kind: "ai";
  spec: AiSpec;
}

export interface AiMapEntry {
  id: number;
  stem: string;
}

export interface AiMapRule {
  kind: "aiMap";
  entries: AiMapEntry[];
}

export interface AiGenerateRequest {
  requestId: string;
  prompt: string;
  sessionId?: string | null;
  ids?: number[] | null;
  providerId?: string | null;
}

export interface AiSchemeResult {
  summary: string;
  spec: AiSpec;
  assumptions: string[];
  unsupported: string[];
}

export interface AiMapRequest {
  requestId: string;
  prompt: string;
  sessionId?: string | null;
  ids?: number[] | null;
  providerId?: string | null;
  allowContent: boolean;
  maxFiles?: number;
}

export interface AiMapResultEntry {
  id: number;
  srcName: string;
  stem: string;
}

export interface AiMapSkipped {
  name: string;
  reason: string;
}

export interface AiMapResult {
  summary: string;
  entries: AiMapResultEntry[];
  assumptions: string[];
  unsupported: string[];
  skipped: AiMapSkipped[];
}
