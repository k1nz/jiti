# M5 RC Ship-Readiness（75 项）

来源：native-feel `checklists/ship-readiness.md`。评分：Green ✓ / Red ✗ / N/A ◯。

**口径**
- A–G 不允许存在未解释的 Red。Red 必须修复或明确阻断 RC。
- 不属于本工具 v0.5 的能力标 N/A，不为过门禁临时加功能。
- 自动检查证据来自 `pnpm ui:typecheck` / `pnpm ui:test` / `cargo fmt --check` / `cargo clippy` / `cargo test` / `pnpm ui:build`。
- 窗口、IME、材质、多屏、辅助功能、自启写入 [`docs/m5-qa.md`](m5-qa.md)，不用单元测试冒充系统集成。
- 负责人默认工程；平台列 `mac` / `win` / `both`。

版本：`0.5.0-rc.1`。M5.1（签名公证、公开发布、生产崩溃上报）不在本表 RC 通过条件内。

## 自动门禁记录（2026-08-27）

在开发机（macOS）执行，全绿：

| 检查 | 结果 |
|---|---|
| `pnpm check:versions` | ok `0.5.0-rc.1` / `com.jiti.app` / ad-hoc `signingIdentity: "-"` / 无 sourcemap 配置 |
| `pnpm ui:typecheck` | 通过 |
| `pnpm ui:test` | 14 files / 66 tests 通过 |
| `pnpm ui:build` | 通过；`packages/ui/dist` 无 `*.map` |
| `cargo fmt --check` | 通过 |
| `cargo clippy --all-targets` | 通过（既有 warn，未当错误） |
| `cargo test` | 96 tests 通过（含崩溃上报清洗 3 项）；导出 specta 绑定 |
| 本机 `tauri build` | 通过；ad-hoc 签名 `Jiti.app` + `Jiti_0.5.0-rc.1_x64.dmg` |

本机 RC 产物（macOS 13 / x86_64，2026-08-27；**内部包，未公证**）：

| 产物 | SHA-256 |
|---|---|
| `Jiti.app/Contents/MacOS/jiti` | `fc36cc0cbcc3aacc209fa458b75a79b161a8870c7a5d6a89d2b773c9e68a2385` |
| `Jiti_0.5.0-rc.1_x64.dmg` | `4a19805ce473393be1041f294d32ab8ab6a6757078dbea8777089b213c75485d` |

抽查：`.app` 内 0 个 `*.map`，0 个 `keys.json` / sqlite；`CFBundleIdentifier=com.jiti.app`；`CFBundleShortVersionString=0.5.0-rc.1`；`codesign` 为 `adhoc,runtime`。Windows 包由 `rc-build.yml` 产出，本机未交叉编译。

真机感知指标与冒烟见 [`docs/m5-qa.md`](m5-qa.md)。下表「待核」不是 Red，须在 macOS / Windows 各填一份 QA 结果后改为 ✓/✗，未填完前内部 RC 可分发但不得宣称已完成双端验收。开发机当时已有 `tauri dev` 常驻，未再启动 RC `.app`，以免热键与单实例冲突。

---

## A. Cold launch（1–10）

| # | 项 | 状态 | 平台 | 证据 | 说明 |
|---|---|---|---|---|---|
| 1 | 热键→可见窗暖 <200ms / 冷 <600ms | 待核 | both | QA 计时；debug `[jiti] panel revealed after` | 目标见 architecture §13 |
| 2 | 无白/黑闪 | 待核 | both | QA；mac `_doAfterNextPresentationUpdate`，Win `NavigationCompleted` | `packages/native/src/services/panel.rs` |
| 3 | 出现在正确屏幕与位置 | ✓ | both | 单元：`clamp_panel_position`；QA 多屏 | 跟随光标 + 工作区夹取；不记忆位置 |
| 4 | 初始焦点在最可能的输入框 | ✓ | both | `App.vue` 搜索框；QA 可直接打字 | |
| 5 | 正常启动无 loading 占位 | ✓ | both | 面板预热，无启动骨架屏 | |
| 6 | Dock/任务栏图标正确 | 待核 | both | QA 看产物图标 | `tauri.conf.json` bundle.icon |
| 7 | 托盘图标单色或跟系统 | 待核 | both | QA | 使用应用图标；非 Electron 默认 |
| 8 | 全局热键首次启动即可用 | ✓ | both | `hotkeys::register` 在 setup | QA 再确认无重注册 |
| 9 | 自启仅用户选择后注册 | ✓ | both | 设置页开关；默认关 | QA 验证写入 |
| 10 | 退出即退出，无僵尸进程 | ✓ | both | 托盘「退出」走 `PredefinedMenuItem::quit` | QA 看 Activity Monitor / 任务管理器 |

## B. Window & focus（11–20）

| # | 项 | 状态 | 平台 | 证据 | 说明 |
|---|---|---|---|---|---|
| 11 | ⌘W / Ctrl-W 关窗 | ✓ | both | 主面板 CloseRequested 防关闭并 hide；前端同快捷键 | 只隐藏、保持预热 |
| 12 | ⌘M / Win-Down 最小化 | ◯ | both | — | Accessory + skipTaskbar，最小化没有任务栏入口；隐藏即预期。不为过门禁改成普通 App |
| 13 | 绿灯缩放而非全屏 | ◯ | mac | `maximizable: false` | 无边框工具窗，无交通灯全屏语义 |
| 14 | 点外部按配置隐藏 | ✓ | both | `should_dismiss_on_click_outside` 测试 + 全局鼠标监视 | 图钉固定后不关 |
| 15 | 跨启动记住尺寸位置 | ◯ | both | architecture §4.4 | 产品默认跟随鼠标；不为过门禁加「记住位置」 |
| 16 | 多屏打开在活动屏 | ✓ | both | mac `position_near_cursor`；Win `MonitorFromPoint` + 夹取 | QA 多屏 |
| 17 | 全屏用原生全屏 | ◯ | both | `fullscreen: false` | 本工具无全屏模式 |
| 18 | 设置是独立原生窗 | ✓ | both | `settings_window.rs`；⌘, / Ctrl-, | 关闭即销毁 |
| 19 | 无 DOM 遮罩对话框 | ✓ | both | 引导页占满面板，不是 modal | 确认类走原生 dialog（导出） |
| 20 | 失焦按配置隐藏 | ✓ | both | `should_dismiss_on_blur` 测试 | 图钉除外 |

## C. Input & cursor（21–30）

| # | 项 | 状态 | 平台 | 证据 | 说明 |
|---|---|---|---|---|---|
| 21 | 行/按钮/Tab 无 `cursor:pointer` | ✓ | both | `style.css` `cursor: default` | |
| 22 | chrome 文案不可选 | ✓ | both | `user-select: none`；内容区显式 `text` | |
| 23 | 无 WebKit 浏览器右键菜单 | ✓ | both | `disableBrowserChrome` + 原生拦菜单 | 不提供自定义菜单，彻底移除 |
| 24 | 无 force-touch / 长按链接预览 | ✓ | mac | `allowsLinkPreview = false` | |
| 25 | chrome 无拼写红线 | ✓ | both | 搜索框 `spellcheck="false"` | |
| 26 | IME composition 正确 | 待核 | both | [`m4-ime-qa.md`](m4-ime-qa.md) | 真机，不单测冒充 |
| 27 | Tab 全键盘可达 | ✓ | both | Tab / Shift+Tab 切模式；Vitest `panelKeys`；Tab 条 ←/→ | |
| 28 | 焦点环可见且平台样式 | ✓ | both | `:focus-visible` + `--j-focus-ring` | QA 再看 |
| 29 | Esc 永远有意义 | ✓ | both | 先关弹层再隐藏；录制中忽略 | `overlays.ts` + `hotkeys.ts` |
| 30 | 列表 type-ahead | ✓ | both | `typeahead.ts`；历史/错题列表 | |

## D. Visual & material（31–40）

| # | 项 | 状态 | 平台 | 证据 | 说明 |
|---|---|---|---|---|---|
| 31 | 平台材质 | ✓ | both | `window-vibrancy`：mac HudWindow、Win Acrylic→Blur | 设置窗不透明；真机看毛玻璃 |
| 32 | 深色跟随系统、无闪 | ✓ | both | `appearance.ts`；QA 切主题 | |
| 33 | 强调色跟系统强调色 | ◯ | both | architecture §8.2 | 产品锁死 amber，不用系统强调色；可选增强后置 |
| 34 | 系统字体，无 web font | ✓ | both | `system-ui` 栈；未打包字体 | |
| 35 | 无 CSS 窗口阴影 | ✓ | both | `shadow: true` 交给 OS；`.shell` 无 box-shadow | 筛选弹出层阴影是组件级，不是窗口 |
| 36 | 无 CSS 充当窗口圆角 | ✓ | both | 原生 layer `PANEL_CORNER_RADIUS` 12px；CSS 只裁内容 | 无边框窗必须裁内容，否则漏白 |
| 37 | 半透明区域能透出模糊 | 待核 | both | CSS `--j-panel-bg` 半透明；材质见 31 | 须在 Tauri 真窗验收，浏览器看不到 OS 模糊 |
| 38 | 再次确认无 pointer | ✓ | both | 同 21 | |
| 39 | `prefers-reduced-motion` | ✓ | both | `style.css` 媒体查询 | |
| 40 | 无页面/路由淡入 | ✓ | both | 无 vue-router；Tab 是切 | |

## E. Scrolling（41–45）

| # | 项 | 状态 | 平台 | 证据 | 说明 |
|---|---|---|---|---|---|
| 41 | Mac overlay 滚动条 | ✓ | mac | 不强制 `scrollbar-width: thin` 到主列表 | QA |
| 42 | Win11 细滚动条 | ✓ | win | 平台默认；不画假滚动条 | QA |
| 43 | 无 `behavior:'smooth'` | ✓ | both | 代码检索无 smooth scroll | |
| 44 | 滚动惯性原生 | 待核 | both | `overscroll-behavior: contain` | QA |
| 45 | 窗内导航保留滚动位置 | ✓ | both | 单窗四模式，切 Tab 不拆列表 DOM 以外的独立路由 | 可接受 |

## F. Performance（46–55）

| # | 项 | 状态 | 平台 | 证据 | 说明 |
|---|---|---|---|---|---|
| 46 | 空闲常驻 <500MB | 待核 | both | QA Activity Monitor / 任务管理器 | 目标 mac ~80–120、Win ~130–180 |
| 47 | 展开结果无顿挫 | 待核 | both | QA 语法卡片流式出现 | |
| 48 | 快打搜索无掉帧 | 待核 | both | QA | |
| 49 | 隐藏时 CPU <0.5% | 待核 | both | QA | §4.6 预热三件套 |
| 50 | 空闲 1h 电池影响 low | 待核 | mac | QA | |
| 51 | 隐藏窗不被 WebKit 节流 | 待核 | mac | QA：隐藏后再显示立刻可交互 | |
| 52 | 文件索引不占满核 | ◯ | both | — | 无文件索引器 |
| 53 | 扩展崩溃不拖垮应用 | ◯ | both | — | 无第三方扩展 |
| 54 | 网络超时 10s 内可见错误 | ✓ | both | `FIRST_BYTE_TIMEOUT=10s`；Rust 测试 | 总预算 20s，重试共享剩余 |
| 55 | >200ms 才有加载态 | ✓ | both | `loading.ts` + grammar store 定时器测试 | <200ms 不显示 spinner |

## G. System integration（56–65）

| # | 项 | 状态 | 平台 | 证据 | 说明 |
|---|---|---|---|---|---|
| 56 | URL Scheme | ◯ | both | architecture §11 | 不在 v0.5 |
| 57 | 文件关联 | ◯ | both | 同上 | 不在 v0.5 |
| 58 | 文件拖放真实 URL | ◯ | both | 非文件类工具 | 不在 v0.5 |
| 59 | 剪贴板写入期望类型 | ✓ | both | 复制结果走 `clipboard.writeText` | 纯文本足够 |
| 60 | 原生保存对话框 | ✓ | both | `tauri-plugin-dialog` 导出 Markdown | |
| 61 | 原生通知 | ◯ | both | architecture §11 | 不在 v0.5 |
| 62 | 自动更新 | ◯ | both | architecture §11 | 不在 v0.5 |
| 63 | 符号化崩溃上报 | ◯ | both | M5.1：`crash.rs` + `JITI_SENTRY_DSN` + sentry-cli 符号上传 | RC 不启用生产 Sentry |
| 64 | Windows 单实例 | ✓ | win | `tauri-plugin-single-instance` | 二次启动唤起已有面板 |
| 65 | bundle identifier / 版本 / 图标 | ✓ | both | `scripts/check-versions.mjs`；`com.jiti.app` | 无 sourcemap、无 Key 进包 |

## H. Accessibility（66–70）

| # | 项 | 状态 | 平台 | 证据 | 说明 |
|---|---|---|---|---|---|
| 66 | VoiceOver / Narrator 可导航 | 待核 | both | QA | H 段允许少量留 |
| 67 | 焦点移动有宣告 | ✓ | both | `aria-live` / `role=tab` / `aria-selected` | QA 再听 |
| 68 | 对比度 WCAG AA | ✓ | both | amber 强调 + 正文 token | 抽查 |
| 69 | 系统大字号 | 待核 | both | QA | |
| 70 | 无鼠标可完成操作 | ✓ | both | Tab + Enter + 快捷键 | QA |

## I. Cross-platform parity（71–75）

| # | 项 | 状态 | 平台 | 证据 | 说明 |
|---|---|---|---|---|---|
| 71 | 双端同一功能集 | ✓ | both | 权限引导仅 mac 有 AX，Win 无对应闸门 | 平台差异符合惯例 |
| 72 | 双端同一 IPC schema | ✓ | both | `SCHEMA_VERSION`；specta 生成绑定 | CI 检查 |
| 73 | 第三方扩展双端一致 | ◯ | both | — | 无扩展体系 |
| 74 | 视觉差只反映平台惯例 | ✓ | both | 系统字体/滚动条/材质 | |
| 75 | 修复大多一份代码两边生效 | ✓ | both | monorepo `packages/ui` + `packages/native` | 原生缝分平台文件 |

---

## RC 阻断规则

- A–G 中状态为 ✗ 且无「明确 N/A 理由」→ 阻断打 `v0.5.0-rc.1`。
- 「待核」项必须在 [`docs/m5-qa.md`](m5-qa.md) 留下真机结果后改为 ✓ 或 ✗。
- M5.1 才把 63 改为 ✓，并补签名/公证硬门禁。GitHub 需建立受保护 Environment `release`，注入 Apple / 可选 Windows 证书与 `JITI_SENTRY_DSN`。在版本改为 `0.5.0` 后打 `v0.5.0` 触发 [`../.github/workflows/release.yml`](../.github/workflows/release.yml)。Keychain 迁移不混入本次发布。
