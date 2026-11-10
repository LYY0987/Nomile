# Nomile · 批量文件重命名

Windows 桌面端批量重命名工具。Rust + Tauri 2 负责规则与文件操作，React + TypeScript 只负责渲染。

## 跑起来

前置：Node 18+、pnpm、Rust stable（MSVC toolchain）、WebView2 Runtime（Win11 自带）。

```bash
pnpm install
pnpm tauri dev
```

首次编译 Rust 依赖需要几分钟，之后增量几秒。

## 怎么用

1. 选来源：点「选目录」挑一个文件夹，或点「选文件」直接挑一批文件（支持多选）；
   目录模式下还能在下面「范围」那行收紧载入范围——含子目录 / 含隐藏项 / 含文件夹 /
   只看 `*.jpg, *.png` / 排除 `*.tmp`（多个模式用逗号分隔）。选文件时这行会置灰，因为文件是你亲手挑的，不该再被范围条件挡掉。
2. 左边加规则：查找替换 / 序号 / 模板 / 大小写 / 前后缀 / 清理，可叠加、可上下调序；
3. 右边列表上方是**列表自己的工具条**：排序（会影响 `${n}` 的取值）、按文件名搜索、
   全选 / 取消全选 / 反选 / 清空列表。它只管「怎么看这份列表」，不改「载入哪些文件」；
4. 预览实时显示「原名 → 新名」，高亮部分是改动；确认无误点「执行重命名」，
   改完随时能在右上「历史记录」里撤销。

> 两块条件的分工：**「范围」决定哪些文件被载入**（改了就重新载入），
> **「列表」决定这些已载入的文件怎么显示和被选中**（搜索只影响这一屏，不影响执行范围）。

## 三条规矩

- **预览即真相**：规则引擎在 Rust 里只有一份，预览和执行调的是同一个 `build_plan`，不会出现「看到的名字」和「落地的名字」不一致。
- **执行可撤销**：每次执行写 WAL 日志；撤销前比对 `size + mtime`，文件被外部改动过就跳过，绝不硬推。
- **永不覆盖**：目标重名时默认自动加 ` (1)`，或选择跳过。没有覆盖选项。

## 支持的规则

| 规则 | 说明 |
|---|---|
| 查找替换 | 支持正则与捕获组，替换串写 `${1}` / `${name}` |
| 序号 | 前缀 / 后缀 / 替换整名，可补零、可设步长 |
| 模板 | 用 `${name}` `${n:3}` `${date}` `${shot}` 等变量拼名字 |
| 大小写 | 全大写 / 全小写 / 首字母大写 / 句首大写 |
| 前后缀 | 加前缀、加后缀 |
| 清理 | 空格转下划线、去非法字符、去首尾空格、压缩重复 |

### 元数据命名

模板里可以用 `${shot}`（EXIF 拍摄时间）、`${camera}`、`${title}`、`${artist}`、`${album}`、`${track}`、`${duration}`、`${width}` 等变量，从图片 EXIF 和音频/视频标签里取名：

```
${shot}_${n:3}.${ext}   →   2023-05-06_070809_001.jpg
```

元数据**按需读取**：只有规则里真的用了这些变量才会打开文件读头；读过的按条目缓存，一次会话内不重复读盘。

## 开发

```bash
pnpm tauri dev            # 开发模式
pnpm exec tsc --noEmit    # 前端类型检查
cd src-tauri; cargo test  # Rust 单测
```

想用真实素材验证元数据解析（不设这两个变量时，对应测试自动跳过）：

```bash
$env:NOMILE_TEST_JPEG="...\shot.jpg"   # 带 DateTimeOriginal 的图片
$env:NOMILE_TEST_MP3="...\song.mp3"    # 带 title / artist 的音频
cd src-tauri; cargo test
```

## 代码结构

```
src-tauri/src/
├── model.rs         前后端共享契约（rename_all = camelCase）
├── domain/          纯函数：取名规则、计划构建、校验
│   ├── name.rs      文件名拆分、非法字符、保留名
│   ├── rules.rs     6 类规则 + 模板变量
│   └── planner.rs   build_plan：取名 + 校验 + 冲突检测
├── infra/           IO：扫描、两阶段执行、撤销日志、配置、元数据
└── commands.rs      9 个 Tauri 命令，只做编排

src/
├── lib/types.ts     与 model.rs 逐字段对应的手写类型
├── store/appStore.ts zustand 单 store
└── features/        Toolbar / RulePanel / PreviewTable / HistoryDrawer
```

## 状态

功能已跑通，尚未打包。设计与决策记录见 [docs/方案.md](docs/方案.md)。
