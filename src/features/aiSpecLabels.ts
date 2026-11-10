import type {
  AiCondition,
  AiDateSource,
  AiField,
  AiMetaKey,
  AiSegment,
  AiTransform,
} from "../lib/types";

export const FIELD_LABEL: Record<AiField, string> = {
  stem: "原文件名",
  parent: "父目录",
  size: "大小",
  index: "序号",
  total: "总数",
};

export const DATE_LABEL: Record<AiDateSource, string> = {
  modified: "修改时间",
  created: "创建时间",
  now: "当前时间",
  shot: "拍摄时间",
};

export const META_LABEL: Record<AiMetaKey, string> = {
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

export const SEGMENT_LABEL: Record<AiSegment["type"], string> = {
  literal: "固定文字",
  field: "字段",
  sequence: "序号",
  date: "时间",
  meta: "元数据",
  extract: "正则提取",
  condition: "条件分支",
  random: "随机串",
};

export const TRANSFORM_LABEL: Record<AiTransform["type"], string> = {
  replace: "查找替换",
  stripPrefix: "删前缀",
  stripSuffix: "删后缀",
  removeBrackets: "删括号内容",
  stripCopyMarks: "去副本标记",
  slice: "截取",
  case: "大小写",
  cleanup: "清理",
  normalizeSeparators: "统一分隔符",
  limit: "限制长度",
};

export function segmentLabel(segment: AiSegment): string {
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

export function transformLabel(transform: AiTransform): string {
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
      return {
        lower: "转小写",
        upper: "转大写",
        title: "首字母大写",
        sentence: "句首大写",
      }[transform.mode];
    case "cleanup":
      return "清理字符";
    case "normalizeSeparators":
      return `统一分隔符「${transform.to}」`;
    case "limit":
      return `限长 ${transform.max}`;
  }
}

export function conditionLabel(condition: AiCondition): string {
  switch (condition.type) {
    case "extIn":
      return `扩展名 ∈ ${condition.values.join("、") || "未填"}`;
    case "extNotIn":
      return `扩展名 ∉ ${condition.values.join("、") || "未填"}`;
    case "metaPresent":
      return `有${META_LABEL[condition.key]}`;
    case "nameMatches":
      return `名称匹配 ${condition.pattern}`;
    case "isDir":
      return condition.value ? "是文件夹" : "不是文件夹";
    case "sizeRange":
      return `大小 ${condition.min ?? 0}~${condition.max ?? "不限"} 字节`;
  }
}
