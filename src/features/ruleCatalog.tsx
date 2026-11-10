import {
  Ban,
  Bot,
  Braces,
  Brackets,
  CalendarClock,
  CaseSensitive,
  Crop,
  Eraser,
  FileType,
  ListOrdered,
  Replace,
  Ruler,
  Scissors,
  SlidersHorizontal,
  Sparkles,
  TextCursorInput,
} from "lucide-react";

import type { RuleKind } from "../lib/types";

/** 中文名、一句话说明、图标都集中在这里，面板和规则库共用一份 */
export const KIND_LABEL: Record<RuleKind, string> = {
  replace: "查找替换",
  sequence: "序号",
  template: "模板",
  case: "大小写",
  affix: "前后缀",
  cleanup: "清理",
  slice: "按位置截取",
  insert: "指定位置插入",
  removeBrackets: "删除括号内容",
  stripCopyMarks: "清除副本标记",
  limitLength: "限制长度",
  setExtension: "设置扩展名",
  normalizeSeparators: "统一分隔符",
  stripAffix: "剥掉固定前后缀",
  insertTime: "插入时间",
  ai: "AI 命名方案",
};

export const KIND_DESC: Record<RuleKind, string> = {
  replace: "按文本或正则查找并替换",
  sequence: "按当前顺序编号，可补零",
  template: "用变量拼出完整文件名",
  case: "统一成全大写 / 全小写等",
  affix: "在名字头尾加固定文字",
  cleanup: "修掉多余空格、非法字符",
  slice: "按字符位置保留或删掉一段",
  insert: "在指定位置插入文字",
  removeBrackets: "删掉括号连同里面的内容",
  stripCopyMarks: "去掉「 - 副本」「(2)」这类痕迹",
  limitLength: "限制名字长度，避免路径超限",
  setExtension: "统一改成指定的扩展名",
  normalizeSeparators: "空格 / 下划线 / 连字符统一成一种",
  stripAffix: "精确剥掉开头或结尾的固定文字",
  insertTime: "按修改 / 创建 / 当前时间加时间戳",
  ai: "由自然语言生成的母模板方案",
};

/** 每类规则配一个图标，扫一眼就能分辨 */
export const KIND_ICON: Record<RuleKind, typeof Replace> = {
  replace: Replace,
  sequence: ListOrdered,
  template: Braces,
  case: CaseSensitive,
  affix: Brackets,
  cleanup: Sparkles,
  slice: Scissors,
  insert: TextCursorInput,
  removeBrackets: Eraser,
  stripCopyMarks: Ban,
  limitLength: Ruler,
  setExtension: FileType,
  normalizeSeparators: SlidersHorizontal,
  stripAffix: Crop,
  insertTime: CalendarClock,
  ai: Bot,
};

/** 底部快捷区只放这 6 个最常用的 */
export const ADD_ORDER: RuleKind[] = [
  "affix",
  "sequence",
  "insertTime",
  "replace",
  "case",
  "cleanup",
];

/** 规则库的分组，覆盖全部 15 条且每条只出现一次 */
export const GROUPS: { title: string; items: RuleKind[] }[] = [
  { title: "常用", items: ADD_ORDER },
  { title: "定位与截取", items: ["slice", "insert", "removeBrackets", "stripCopyMarks"] },
  { title: "清理与规范", items: ["normalizeSeparators", "stripAffix", "limitLength"] },
  // 模板参数多、要先查变量表，不适合当起手式，放到库里
  { title: "文本与格式", items: ["template", "setExtension"] },
];
