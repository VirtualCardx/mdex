[English](README.md) | 简体中文

# mdex

一款桌面 Markdown 编辑器，围绕自定义的 **`.mdex` 文档格式**打造——`.mdex`
是一个单一的 ZIP 归档，把 Markdown 文档与其引用的资源打包在一起。基于
Tauri 2、React、CodeMirror 6 与 Rust 构建。

![mdex](src-tauri/icons/128x128.png)

## mdex 格式

`.mdex` 文件是标准的 ZIP 归档（deflate 压缩），布局如下：

```text
my-document.mdex
├── meta.json      # 格式元数据（见下文）
├── document.md    # Markdown 源文
└── assets/        # Markdown 引用的二进制资源
    └── logo.png
```

`meta.json`：

```json
{
  "format": "mdex",
  "version": 1,
  "title": "可选的文档标题",
  "created": "2026-09-25T00:00:00Z",
  "modified": "2026-09-25T00:00:00Z"
}
```

- `format` 必须为 `"mdex"`；`version` `1` 是当前版本。
- `document.md` 为必需条目；图片以 `![alt](assets/name.png)` 形式引用。
- 未知条目会被忽略，因此未来版本可以扩展该格式。
- 任何 ZIP 工具都能打开 `.mdex` 文件——它就是一个普通的归档。

仓库根目录下有一个 `sample.mdex` 示例，覆盖了全部特性。

## 特性

- **分栏视图**：CodeMirror 6 源码编辑器 + 实时渲染预览，按比例滚动同步；
  编辑 / 分栏 / 预览三种模式。
- **内嵌资源**：图片保存在归档内部，通过自定义 `mdexasset://` 协议提供给
  webview（从不解包到磁盘）。
- **Markdown 增强**：GFM 表格与任务列表、围栏代码块语法高亮
  （highlight.js）、KaTeX 数学公式、URL 自动链接。
- **图片工作流**：从剪贴板粘贴、拖放图片文件或使用工具栏插入——统一存储
  为归档资源。普通 `.md` 文件的外部图片保持在原处：相对引用（如
  `images/foo.png`）从文件所在目录加载，不会被复制或移动。
- **格式感知保存**：直接保存（Ctrl+S）保持文档来源格式——`.mdex` 归档仍
  存为归档，打开的 `.md` 文件原地写回纯文本（含嵌入图片时另写 `assets/`
  目录）。另存为提供两种格式并以当前格式预选，因此它同时也是 `.mdex` 与
  `.md` 之间的转换器：`.md` → `.mdex` 转换会把所有引用的外部图片收进归档
  并将链接重写到 `assets/` 条目，生成完全自包含的文件。
- **导出**：纯 `.md`（+ `assets/` 目录），或图片内联的单文件自包含 HTML。
- **文件关联（Windows）**：一键将 mdex 注册为 `.mdex` / `.md` 的默认打开
  应用——见下文专节。
- **体验细节**：浅色/深色主题（跨启动记忆选择，未手动选择前跟随系统外
  观）、关闭前未保存更改确认、Ctrl+S / Ctrl+Shift+S / Ctrl+O / Ctrl+N 快
  捷键、带字数与光标位置的状态栏。

## 快速开始

需要 Node.js 18+、pnpm 以及带 MSVC 的 Rust 工具链。

```bash
pnpm install          # 安装前端依赖
pnpm tauri dev        # 以开发模式运行应用
pnpm tauri build      # 构建 release 产物（NSIS 安装包 + exe）
```

其他常用命令：

```bash
node scripts/make-icon.mjs   # 重新生成 src-tauri/app-icon.png
pnpm tauri icon src-tauri/app-icon.png
cargo test --manifest-path src-tauri/Cargo.toml   # 格式往返测试
cargo test --manifest-path src-tauri/Cargo.toml --lib -- --ignored
                              # 文件关联注册表往返测试（需要真实 HKCU
                              # 访问权限；默认跳过）
```

## 文件关联（Windows）

mdex 可以把自己注册为 `.mdex` 与 `.md` 文件的默认"打开方式"，并把两种类
型加入资源管理器的**新建**右键菜单。点击工具栏的 **⚙ Link files** 按钮
即可注册（已注册时再次点击则移除注册）。注册后，双击文档会在 mdex 中打
开；若 mdex 已在运行，文件会路由到现有窗口而不是启动第二个实例
（`tauri-plugin-single-instance`），同样会先确认未保存的更改。

### 注册表写入内容

所有内容都位于 `HKEY_CURRENT_USER\Software\Classes` 之下——**无需管理员
权限**，且注册完全可逆（同一个按钮即可移除）：

```text
HKCU\Software\Classes
├── Mdex.Editor                          # .mdex 的 ProgID
│   (Default)          = "Mdex Archive Document"
│   DefaultIcon
│     (Default)        = "<path-to-mdex.exe>",0
│   shell\open\command
│     (Default)        = "<path-to-mdex.exe>" "%1"
├── Mdex.Markdown                        # .md 的 ProgID
│   (Default)          = "Markdown Document"
│   DefaultIcon
│     (Default)        = "<path-to-mdex.exe>",0
│   shell\open\command
│     (Default)        = "<path-to-mdex.exe>" "%1"
├── .mdex
│   (Default)          = Mdex.Editor            # 认领为默认
│   OpenWithProgids
│     Mdex.Editor      = ""
│   ShellNew
│     Data             = <最小 .mdex 模板，合法 ZIP 归档>
└── .md
    (Default)          = Mdex.Markdown          # 仅在未被占用时
    OpenWithProgids
      Mdex.Markdown     = ""
    ShellNew
      NullFile         = ""                     # 新建空 .md 文件
```

- 只有当扩展名的默认值未被其他应用占用（或已经是本应用）时才会写入扩展
  名默认值。Windows 10/11 用 `UserChoice` 哈希保护既有的每用户默认值，
  因此对于已被其他应用认领的扩展名，mdex 仍会出现在"打开方式"列表中，
  你可以在那里（或通过 设置 → 默认应用）确认它为默认。
- `ShellNew` 键把两种文件类型加入资源管理器的**新建**右键菜单，呈现为两个
  可区分的条目——"Mdex Archive Document"（`.mdex`）与 "Markdown
  Document"（`.md`），每个扩展名一个专属 ProgID。新建的 `.mdex` 文件由内
  嵌的最小模板（`meta.json` + `document.md`）写入，打开即是合法归档；新
  建的 `.md` 文件为空。若 `.md` 已被其他应用关联，则显示该应用的名称，mdex
  不会抢占。
- 注册后通过 `SHChangeNotify(SHCNE_ASSOCCHANGED)` 通知资源管理器，图标与
  右键菜单立即刷新。
- `pnpm tauri build` 产出的 NSIS 安装包通过 `tauri.conf.json` 的
  `bundle.fileAssociations` 以声明式方式注册同样的关联（卸载时移除）。
- 注册记录的是当前运行的可执行文件路径——使用 `pnpm tauri dev` 开发时这
  是调试版二进制，安装 release 版后需要重新注册。

如有残留条目，可手动删除：

```text
reg delete "HKCU\Software\Classes\Mdex.Editor" /f
reg delete "HKCU\Software\Classes\.mdex\OpenWithProgids\Mdex.Editor" /f
reg delete "HKCU\Software\Classes\.md\OpenWithProgids\Mdex.Editor" /f
```

## 架构

```text
src/                    React 前端
  App.tsx               文档状态、快捷键、对话框、导出流程
  components/           Editor（CodeMirror）、Preview、Toolbar、StatusBar
  lib/markdown.ts       markdown-it 渲染管线；把 assets/* 重写为 mdexasset://
  lib/actions.ts        作用于编辑器视图的工具栏文本变换
  lib/api.ts            Tauri 命令与对话框的类型化封装
  lib/settings.ts       通过 localStorage 持久化的界面偏好（主题）
src-tauri/
  src/mdex.rs           mdex 格式核心：ZIP 归档读写、元数据
  src/commands.rs       Tauri 命令 + mdexasset:// 协议处理器
  src/fileassoc.rs      Windows 文件关联注册表（HKCU，可逆）
  src/lib.rs            应用构建器、插件、状态、命令注册
```

Rust 后端负责全部文件 I/O。当前打开的文档（markdown、资源、元数据）保
存在托管状态中；保存采用原子写入（临时文件 + 重命名）。前端不直接接触
文件系统。

## 路线图

- 最近文件菜单
- 多文档标签页
- 资源管理面板（重命名/替换/清理未使用资源）
- 文档大纲 / 目录侧边栏
- Mermaid 图表支持
