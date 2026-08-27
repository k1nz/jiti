# Jiti

快速翻译 / 语法检查桌面工具（工作代号）。Raycast 风格的系统级浮层：
全局快捷键唤起、常驻后台、不抢焦点、秒级显示。架构见
[docs/architecture.md](docs/architecture.md)（v0.5.0-rc.1，决策以文档为准）。
M5 RC 门禁见 [docs/m5-readiness.md](docs/m5-readiness.md)，双端 QA 见 [docs/m5-qa.md](docs/m5-qa.md)。
IPC 契约见 [docs/api-contract.md](docs/api-contract.md)。

## 技术栈

- 原生壳：Tauri 2（Rust）+ objc2（macOS 原生缝：无焦点面板 / 首帧同步 / 光标定位）
- 前端：Vue 3 + Vite + TypeScript + vue-i18n（`packages/ui`，主面板 `src/panel/`，设置独立入口 `src/settings/`）
- 数据库：SQLite（`history` / `mistakes`，`PRAGMA user_version` 迁移）
- IPC：tauri-specta 单源契约 → 构建时生成 `packages/ui/src/ipc/bindings.ts`

## 目录（按 architecture.md §10）

- `packages/ui/` — WebView 前端（Vue；面板常驻 + 设置独立入口）
- `packages/native/` — = src-tauri（Rust：commands / services / providers / capabilities）
- `rust-toolchain.toml` — 本机使用 stable Rust（高于系统默认 1.81）

## 运行

```bash
pnpm install            # 首次（pnpm 工作区）
pnpm dev                # tauri dev（自动起 vite + 编译 + 启动 App）
pnpm ui:typecheck       # vue-tsc
pnpm test               # vitest + cargo test（cargo test 会重新导出 bindings.ts）
pnpm check:versions     # 对齐 package / Cargo / tauri 版本与 identifier
pnpm build              # 生产构建（tauri build；macOS ad-hoc 签名）
```

CI：`.github/workflows/ci.yml`。内部 RC 包：`.github/workflows/rc-build.yml`（手动或 `v0.5.0-rc.*` tag）。正式签名发布：`.github/workflows/release.yml`（M5.1，需 `release` Environment 的 Apple 证书；Windows 签名与 `JITI_SENTRY_DSN` 可选且不与 Apple 凭据耦合）。

开发自检钩子（debug 构建，`services/dev.rs`）：

```bash
JITI_AUTOSHOW_MS=1500 pnpm dev   # 启动 1.5s 后自动唤起面板（与热键同代码路径）
JITI_CENTER=1 JITI_AUTOSHOW_MS=1500 pnpm dev   # 配合居中（截图对齐）
```

默认热键：

- macOS：`⌥⌘T` 翻译 / `⌥⌘G` 语法 / `⌥⌘Space` 统一面板
- Windows：`Ctrl+Shift+T` 翻译 / `Ctrl+Alt+G` 语法 / `Ctrl+Alt+Space` 统一面板

可在独立设置窗口里重配（面板 `⌘,` / `Ctrl-,`，或托盘「设置」）。托盘提供「显示面板 / 设置 / 退出」。