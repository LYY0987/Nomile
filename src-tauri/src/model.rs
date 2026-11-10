//! 前后端共享的数据契约。
//!
//! 所有结构体统一 `rename_all = "camelCase"`，与 `src/lib/types.ts` 逐字段对应。
//! 改动任一字段时必须同步修改前端类型（见 docs/方案.md「类型同步」一节）。

use serde::{Deserialize, Serialize};

fn d_true() -> bool {
    true
}
fn d_step() -> u64 {
    1
}
fn d_pad() -> u8 {
    0
}
fn d_limit() -> usize {
    60
}
fn d_sep() -> String {
    "_".into()
}
fn d_stamp_fmt() -> String {
    "yyyyMMdd".into()
}

// ---------------------------------------------------------------- 文件条目

/// 从文件内部读出的元数据（EXIF / 音视频标签）。
/// 缺失一律为 null，不臆造默认值；读取失败时 `error` 给出原因。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaMeta {
    /// 拍摄时间（EXIF DateTimeOriginal 等），Unix 毫秒，按本地时区解释
    pub shot: Option<i64>,
    /// 相机 / 设备厂商
    pub make: Option<String>,
    /// 相机 / 设备型号
    pub model: Option<String>,
    pub lens: Option<String>,
    /// 音频 / 视频标题
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub track: Option<u32>,
    /// 时长（秒）
    pub duration: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// 解析失败原因，仅用于界面提示
    pub error: Option<String>,
}

impl MediaMeta {
    /// 有没有任何可用于命名的字段
    pub fn has_value(&self) -> bool {
        self.shot.is_some()
            || self.title.is_some()
            || self.artist.is_some()
            || self.album.is_some()
            || self.track.is_some()
            || self.make.is_some()
            || self.model.is_some()
            || self.lens.is_some()
    }
}

/// 文件类型分类，只用于界面展示，不参与命名规则
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    Folder,
    Image,
    Video,
    Audio,
    Document,
    Sheet,
    Slide,
    Archive,
    Code,
    Program,
    Font,
    Other,
}

impl Default for FileKind {
    fn default() -> Self {
        FileKind::Other
    }
}

const IMAGE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "jpe", "jfif", "gif", "bmp", "webp", "avif", "heic", "heif", "tif",
    "tiff", "svg", "ico", "psd", "ai", "raw", "cr2", "cr3", "nef", "arw", "dng", "orf", "rw2",
];
const VIDEO_EXTS: &[&str] = &[
    "mp4", "m4v", "mkv", "avi", "mov", "wmv", "flv", "f4v", "webm", "mpg", "mpeg", "m2ts", "3gp",
    "rm", "rmvb", "vob", "ogv",
];
const AUDIO_EXTS: &[&str] = &[
    "mp3", "wav", "flac", "aac", "ogg", "oga", "m4a", "wma", "ape", "alac", "opus", "mid", "midi",
    "amr", "aiff", "aif",
];
const DOC_EXTS: &[&str] = &[
    "doc", "docx", "pdf", "txt", "rtf", "odt", "pages", "tex", "md", "markdown", "epub", "mobi",
    "wps",
];
const SHEET_EXTS: &[&str] = &["xls", "xlsx", "xlsm", "xlsb", "csv", "tsv", "ods", "numbers"];
const SLIDE_EXTS: &[&str] = &["ppt", "pptx", "pps", "ppsx", "odp", "key"];
const ARCHIVE_EXTS: &[&str] = &[
    "zip", "rar", "7z", "tar", "gz", "tgz", "bz2", "xz", "zst", "iso", "cab", "lz", "lzma", "arj",
];
const CODE_EXTS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "pyw", "java", "kt", "kts", "go", "c",
    "cc", "cpp", "cxx", "h", "hpp", "hxx", "cs", "rb", "php", "swift", "m", "mm", "lua", "pl",
    "r", "sql", "html", "htm", "css", "scss", "sass", "less", "json", "json5", "xml", "yml",
    "yaml", "toml", "ini", "conf", "cfg", "sh", "bash", "zsh", "fish", "bat", "cmd", "ps1",
    "vue", "svelte", "dart", "scala",
];
const PROGRAM_EXTS: &[&str] = &[
    "exe", "msi", "dll", "sys", "so", "dylib", "apk", "ipa", "dmg", "pkg", "deb", "rpm",
    "appimage", "com", "scr", "bin",
];
const FONT_EXTS: &[&str] = &["ttf", "otf", "woff", "woff2", "eot", "fon", "fnt"];

impl FileKind {
    /// 按扩展名归类；目录一律算「文件夹」。只看后缀，不读文件内容。
    pub fn of(ext: &str, is_dir: bool) -> FileKind {
        if is_dir {
            return FileKind::Folder;
        }
        let e = ext.trim_start_matches('.').to_ascii_lowercase();
        if e.is_empty() {
            return FileKind::Other;
        }

        // 顺序有意义：`.ts` 归代码（TypeScript 更常见），视频容器只认 `m2ts`
        if IMAGE_EXTS.contains(&e.as_str()) {
            FileKind::Image
        } else if VIDEO_EXTS.contains(&e.as_str()) {
            FileKind::Video
        } else if AUDIO_EXTS.contains(&e.as_str()) {
            FileKind::Audio
        } else if DOC_EXTS.contains(&e.as_str()) {
            FileKind::Document
        } else if SHEET_EXTS.contains(&e.as_str()) {
            FileKind::Sheet
        } else if SLIDE_EXTS.contains(&e.as_str()) {
            FileKind::Slide
        } else if ARCHIVE_EXTS.contains(&e.as_str()) {
            FileKind::Archive
        } else if CODE_EXTS.contains(&e.as_str()) {
            FileKind::Code
        } else if PROGRAM_EXTS.contains(&e.as_str()) {
            FileKind::Program
        } else if FONT_EXTS.contains(&e.as_str()) {
            FileKind::Font
        } else {
            FileKind::Other
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub id: u64,
    /// 所在目录（绝对路径）
    pub dir: String,
    /// 完整文件名，含扩展名
    pub name: String,
    /// 不含扩展名
    pub stem: String,
    /// 不含点，可为空
    pub ext: String,
    /// 类型分类（图片 / 文档 / 视频…），只用于展示
    #[serde(default)]
    pub kind: FileKind,
    pub size: u64,
    /// 修改时间，Unix 毫秒
    pub modified: i64,
    /// 创建时间，Unix 毫秒
    pub created: i64,
    pub is_dir: bool,
    pub readonly: bool,
    /// 元数据（EXIF / 音视频标签）。按需读取后填充，未读取时为 null。
    #[serde(default)]
    pub meta: Option<MediaMeta>,
}

// ---------------------------------------------------------------- 扫描

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortBy {
    #[default]
    Name,
    Modified,
    Created,
    Size,
    Path,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRequest {
    pub dir: String,
    /// 用户用「选文件」亲手点名的一批路径。非空时不再遍历 `dir`，
    /// 也不套用「含子目录 / 含隐藏 / 只包含 / 排除」这些范围条件。
    #[serde(default)]
    pub files: Option<Vec<String>>,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default = "d_true")]
    pub include_files: bool,
    #[serde(default)]
    pub include_dirs: bool,
    #[serde(default)]
    pub include_hidden: bool,
    /// 通配符，支持 * 与 ?；空串表示不过滤
    #[serde(default)]
    pub include_pattern: String,
    #[serde(default)]
    pub exclude_pattern: String,
    #[serde(default)]
    pub sort_by: SortBy,
    #[serde(default)]
    pub sort_desc: bool,
    /// 上限保护，默认 200000
    #[serde(default)]
    pub max_entries: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResponse {
    pub session_id: String,
    pub dir: String,
    pub entries: Vec<FileEntry>,
    /// 因编码/权限等原因跳过的条目数
    pub skipped: u32,
    pub truncated: bool,
}

// ---------------------------------------------------------------- 规则

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Target {
    #[default]
    Name,
    Extension,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SeqPosition {
    #[default]
    Prefix,
    Suffix,
    Replace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaseKind {
    #[default]
    Lower,
    Upper,
    Title,
    Sentence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpaceMode {
    #[default]
    Keep,
    Underscore,
    Remove,
    Collapse,
}

/// 区间截取：保留一段，还是删掉一段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SliceMode {
    #[default]
    Keep,
    Cut,
}

/// 超长时的截断方式：直接砍尾巴，还是中间留省略号
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LimitMode {
    #[default]
    Truncate,
    Middle,
}

/// 插入时间戳的取值来源
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TimeSource {
    #[default]
    Modified,
    Created,
    Now,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Rule {
    /// 查找替换；regex=true 时 find 按正则解释，
    /// replace_with 用 `${1}`、`${name}` 形式引用捕获组（`$1_` 会被当成组名，必须加花括号）
    Replace {
        find: String,
        #[serde(default)]
        replace_with: String,
        #[serde(default)]
        regex: bool,
        #[serde(default)]
        case_sensitive: bool,
        #[serde(default)]
        all: bool,
    },
    /// 序号
    Sequence {
        #[serde(default)]
        start: u64,
        #[serde(default = "d_step")]
        step: u64,
        #[serde(default = "d_pad")]
        pad: u8,
        #[serde(default)]
        position: SeqPosition,
        #[serde(default)]
        separator: String,
    },
    /// 模板合成，作用于「文件名 + 扩展名」整体
    Template { pattern: String },
    /// 大小写
    Case { #[serde(default)] mode: CaseKind },
    /// 前后缀
    Affix {
        #[serde(default)]
        prefix: String,
        #[serde(default)]
        suffix: String,
    },
    /// 清理：空白、非法字符、首尾、连续重复
    Cleanup {
        #[serde(default)]
        spaces: SpaceMode,
        #[serde(default)]
        strip_illegal: bool,
        #[serde(default)]
        trim: bool,
        #[serde(default)]
        collapse_repeat: bool,
    },
    /// 按字符位置截取：下标 1 基，负数从末尾数，`end=0` 表示一直到末尾
    Slice {
        #[serde(default)]
        start: i64,
        #[serde(default)]
        end: i64,
        #[serde(default)]
        mode: SliceMode,
    },
    /// 在第 `at` 个字符之前插入文本（负数从末尾数，0 与 1 都是开头）
    Insert {
        #[serde(default)]
        at: i64,
        text: String,
    },
    /// 删除成对括号（连同里面的内容）
    RemoveBrackets {
        #[serde(default = "d_true")]
        round: bool,
        #[serde(default = "d_true")]
        square: bool,
        #[serde(default = "d_true")]
        curly: bool,
        #[serde(default = "d_true")]
        cjk: bool,
        #[serde(default)]
        keep_brackets: bool,
    },
    /// 去掉「 - 副本」「(2)」「 - copy」这类复制痕迹
    StripCopyMarks {
        #[serde(default = "d_true")]
        trailing_number: bool,
    },
    /// 限制长度，超出部分截掉
    LimitLength {
        #[serde(default = "d_limit")]
        max: usize,
        #[serde(default)]
        mode: LimitMode,
    },
    /// 直接设置扩展名（不看 target）
    SetExtension { ext: String },
    /// 把空格 / 下划线 / 连字符 / 点统一成一种分隔符
    NormalizeSeparators {
        #[serde(default = "d_sep")]
        to: String,
        #[serde(default = "d_true")]
        space: bool,
        #[serde(default = "d_true")]
        underscore: bool,
        #[serde(default = "d_true")]
        hyphen: bool,
        #[serde(default = "d_true")]
        dot: bool,
    },
    /// 剥掉开头 / 结尾固定的一串字符
    StripAffix {
        #[serde(default)]
        prefix: String,
        #[serde(default)]
        suffix: String,
    },
    /// 插入修改时间 / 创建时间 / 当前时间
    InsertTime {
        #[serde(default)]
        source: TimeSource,
        #[serde(default = "d_stamp_fmt")]
        format: String,
        #[serde(default)]
        position: SeqPosition,
        #[serde(default = "d_sep")]
        separator: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleInstance {
    pub id: String,
    #[serde(default = "d_true")]
    pub enabled: bool,
    #[serde(default)]
    pub target: Target,
    pub rule: Rule,
}

// ---------------------------------------------------------------- 预览 / 计划

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConflictPolicy {
    #[default]
    AutoSuffix,
    Skip,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRequest {
    pub session_id: String,
    /// null / 缺失 = 全部条目
    #[serde(default)]
    pub ids: Option<Vec<u64>>,
    pub rules: Vec<RuleInstance>,
    #[serde(default)]
    pub conflict_policy: ConflictPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IssueLevel {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IssueKind {
    EmptyName,
    IllegalChar,
    ReservedName,
    TooLong,
    Duplicate,
    ExistsOnDisk,
    CaseOnly,
    /// 模板里引用了元数据变量，但文件没有可用的元数据
    MetaMissing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub level: IssueLevel,
    pub kind: IssueKind,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    pub id: u64,
    pub src_name: String,
    pub dst_name: String,
    pub changed: bool,
    /// 目标名中发生变化的字符区间 [start, end)，按字符计
    pub diff_start: usize,
    pub diff_end: usize,
    pub issues: Vec<Issue>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewResult {
    pub items: Vec<PlanItem>,
    pub total: usize,
    pub changed: usize,
    pub errors: usize,
    pub warnings: usize,
    pub can_execute: bool,
}

// ---------------------------------------------------------------- 执行 / 撤销

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteRequest {
    pub session_id: String,
    #[serde(default)]
    pub ids: Option<Vec<u64>>,
    pub rules: Vec<RuleInstance>,
    #[serde(default)]
    pub conflict_policy: ConflictPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OperationState {
    Running,
    Completed,
    Partial,
    RolledBack,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedRename {
    pub src_name: String,
    pub dst_name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteResult {
    pub operation_id: String,
    pub total: usize,
    pub renamed: usize,
    pub skipped: usize,
    pub failed: Vec<FailedRename>,
    pub state: OperationState,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationSummary {
    pub id: String,
    pub dir: String,
    pub started_at: i64,
    pub total: usize,
    pub renamed: usize,
    pub failed: usize,
    pub state: OperationState,
    pub can_undo: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoFailure {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoResult {
    pub operation_id: String,
    pub restored: usize,
    pub skipped: usize,
    pub failed: Vec<UndoFailure>,
}

// ---------------------------------------------------------------- 设置 / 预设

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub recursive: bool,
    pub include_hidden: bool,
    pub include_files: bool,
    pub include_dirs: bool,
    pub sort_by: SortBy,
    pub sort_desc: bool,
    pub conflict_policy: ConflictPolicy,
    pub confirm_before_execute: bool,
    pub recent_dirs: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            recursive: false,
            include_hidden: false,
            include_files: true,
            include_dirs: false,
            sort_by: SortBy::Name,
            sort_desc: false,
            conflict_policy: ConflictPolicy::AutoSuffix,
            confirm_before_execute: false,
            recent_dirs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub rules: Vec<RuleInstance>,
    pub created: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetFile {
    pub presets: Vec<Preset>,
}

#[cfg(test)]
mod kind_tests {
    use super::FileKind;

    #[test]
    fn classifies_common_extensions() {
        assert_eq!(FileKind::of("jpg", false), FileKind::Image);
        assert_eq!(FileKind::of("PNG", false), FileKind::Image);
        assert_eq!(FileKind::of("mp4", false), FileKind::Video);
        assert_eq!(FileKind::of("flac", false), FileKind::Audio);
        assert_eq!(FileKind::of("docx", false), FileKind::Document);
        assert_eq!(FileKind::of("xlsx", false), FileKind::Sheet);
        assert_eq!(FileKind::of("pptx", false), FileKind::Slide);
        assert_eq!(FileKind::of("zip", false), FileKind::Archive);
        assert_eq!(FileKind::of("ts", false), FileKind::Code);
        assert_eq!(FileKind::of("exe", false), FileKind::Program);
        assert_eq!(FileKind::of("ttf", false), FileKind::Font);
        assert_eq!(FileKind::of("", false), FileKind::Other);
        assert_eq!(FileKind::of("unknownext", false), FileKind::Other);
        // 目录一律算文件夹，哪怕名字里带扩展名
        assert_eq!(FileKind::of("", true), FileKind::Folder);
        assert_eq!(FileKind::of("jpg", true), FileKind::Folder);
    }

    #[test]
    fn serde_uses_camel_case_tags() {
        let json = serde_json::to_string(&FileKind::Image).unwrap();
        assert_eq!(json, "\"image\"");
        let back: FileKind = serde_json::from_str("\"sheet\"").unwrap();
        assert_eq!(back, FileKind::Sheet);
    }
}
