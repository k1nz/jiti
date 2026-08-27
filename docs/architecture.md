# Jiti · 快速翻译 / 语法检查桌面工具 · 技术架构设计

> 工作代号：**Jiti**（可随时改名）
> 版本：**v0.4.0** · 2026-08-27 · 面向 macOS 与 Windows 10/11
> 目标形态：Raycast 风格的小弹窗，全局快捷键唤起，常驻后台，秒级显示
> v0.2 变更：依据 `native-feel-cross-platform-desktop` 技能完成架构审计（哲学八原则、WebView 存活清单、IPC 单契约、内存基线修正），依据 `design-taste-frontend` 重写 UI 层设计规范
> v0.2.2 变更：M2 语法检查闭环（SSE+NDJSON、错误卡片、grammar 历史、进程内 LRU）
> v0.3.0 变更：M3 错题本（版本化 SQLite、自动/手动收录、筛选、Markdown 导出、AI 复习）；LanguageTool / i18n / 主题 / 自启仍属后续里程碑
> v0.4.0 变更：M4 设置完善（独立设置窗口、开机自启、vue-i18n zh-CN/en-US、浅/深/系统主题、IME 专项 QA）；弹窗位置策略与 LanguageTool 仍后置

---

## 1. 产品定位与需求

### 1.1 一句话定位
一个由全局快捷键唤醒的系统级浮层工具：自动读取你当前**选中的文字**，按需执行**中英翻译**或**英语语法检查**，错题自动归档成**错题本**，可 AI 总结复习。

### 1.2 确认后的需求清单（对应 v1）

| # | 需求 | 说明 |
|---|---|---|
| 1 | 快速翻译 | 中↔英（双向、自动判向），选中即弹，可手动输入 |
| 2 | 语法检查 & 改错 | 针对英语句子：找错、给修改、逐条讲解；错误卡片呈现 |
| 3 | 错题本 | 自动收录每次语法错误；按类型/时间过滤；导出 Markdown |
| 4 | AI 复习 | 一键总结高频错误、生成复习要点 |
| 5 | 历史记录 | 最近查询历史，可翻看 |
| 6 | 设置 | 快捷键重配、开机自启、多语言设定、引擎管理与测试 |
| 7 | 交互 | 选中即弹 + 手动输入；Tab 切模式；不同快捷键直达对应模式 |
| 8 | 引擎 | 可插拔 Provider：云端大模型 + DeepL / 有道 / Google 等翻译 API |
| 9 | 服务模式 | v1 用户自带 Key、纯本地；**架构预留**未来后端 / SaaS 切换 |

### 1.3 非目标（v1 明确不做，避免范围失控）
- ❌ **Grammarly 式内联纠错**（在其它 App 输入框里实时划红线）。需要输入法/文本注入级别的系统能力，二期评估。
- ❌ 多端云同步错题本（Schema 预留字段，不实现）。
- ❌ 自有后端 / 账号 / 计费（仅预留边界）。

---

## 2. 技术选型总览

### 2.1 选型表

| 层 | 选型 | 理由 |
|---|---|---|
| 应用壳（原生） | **Tauri 2**（Rust） | 一套代码出 macOS + Windows；不捆绑 Chromium；官方宣称最小 ≈600KB（实际工程几 MB）；官方插件覆盖热键 / 自启 / SQL / 托盘 |
| 渲染引擎 | macOS **WKWebView** / Windows **WebView2** | 系统自带、无捆绑浏览器；Win11 预装 WebView2，Win10 绝大多数已装 |
| 前端框架 | **Vue 3 + Vite + TypeScript** | 熟悉、轻量；生态与 Tauri 配合顺 |
| 后端语言 | **Rust** | 热键、窗口、选中读取、密钥托管、统一出网、LLM 流式 |
| 数据库 | **SQLite**（rusqlite + `PRAGMA user_version`） | 错题本 / 历史本地持久化 |
| 密钥存储 | 本地 `keys.json`（tauri-plugin-store） | API Key 不进 WebView；不走 Keychain，避免每次弹系统密码 |
| IPC 契约 | **tauri-specta**（Rust 单源 → 生成 TS 类型） | 一份 schema，两端类型永远同步（详见 §3.5） |
| 引擎 | **可插拔 Provider 层** | LLM（OpenAI 兼容）+ DeepL / 有道 / Google / Baidu |

### 2.2 为什么「不用 Electron」是对的决定，以及 Tauri 替代了什么
Electron 的体量主要来自捆绑一套 Chromium（安装包 100MB+、内存数百 MB）。Tauri 是「**薄原生壳 + 系统 WebView**」：macOS 直接用 Apple 的 **WKWebView**（与 Safari 同源内核），Windows 用微软的 **WebView2**（Edge 内核，Win11 系统内置）。前端照样写 Vue/React，原生能力由 Rust 补齐。

### 2.3 你之前在 macOS 上想用 Swift，还需要吗？
不需要，但没关死。两条口子开放：

1. Tauri 允许在 Rust 里通过 `objc2` 直接调 AppKit / CoreGraphics（本项目「不抢焦点的面板」「读取选中文本」「系统材质」「杀掉 WebKit 右键菜单」都靠它）；
2. 将来若想把 macOS 版改写成**纯 Swift + WKWebView**：本架构把「UI + 业务逻辑」打包成一套**与壳无关**的 Web 资源，Swift 壳只是另一个薄容器。所以 UI/核心层的干净边界本身就是对「将来用 Swift 重写壳」的预留。

### 2.4 前端 Vue 3 的形态
面板是**一个无边框、自绘 UI 的窗口**（非浏览器网站）。Vue 负责：弹窗整体、模式 Tab、翻译/语法结果渲染、错题本管理、设置表单、快捷键/权限引导。状态用 `Pinia`，样式用 CSS 变量（原生系统字体 + 平台材质，详见 §8）。

---

## 3. 总体架构

### 3.1 架构图

```
┌───────────────────────────────────────────────────────────────┐
│                    Vue 3 + Vite + TS（弹窗 UI）                  │
│  翻译 · 语法 · 错题本 · 历史 · 设置（设置独立入口，用后即拆）     │
│  core/（纯 TS 业务：store、normalize、provider 契约、i18n）      │
└──────────────┬─────────────────────────┬──────────────────────┘
        invoke()（命令，tauri-specta 生成类型）  事件（event，分离 schema）
┌──────────────▼─────────────────────────▼──────────────────────┐
│                     Rust（Tauri 2 原生层）                      │
│  commands: translate / grammar_check / mistakes / history /    │
│            settings / hotkeys / autostart / export              │
│  services: selection(选中读取)  panel(窗口/焦点/材质)  secrets   │
│            engine_http(统一出网)  store(SQLite/JSON)            │
│  native-glue: objc2(macOS) / windows crate(Win)                │
│    · 渲染表面以下的一切：热键、窗口、材质、IME、右键菜单        │
└──────────────┬─────────────────────────────────────────────────┘
        reqwest（出口，规避 WebView CORS）
┌──────────────▼─────────────────────────────────────────────────┐
│  引擎 Providers（配置化、可插拔）                                 │
│  LLM(OpenAI兼容) · DeepL · 有道 · Google · (Languagetool 可选)  │
└─────────────────────────────────────────────────────────────────┘
```

### 3.2 分层职责

| 层 | 职责 | 不放什么 |
|---|---|---|
| Vue UI | 渲染、交互、本地状态、模式切换、设置表单、权限引导 | 不直接发 HTTP 给引擎；持密钥 |
| TS `core` | 纯函数：结果归一化、Provider 请求构建、设置校验、i18n | 不碰文件系统/密钥 |
| Rust 原生 | 热键、窗口显示/定位/焦点/材质、选中文本读取、剪贴板、密钥、SQLite、**所有出网 HTTP**、进程/单例/自启 | 不放 UI 逻辑 |
| 引擎 Provider | 具体 API 适配 | 互相不通 |

### 3.3 三个关键架构决策

**D1 · 业务编排放 TS，出网请求走 Rust。**
WebView 里 `fetch` 受 CORS 约束。所有引擎 HTTP 一律由 Rust `reqwest` 发出（统一超时、重试、错误分类、LLM 流式 SSE）。Vue 只通过 `invoke('translate')` / `invoke('grammar_check')` 等语义命令调用。这同时让未来「本地 → 远程后端」只替换 Rust 这一跳。

**D2 · API Key 与引擎配置永不出现在 WebView。**
Key 存应用数据目录的 `keys.json`（tauri-plugin-store，与 `settings.json` 分开）。Rust 命令内部拼请求；WebView 只能拿到「哪个 Provider 已配置 / 是否可用 / 测试结果」。存储后端可在签名后换成加密库 / Keychain，IPC 契约不变。开发期不走 Keychain，避免未签名构建每次弹系统密码。详见 §6.2。

**D3 · 原生壳保持「薄」，但渲染表面以下绝不妥协。**
UI 与业务核心是可复用资产；窗口、热键、材质、IME、右键菜单这类"渲染表面以下"的系统能力，在 Tauri 里通过 `objc2` / `windows` crate 直接触达 AppKit / Win32，不绕开、不假装能用抽象层搞定（详见 §3.6 哲学 T1）。

### 3.4 数据流示例

**翻译（点按热键 → 结果出现，目标 <1s 感知）**
```
热键触发 ─▶ Rust: 读选中文本(AX/剪贴板) ─▶ event 'hotkey://pressed{translate}'
Vue 收到 ─▶ 输入框填入选中文案 ─▶ invoke translate{text,to:'en',engine:default}
Rust: 取 keys.json→DeepL key → reqwest POST → 归一化 → 写 history → 返回
Vue: 渲染译文 + 来源引擎 + 耗时；⌘C 复制 / Esc 隐藏
```

**语法检查（M2：单次 OpenAI 兼容请求 + SSE + NDJSON）**
```
热键 ⌥⌘G / Ctrl+Alt+G 或手动 Enter
  ─▶ Vue GrammarView 填入选中文本
  ─▶ invoke grammar_check{text, engine:'llm', on_progress: Channel}
Rust: 组装固定提示词 → reqwest SSE（20s 超时）
    → 按 SSE 帧抽出 token → 按 NDJSON 行解码
    → Channel 立即推 overall / correctedText / error / retrying / finished
    → 聚合校验后返回 GrammarCheckOutcome；成功写入 history（kind=grammar），并按偏好自动收录错题
Vue: 首条语义记录即出卡片；用户从不看到 JSON/SSE。⌘C 复制改写。
结构解析失败：清空临时结果，非流式严格 JSON 再打一次。网络/鉴权/限流不重试。
```

M2 边界：不做 LanguageTool、后台取消。LanguageTool 后续只加适配器并复用同一 `GrammarResult`。

**错题本（M3：自动/手动收录 + 筛选 + 导出 + AI 复习）**
```
语法成功 ─▶ 若 autoCollect：事务批量写入 mistakes（无错误不写；失败整批回滚）
         ─▶ 返回 GrammarCheckOutcome { result, mistakeIds }
错题本 Tab ─▶ mistakes_list(filter) 类型/状态/时间筛选
         ─▶ mistakes_export：按类型分组 Markdown，系统保存对话框
         ─▶ mistakes_ai_review：聚合频次+代表例句 → LLM → history(kind=ai_review)
```
偏好在 `settings.json` 的独立 `mistakes` 键（默认 `autoCollect=true`、`defaultStatus=open`），不混进 Provider 配置。


### 3.5 IPC 契约：一份 schema，两端编译期同步

（对应 native-feel 技能 T2 *one schema, many languages* 与 T6 *cross boundaries intentionally*）

**规则：不在两端手写类型。** 用 **`tauri-specta`** 让 Rust 命令/事件的类型成为唯一事实源，构建时生成 TypeScript 绑定（`bindings.ts`）。改一个 Rust 结构体，TS 侧不通过编译，类型漂移在工程上不可能发生。

```rust
// src-tauri/src/main.rs
tauri_specta::ts::export(
    ExportConfig::new("../packages/ui/src/ipc/bindings.ts")
        .bigint(tauri_specta::ts::BigIntExportBehavior::Number),
    &[// 命令函数引用列表
    ],
)
```

如果将来演进为「每平台原生壳 + 共享 WebView」（§3.7 预留路径），同一份契约的 `shell ↔ core` 段改用 **UniFFI** 生成 Swift/C# 绑定；WebView ↔ core 段仍用 tauri-specta。契约文件 [`docs/api-contract.md`](api-contract.md) 同时是未来后端 OpenAPI 的初稿。

**两条不同的消息形状（必须分开设计）：**
- **Request/Response**（同步语义，带 correlation ID）：`translate` / `grammar_check` / `mistakes_*` 等。
- **Event**（fire-and-forget 广播）：`hotkey://pressed` / `capture://changed` / `panel://visibility` / `engine://error`。
- `RequestKind` 与 `EventKind` 是**互不相交的两个枚举**，schema 分文件，避免混用。

**开发期追踪：** 每次 IPC 调用打日志 `{requestId, kind, durationMs}`，你会立刻发现慢 handler（T6）。**版本化：** 常量 `SCHEMA_VERSION`，破坏性变更必升；新字段一律 optional；不删字段、不重命名字段（留过弃窗口）。

### 3.6 架构哲学：native-feel 八原则如何落到 Jiti

（native-feel 技能 `references/01-philosophy.md`，本表是逐条映射）

| 原则 | 在 Jiti 的落法 |
|---|---|
| **T1 把缝划在渲染表面** | 渲染表面以下（窗口、热键、材质、托盘、IME、右键菜单、辅助功能）全部原生：macOS 经 `objc2` 触 AppKit，Windows 经 `windows` crate 触 Win32。以上（Vue 树、业务逻辑）共享一套。判断一切跨平台决策问一句：在表面之上还是之下？ |
| **T2 一份 schema，多语言生成** | §3.5 的 tauri-specta 单源契约；禁用两端手写 marshalling |
| **T3 采用平台，别跟平台较劲** | 系统字体、系统滚动条、系统深色模式、系统焦点环、平台材质（vibrancy/mica）；只覆盖「平台默认反而破坏原生感」的极少数点（如 WebKit 的浏览器右键菜单） |
| **T4 性能是感知属性** | 目标不是"内存小"，是"热键下去弹窗就出来"。感知目标见 §13 |
| **T5 短迭代环就是产品** | Vue HMR 200ms 级迭代，这正是选 WebView UI 的理由；任何拉长 UI 迭代环的改动都要自证价值 |
| **T6 有意跨越边界** | IPC 是设计决策：频率、载荷、批量、缓存都要审计（§3.5 开发期追踪） |
| **T7 身份是肌肉记忆** | 快捷键、默认引擎、错误卡片交互这些用户已经/将要养成的习惯，在迭代中保持稳定，不追求"换新"而破坏惯性 |
| **T8 基线成本与边际成本分开** | 系统 WebView 的内存是**租来的基线**，接受它并如实告知用户；我们代码产生的堆、缓存、泄漏是**自有边际成本**，优化火力全打在这里（§13） |

### 3.7 决策树结论：精简三层，跳过 Node（含代价声明）

（native-feel 技能 `checklists/decision-tree.md`，如实跑完）

| 问题 | 答案 | 结论 |
|---|---|---|
| Q1 目标 OS | macOS + Windows | 该架构适用 |
| Q2 原生手感是否硬需求 | 是（快捷键工具，用户整天在用） | 适用 |
| Q3 插件生态 | 无，纯 UI + 少量原生工作 | **跳过 Layer 3（Node）**，`shell + WebView + 小 Rust core` 即可 |
| Q4 启动预算 | 冷启可 <600ms，**热启必须 <50ms** | 预热隐藏窗即可（§4） |
| Q5 内存预算 | 非 <150MB 强制 | 适用，但要接受 WebView 基线 |
| Q6 团队 | solo Web 开发者，无原生经验 | **用 Tauri 而非双原生壳**（短跑道解法） |
| Q7 跑道 | 短（数月出 v1） | 技能明示：短跑道用 **Tauri**，有产品市场契合后再演进 |

**代价声明（不回避）：** 技能的完整形态是「每平台原生壳（Swift / C#）+ 共享 WebView」，Tauri 是这形态的简化容器。Tauri 对窗口/材质/托盘是**抽象层**，在"每个动画都要跟 OS 逐帧一致"的场景会泄漏；Jiti 是小型工具，可接受，但我们在 Tauri 里用 `objc2`/`windows` crate 把上述"缝"补成原生（等效于技能 T1）。残余代价：Tauri 升级节奏、两处各写少量原生 glue（不共享）、以及极少数"只能在纯原生壳做到"的细节（如 `NSGlassEffectView` 液态玻璃深度定制）在 Tauri 里更费劲。若日后要做到像素级 Raycast 手感，**架构预留了"换壳不换 UI"的演进路径**（§2.3）。

---

## 4. 弹窗与唤醒（Raycast 式体验）

### 4.1 生命周期：常驻 + 预热，而不是「唤醒即启动」

- App 常驻后台：macOS `activationPolicy = .accessory`（无 Dock 图标）；Windows 隐藏主窗 + 可选托盘。
- 启动即注册全局热键、创建**隐藏**面板窗口、加载 Web bundle 并保持 WebView 存活。
- 热键触发：`set_position → show()`，需要输入才 `set_focus()`。**不销毁、不重建** → 热启画面 <30ms。
- 关闭 = `hide()`；退出走托盘菜单。单例插件 `tauri-plugin-single-instance` 防竞态。
- **主面板窗口永远预热**；**设置窗口独立入口、用后即拆**（内存规则见 §13：二级窗口拆得越狠，内存越干净）。

### 4.2 macOS「不抢焦点」方案（体验核心）

快速词典类工具的关键：弹窗出现时**不打断用户在原应用里的输入**。系统原生做法是 `NSPanel` + `nonactivatingPanel` + `orderFrontRegardless` + `becomesKeyOnlyIfNeeded`。Tauri 窗口是普通 NSWindow，经 `objc2` 补：

1. `set_activation_policy(Accessory)`，应用不成为前台主应用。
2. 显示时绕过 `activate()`，调 `orderFrontRegardless`。
3. 焦点策略：默认不夺焦；用户点进输入框（手动模式）才 `set_focus()`。
4. 材质走 `NSVisualEffectView`（macOS 26+ 可评估 `NSGlassEffectView`），透明由 `macosPrivateApi` 开启，WebView `drawsBackground=false` + CSS `background: transparent`。

> ⚠️ 若 v1 为保进度接受「激活才显示/手动输入必须焦点」的简化，设置里留开关。**默认目标是不激活。**

### 4.3 Windows 差异
- Win11 支持 Mica/Acrylic（`DWMWA_SYSTEMBACKDROP_TYPE` + `DwmExtendFrameIntoClientArea` + WebView2 透明背景），成熟度低于 macOS。v1 用稳妥视觉，Mica 作增值。
- 焦点语义：无手动输入需要时尽量避免激活（`WS_EX_NOACTIVATE`），需要输入时正常激活。托盘图标是 Windows 主流形态。

### 4.4 位置策略
默认**跟随鼠标**（读光标坐标 → 换算显示器工作区 → 贴边出现在光标附近，不遮光标）；设置可切「屏幕中央 / 记住位置」；记住每屏位置（多显示器）。

### 4.5 「选中即弹」：读取当前选中文本的机制链

按**降级链**实现（每个平台一个 capture 模块）：

**macOS**
1. **AX（首选）**：`objc2` 调 `AXUIElement`（前台 App → 聚焦元素 → `kAXSelectedTextAttribute`）。需**辅助功能权限**。浏览器选区经常滞后，AX 只作乐观预填。
2. **剪贴板模拟（始终跑）**：记下源进程 PID，等修饰键松开后 `CGEventPostToPid` 对该进程发 Cmd+C → 变更 `changeCount` 才采用 → 立刻回写恢复原剪贴板。每次热键一个 epoch，迟到的上一次复制不得写回输入框。
3. 都拿不到 → Vue 进入「手动输入」友好空态。捕获后不抢焦点。

**Windows**
1. **UI Automation（首选）**：`windows` crate 取聚焦元素 `TextPattern2/ValuePattern`。
2. **Ctrl+C 模拟（始终跑）** + `GetClipboardSequenceNumber` 确认复制发生 + 剪贴板恢复；同样用 epoch 丢弃过期结果。
3. 空 → 手动输入态。

### 4.6 WebView 存活指南（防坑清单，开工前必读）

（native-feel 技能 `references/03-webview-survival.md` 逐条消化，按 Tauri/Rust 落地。**大部分 bug 设计期就能绕开，不要上线后修。**）

**macOS / WKWebView**

| 坑 | 症状 | 解法（Rust/objc2 或 config） |
|---|---|---|
| A.1 隐藏窗节流 | 预热隐藏窗被 WebKit 判定不可见，rAF 掉到 1Hz，首次显示卡顿 | `windowOcclusionDetectionEnabled=false`；预热期 `alphaValue=0 + orderFront`；JS 里跑空转 `requestAnimationFrame` 保活循环 |
| A.2 首帧白/黑闪 | 先显示窗口，内容后画出来 | 用 `_doAfterNextPresentationUpdate` 同步到首帧再 show；私有但稳定近 10 年，后备 50ms 延时 |
| A.3 高度展开留白 | 弹窗随结果变高，新露区域空 1-2 帧 | **WebView frame 恒为最大高度，只改宿主窗口尺寸**（窗口裁剪，WebView 永远全量渲染） |
| A.4 动画尺寸抖动 | `setFrame(animate:true)` 挂起子视图绘制 | 改 Core Animation 事务（`NSAnimationContext`）驱动，WebView 保持逐帧绘制 |
| A.5 透明/毛玻璃 | 默认 WebView 画不透明白底 | `drawsBackground=false` + CSS `body{background:transparent}`，材质由 `NSVisualEffectView` 透出 |
| A.6 浏览器右键菜单 | 右键出现 Reload / Inspect Element | 覆写 `willOpenMenu` 清空（或换原生项）；禁 `-webkit-touch-callout`、force-touch 字典查询 |
| A.7 弹层越窗裁剪 | DOM 版 tooltip/popover 被窗口裁剪、不跟手 | **弹层一律走 IPC → 原生 `NSPopover` / borderless Window 渲染**（edge 处理、阴影、动画、焦点全交给 OS） |
| A.8 视图切换闪帧 | Tab/详情切换时一帧空白/未渲染 | **切，不淡入淡出**；旧视图挂到新视图首帧后再卸载；inline 关键 CSS |
| A.9 字体兜底冷启 | 首个 emoji / CJK 字符出现时顿一下或方块 | 启动时渲染一个隐藏 span 预装 `Apple Color Emoji` + `PingFang SC` 字体缓存（双 rAF 后移除），**双语应用必做** |
| A.10 特性开关 | 120Hz 屏上 WebView 锁 60Hz | 枚举 `_WKExperimentalFeature` 解锁刷新率；`RequestIdleCallbackEnabled` 可选 |
| A.11 滚动惯性 | 列表像网页不像原生 | 内层滚动 `overscroll-behavior: contain` 杀橡皮筋；主列表滚动交给平台 |

**Windows / WebView2**

| 坑 | 症状 | 解法 |
|---|---|---|
| B.1 初始化白闪 | 窗口先显示，WebView 100-300ms 后才画 | 先 `EnsureCoreWebView2Async`，`DefaultBackgroundColor` 设为透明/窗口底色（默认白），`NavigationCompleted` 后再 show |
| B.2 亚克力 + 无边框 | 亚克力不渲染 / WebView 不透明盖住 | `DWMWA_SYSTEMBACKDROP_TYPE` + `DwmExtendFrameIntoClientArea` + WebView 透明 + 拖拽区 `IsNonClientRegionSupportEnabled=true`（Win10/11 各版本都要测） |
| B.3 后台节流 | 隐藏窗时 timer/JS 被 Chromium 掐 | `IsBackgroundResourceLoadingEnabled=true` + env 参数 `--disable-renderer-backgrounding --disable-background-timer-throttling --disable-backgrounding-occluded-windows` |
| B.4 崩溃隔离 | 一个窗口崩拖垮全部 | 每个逻辑窗口独立 `CoreWebView2Environment`（主面板 / 设置分离） |
| B.5 IME | 中文输入法候选框错位/鬼字符 | WebView `GotFocus` 确保收 IME 事件；自定义标题栏别抢焦点；**Pinyin 重点测** |

**跨平台（CSS / 行为红线）**
- 可点击行 `cursor: default`（原生不动指针；`cursor:pointer` 是"网页味"第一 tell）
- chrome 默认 `user-select:none`，仅内容区/输入区 `user-select:text`
- 系统滚动条约定（mac 悬浮式 / Win11 细条），不做自定义美化
- 程序化滚动 `behavior:'auto'`（禁 smooth）
- 视图切换不做页面过渡（native 是"切"）
- `prefers-reduced-motion` 关闭全部非必要动画

### 4.7 弹层策略
一切 tooltip / popover / 悬浮提示走 **IPC → 原生弹层**（macOS `NSPopover`、Windows 无边框 Window），不做 DOM 浮层（§4.6 A.7）。这是"原生感"杠杆最高的改动之一。

---

## 5. 引擎与 Provider 层

### 5.1 Provider 注册表（设置里统一管理）

```
providers:
  llm:       { baseUrl, apiKey?(keys.json), kind: 'openai'|'claude'|'deepseek'|'custom', model, temperature, maxTokens }
  deepl:     { apiKey?(keys.json), formality }
  youdao:    { appKey?(keys.json), appSecret?(keys.json) }
  google:    { apiKey?(keys.json) }
  languagetool: { mode: 'off'|'local'|'online', localPort? }
defaults:
  translate: deepl  (fallback: llm)
  grammar:   llm    (fallback: languagetool)
```

- 每个 Provider 一个适配器（Rust enum match / trait），实现「构建请求 → reqwest → 解析 → 归一化」。
- 设置里「测试连接」按钮：读 keys.json 后真实打一次 API，只返回 ok/fail + 原因。
- 网络可达性：若在国内，DeepL/Google 直连可能不顺，有道/DeepSeek/国内镜像更可用；设置里标注可达性。

### 5.2 翻译适配器差异点

| Provider | 认证 | 说明 |
|---|---|---|
| DeepL | `auth_key` header | 免费档有限额；质量好 |
| 有道智云 | appKey+appSecret 签名 | 国内直连最稳，有免费额度 |
| Google Cloud Translation | apiKey | 按量收费；需可达网络 |
| Baidu | appKey/appSecret | 备选 |
| LLM | OpenAI 兼容协议 | 「高级翻译」fallback / 可选；质量最高、慢且贵 |

统一进出参：`TranslateRequest{text, from?, to}` → `TranslateResult{output, detectedFrom, engine, durationMs}`。

### 5.3 语法检查适配器
- **LLM（M2 主力）**：单次 chat/completions。`stream=true` 走 SSE；业务协议是 NDJSON 行（`overall` → `correctedText` → `error*` → `done`），不是自然语言打字机，也不是二次 JSON 请求。模型只返回原文片段；Rust 仅在片段唯一匹配时写入 Unicode 字符偏移。温度固定接近 0。解析失败（缺行 / 非法枚举 / 无 `done`）清空临时结果并**只重试一次**非流式严格 JSON。
- **LanguageTool（后续可选）**：enum-match 已留分支，M2/M3 不实现配置和 UI。归一化目标仍是同一 `GrammarResult`。

### 5.4 结果标准化 Schema（两端共用，由 tauri-specta 生成）

```ts
interface TranslateResult {
  engine: string; output: string;
  input: string; detectedFrom?: string; target: string; durationMs: number;
}
interface GrammarResult {
  engine: string; input: string;
  errors: GrammarError[];
  correctedText?: string;   // 整句改后
  overall?: string;         // 一句话总评（中文）
  durationMs: number;
}
interface GrammarError {
  offset?: number; length?: number;   // 原文 rune 偏移（可选）
  fragment: string; correction: string;
  type: 'grammar'|'spelling'|'punctuation'|'word_choice'|'style';
  severity: 'low'|'medium'|'high';
  explanation: string;               // 面向学习者的中文讲解
  suggestions: string[];
}
```

### 5.5 LLM 提示词设计与成本控制
- 语法：系统提示固定英语检查、中文讲解、封闭枚举和 NDJSON 顺序；翻译仍输出纯文本。
- 默认模型选最便宜档（gpt-4o-mini / deepseek-chat / haiku），`maxTokens` 上限沿用 Provider 配置，缺省 1024。
- 相同输入的**内存 LRU 缓存**（键含输入、模型、Base URL、提示词版本）；请求级超时（语法 20s / 翻译 8s）。连接池共享，超时不绑在 Client 上。
- AI 复习复用同一 LLM Provider，单独 prompt，只读历史错误聚合高频类型（M3）。

### 5.6 商业化（SaaS）预留边界
1. **Transport 抽象**：Rust 侧「出网」封装 `Transport` trait，v1 是 `LocalTransport`（reqwest + keys.json）；未来加 `RemoteTransport`，UI 零改动。
2. **Schema 预留**：`mistakes` 表 `server_id / synced_at`；provider 配置预留 `remote_profile`。
3. **契约文档先行**：[`docs/api-contract.md`](api-contract.md) 同时是未来后端 OpenAPI 初稿。

---

## 6. 数据层

按数据类型三分存放，对齐 Raycast 等本地优先桌面应用：**不要**把配置、历史、密钥塞进同一个明文库或同一份 JSON。

| 类型 | 现在（v1 / 开发期） | 目标（M5 签名公证后，可选升级） | 明确不放 |
|---|---|---|---|
| 非密钥配置 | `settings.json`（热键、主题、默认引擎、窗口位置） | 不变 | API Key、历史、错题 |
| 用户内容 | SQLite（`history` / `mistakes`） | 不变 | API Key |
| 密钥 | `keys.json` 明文（条目少、可手改、不弹密码） | 加密存储；解锁密钥进 OS Keychain | WebView、历史表、settings.json |

条目少（两三个 Provider Key）不值得单独做业务表。SQLite 的价值在查询与体量；把 Key 写成历史库里的明文列，并不比 `keys.json` 更安全。

### 6.1 SQLite（rusqlite，`PRAGMA user_version` 版本化迁移）

```sql
CREATE TABLE mistakes (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  source_text TEXT NOT NULL,            -- 触发检查的原句
  fragment TEXT NOT NULL,               -- 出错片段
  correction TEXT NOT NULL,             -- 建议修改
  error_type TEXT,                      -- subject-verb-agreement / tense / article / spelling ...
  severity TEXT DEFAULT 'medium',       -- low | medium | high
  explanation TEXT,                     -- 中文讲解
  corrected_sentence TEXT,              -- 整句改后（可空）
  suggestions TEXT,                     -- JSON Array<string>
  engine TEXT,                          -- 如 llm:deepseek
  source_app TEXT,
  tags TEXT,                            -- 逗号分隔（v1 简版标签）
  status TEXT DEFAULT 'open',           -- open | learned | archived
  meta TEXT,                            -- JSON 保留字段
  server_id TEXT,                       -- 【预留云同步】
  synced_at TEXT
);
CREATE INDEX idx_mistakes_created ON mistakes(created_at);
CREATE INDEX idx_mistakes_type    ON mistakes(error_type);

CREATE TABLE history (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  kind TEXT NOT NULL,                   -- translate | grammar | ai_review
  input TEXT, output TEXT,
  engine TEXT, from_lang TEXT, to_lang TEXT,
  duration_ms INTEGER, source_app TEXT, meta TEXT
);
CREATE INDEX idx_history_created ON history(created_at);
```

### 6.2 密钥存储

**现在（已落地）**  
API Key 放应用数据目录的 `keys.json`（tauri-plugin-store），与 `settings.json` 分开。路径示例：macOS `~/Library/Application Support/com.jiti.app/keys.json`。不走 Keychain / Credential Manager，避免未签名开发构建每次启动或读写都弹系统密码。WebView 只见存在性，拿不到明文。文件明文落盘是为省掉授权打扰的取舍。

**目标（M5 签名公证之后，后置可选）**  
向 Raycast 看齐，而不是继续把 Key 当普通配置：

1. 应用 **Developer ID 签名 + 公证** 后，本进程读写 Keychain 通常静默通过（Raycast 不弹密码的主因是签名，不是「写进了数据库」）。
2. 密钥从 `keys.json` **迁移**到加密存储：加密 SQLite / 加密文件，解锁密钥只进 Keychain 一次；或直接用 Keychain 存各 Provider Key。
3. 迁移一次性、可回退；IPC 仍只返回 `hasKey`，WebView 契约不变。
4. **禁止**把 API Key 写入 `history` / `mistakes` 表。

未签名阶段维持 `keys.json`，不为「看起来更像数据库」而提前混进明文 SQLite。

### 6.3 导出
错题本导出 **Markdown**（按错误类型分组，例句/修改/讲解）。文件位置用系统对话框选；Rust 写盘后返回路径。

---

## 7. 设置与权限

### 7.1 设置项清单

| 分组 | 项 |
|---|---|
| 快捷键 | 全局热键（翻译 / 语法 / 统一面板）、是否激活焦点、冲突检测 |
| 引擎 | 各 Provider Key 录入与状态、默认引擎、fallback 顺序、测试连接 |
| 语言 | 翻译方向（中英）、语法目标语言（英语）、界面语言（zh/en 跟随系统或手动） |
| 通用 | 开机自启、弹窗位置（跟随鼠标/居中/记住）、主题（浅/深/系统）、是否写历史 |
| 错题本 | 是否自动收录、默认 status、导出位置 |
| 权限 | macOS 无障碍 / Tauri 权限状态卡 + 一键跳系统设置 |

### 7.2 权限矩阵

| 能力 | macOS | Windows |
|---|---|---|
| 全局热键 | Carbon `RegisterEventHotKey`（Tauri global-shortcut 底层）；不监听按键，**不索要输入监控** | `RegisterHotKey`（Win32） |
| 读取选中文本 | 辅助功能权限（AX） | UI Automation（无用户授权闸门） |
| 剪贴板兜底 | 同辅助功能（CGEvent 模拟 Cmd+C） | 无需 |
| 开机自启 | SMAppService（App 需在 Applications） | 注册表 Run 键 |
| 密钥 | v1：`keys.json` 明文；签名后可升级加密库 / Keychain（§6.2） | 同左 |

**首次引导：** 冷启后 WebView 就绪即查 `permissions_snapshot`。macOS 且辅助功能未开、且用户未跳过 → 自动显示面板，内容换成全屏引导页（不是 DOM 遮罩对话框）。「去开启」调用 `AXIsProcessTrustedWithOptions(prompt)` 并跳到系统设置辅助功能页；引导页可见期间轮询状态。可「稍后再说」（仍可手动输入翻译）；跳过后不再自动弹出全屏引导，设置页状态卡 + 底栏告警继续提醒。Windows 无必需权限，跳过引导。

> macOS 辅助功能授权后偶需重启才真正生效，引导页提供「重启应用」。

### 7.3 快捷键配置
- 默认两套（均可改）：macOS `⌥⌘T` 翻译 / `⌥⌘G` 语法 / `⌥⌘Space` 统一面板（上次模式）；Windows `Ctrl+Shift+T` 翻译 / `Ctrl+Alt+G` 语法 / `Ctrl+Alt+Space` 统一面板。
- 设置页点击组合键后按下新快捷键即可重配；变更即 Unregister+Register（tauri-plugin-global-shortcut），与本应用其它热键冲突或系统占用时弹提示。
- 面板内 `Tab` 切模式、`Esc` 收起、`⌘Enter` 手动触发、`⌘C` 复制结果。**`Esc` 永远有明确含义**（原生约定）。

---

## 8. 前端 UI（Vue 3）设计

> 依据 `design-taste-frontend` 技能做「先读题、后设计」，再叠加 native-feel 技能 T3（采用平台）与 `references/06-native-conventions.md`（原生约定审计）。

### 8.1 Design Read（设计判断，先声明）

**"Reading this as：给键盘优先、中英双语用户使用的系统级小工具浮层，native-feel 材质语言，倾向平台原生 + 克制的现代工具风，不模仿任何模板。"**

三个拨盘（对应技能 §1）：
- `DESIGN_VARIANCE: 4`（工具型、低花活；对称稳定为主）
- `MOTION_INTENSITY: 3`（只做有动机的动效：弹窗显隐、流式文本、`:active` 按压）
- `VISUAL_DENSITY: 5`（紧凑但透气；错误卡片信息密度是核心）

适用性声明：该技能面向营销/落地页，本项目是**产品 UI**，故只取可迁移部分（排版纪律、色彩校准、形态一致性、状态完整性、动效动机、文案自审、图标政策、明暗主题），不套用落地页组件。

### 8.2 设计系统：不引第三方 UI 库，用「平台 token + 自研最小组件集」
小工具 UI 不需要 Fluent/Carbon 这类企业系统；也不需要落地页组件库。做一个**一套系统、两个 OS 各取平台语义**的最小 token 集：

**字体（T3：采用平台）**
- UI 全部用**系统字体栈**，不打包 web font：`system-ui, -apple-system, "Segoe UI Variable", "PingFang SC", "Microsoft YaHei", sans-serif`。西文数字/引擎数据用等宽：`ui-monospace, "SF Mono", "Cascadia Mono", monospace`。
- 全项目**不用衬线**（产品工具 UI 默认无衬线；中英混排下衬线必割裂）。显示层用系统字体的 Medium/Semibold 而非大字号堆砌（用字重+颜色控制层级，不用疯狂字号）。
- 不引入 Fraunces / Instrument 这类 AI 默认衬线（设计纪律，项目内零使用）。

**色彩（一个强调色，锁死；语义色独立）**
- 中性基调：浅 `#f4f6f8` / 深 `#0f141a` 类蓝灰（有向色温，不用纯灰、纯黑/纯白）。
- 唯一强调色 **amber `#e8910f`（浅）/ `#f2ae4b`（深）**，饱和度 <80%，用于选中态、当前 Tab、主操作、进度。全应用只用这一个强调色（色彩一致性锁）。
- **严重度语义色**（独立于强调色，仅作状态）：high `#d94f45` / medium `#d97706` / low `#0f9d6a` 系，用于错误卡片色条与状态点。
- 全部色值过 WCAG AA（正文 4.5:1，大字 3:1）。深/浅两套 token 从第一版同时设计。

**形态（一个圆角体系）**
- 面板本体圆角交给 **OS**（原生约定：mac 由系统窗口圆角，Tauri 里设 `shadow:true`、不自定义 `border-radius` 与 `box-shadow` 给窗口；CSS 只处理内容组件）。
- 组件圆角规则唯一：输入框 8px、按钮 8px、错误卡片 10px、chip/标签 pill。**一个体系，全程遵守**。

### 8.3 布局（Raycast 风，但收敛）

```
┌──────────────────────────────────────────────┐  ← 平台窗口（OS 圆角/阴影/材质）
│ [ 搜索/输入框  选中自动填入 ]                     │  ← 聚焦即全选，可直接替换
│ [ 翻译 ] [ 语法 ] [ 错题本 ] [ 历史 ]           │  ← Tab 切换；设置走独立窗口（⌘, / Ctrl-,）
│ ┌──────────────────────────────────────────┐ │
│ │ 翻译: 原文 / 译文 [+复制]                 │ │
│ │ 语法: 错误卡片(类型/严重度/修改/中文讲解)  │ │
│ │ 错题本: 过滤chip × 列表 × [AI 总结]       │ │
│ │ 历史: 列表 + 清空                         │ │
│ └──────────────────────────────────────────┘ │
│ status: 引擎 · 耗时 · Provider · 权限告警     │
└──────────────────────────────────────────────┘
```

- 面板默认 ~640×440，可调；随内容自适应高度（**WebView 恒最大尺寸，只改窗口高度**，§4.6 A.3）。
- 错误卡片：`严重度色条 + 原文高亮 + 修改 + 类型标签 + 中文讲解 + [收录错题]`。卡片仅在有层级需要时用（信息分组用 `divide-y`/留白，不过度起框）。
- 空态、加载、错误三态从设计第一版就做（见 §8.5）。

### 8.4 原生约定审计（每一条都是"网页味"的消灭点）

（native-feel `references/06-native-conventions.md` 截取与本应用强相关项）

- 可点击行/按钮 **不改变鼠标指针**（`cursor: default`），仅输入框用 I-beam
- chrome 文案与标签 **默认不可选中**（`user-select:none`），仅内容/输入区可选
- **无浏览器右键菜单**（mac 覆写 `willOpenMenu`，win 拦 `ContextMenuRequested`）；force-touch 不触发字典查询
- 输入框之外**不做拼写红波浪线**
- **中文输入法候选框跟随光标**（IME 正确性，Pinyin 重点测，§4.6 B.5）
- 键盘全可达：Tab/方向键到每个动作；焦点环用平台样式；**Esc 永远有明确行为**；列表支持打字跳转（type-ahead）
- 设置是**独立原生窗口**（⌘, / Ctrl-,），不是主窗内 modal；确认类对话用系统原生 `NSAlert`/`MessageBox`，**不用 DOM 遮罩层**
- 明暗随系统、切换无闪帧；系统强调色为可选增强（可先不用，保持品牌单一强调色）
- 窗口尺寸/位置跨启动记忆；多屏在激活屏打开

### 8.5 状态设计（加载 / 空 / 错误，完整闭环）

- **加载**：<200ms 什么都不显示；200ms-2s 显示 spinner；>2s 显示进度。**LLM 语法流式本身就是进度**（打字机文本），不套骨架屏（native 约定：快速操作用骨架屏反而显得慢）。
- **空态**：极简（图标 + 一行），例如错题本空 = "还没有错题，检查一次就有了"。
- **错误**：内联展示于卡片/输入区；引擎错误走 status bar 统一 toast 式（`engine://error`），可复制错误码。toast 是**瞬时**语义，持续性状态用内联。
- **按压反馈**：`:active` 做轻微下沉/缩放（`translateY(1px)` / `scale(0.98)`），原生按钮有按压态，网页只有 hover 是 tell。
- **文案自审**：UI 字符串零 em-dash；面向学习者讲解用具体、可执行的语气，不用 AI 式空话；不伪造精确数字。

### 8.6 动效（有动机才动，全部可被 reduced-motion 关闭）

- 允许：弹窗显隐（层级反馈）、流式文本（状态转换）、Tab 切换（状态转换，切不淡入）、`:active` 按压。
- 禁止：无限循环装饰、spring/bounce、DOM 路由淡入淡出（native 是"切"）。
- 所有动画只动 `transform` / `opacity`；尊重 `prefers-reduced-motion`。

### 8.7 图标与资源政策

- 图标：**`@tabler/icons-vue`**（全局统一 `stroke-width=1.75`），**一个图标家族贯穿**，不手绘 SVG，不用 emoji 当图标。设置里允许极少量 emoji 作为"配图"需再评审（默认禁）。
- 字体不打包（系统字体，§8.2）；无远程图片/资源（CSP `self`）。
- 无 div 拼出来的"假截图"类装饰（本应用就是产品本身，不需要）。

### 8.8 i18n
- `vue-i18n`，`zh-CN` 默认 / `en-US`；设置项「跟随系统 / 简体中文 / English」。
- 「界面语言」与「翻译语种」分开；语法讲解语气固定为面向中文英语学习者（中文讲解），不随界面语言变（产品定位）。
- 设置是独立窗口（`packages/ui/src/settings/`，⌘, / Ctrl-, / 托盘），关闭即销毁。IME 专项 QA 见 [`docs/m4-ime-qa.md`](m4-ime-qa.md)。

---

## 9. 安全与能力最小化

- CSP 仅 `self`；无远程资源加载。
- Tauri 2 capability 白名单最小集（global-shortcut / autostart / store / sql / window），不开 shell、不开远程 http 域。
- 密钥不进 WebView（D2）。
- 不做设置导入导出（避免带走 Key）；「测试连接」只返回 ok/fail。
- LLM baseUrl 限 http/https。

---

## 10. 工程结构（Monorepo 预留）

```
jiti/
├─ packages/
│  ├─ ui/                  # Vue 3 + Vite + TS 弹窗前端
│  │  ├─ src/panel/        #  主面板入口（预热常驻）
│  │  ├─ src/settings/     #  设置独立入口（用后即拆，独立 bundle）
│  │  ├─ src/i18n/         #  zh-CN / en-US 界面文案
│  │  ├─ src/ipc/bindings.ts  #  ← tauri-specta 生成，勿手改
│  │  └─ tests/            #  Vitest
│  └─ native/              # = src-tauri（Rust）
│     ├─ src/commands/     #  IPC 命令层（薄，tauri-specta 标注）
│     ├─ src/services/     #  selection / panel(含 objc2 原生缝) / secrets / engine_http / store
│     ├─ src/providers/    #  llm / deepl / youdao / google / languagetool
│     └─ capabilities/     #  权限白名单
├─ docs/
│  ├─ architecture.md      # 本文档
│  └─ api-contract.md      # IPC 与引擎契约（未来 OpenAPI 初稿）
└─ .github/workflows/      # tauri-action 双端打包
```

关键工程纪律：
- `src/ipc/bindings.ts` 由构建钩子从 Rust 生成，**不手写、不进 code review 的 diff**。
- 主面板与设置是**两个 HTML 入口 / 两套 bundle**（native-feel 多入口原则：面板不背设置依赖，设置拆得快、冷启可接受）。

---

## 11. 构建、分发与签名

| 平台 | 产物 | 注意 |
|---|---|---|
| macOS | `.dmg` / `.app` | 公开分发需 Apple Developer 签名 + 公证（$99/年）；v1 自用可不签 |
| Windows | `.msi` / `.nsis` | 建议代码签名；WebView2 缺失时引导装 bootstrapper |
| CI | GitHub Actions + tauri-action | macOS 需 macOS runner，双端矩阵 |

- `tauri.conf.json` 统一管透明 / 无边框 / alwaysOnTop / 初始隐藏 / macosPrivateApi。
- v1 不做自动更新；二期可接 `tauri-plugin-updater`（发布前把「升级是真实流程」列入，而不是"请手动下载新版"）。
- 崩溃上报接 Sentry（`tauri-plugin-sentry`），发布版才启用。

---

## 12. 测试策略

| 层级 | 工具 | 覆盖 |
|---|---|---|
| TS 核心 | Vitest | normalize、provider 请求构建、设置校验、错误分类 |
| Rust | cargo test + mockito | provider 适配器（mock HTTP）、SSE 解析、SQL 迁移 |
| IPC 契约 | 构建期 tauri-specta 生成 + TS 类型编译 | 类型同步（改 Rust 后 TS 不通过=红） |
| E2E（增值） | tauri-driver / WebdriverIO | 弹窗开合、热键→面板、假引擎翻译/语法流程 |
| 手工 QA | 真机双端 | 权限引导、快捷键冲突、**Pinyin 输入法**（[`docs/m4-ime-qa.md`](m4-ime-qa.md)）、断网降级、**隐藏窗节流（§4.6 A.1）** |

**发布门禁**：M5 收尾时跑一遍 native-feel `checklists/ship-readiness.md`（70 项审计，A-G 段不允许红，H/I 可少量留）。详见附录 A。

---

## 13. 性能目标（基线 / 感知，分开算）

（native-feel `references/05-memory-truths.md` 修正了 v0.1 的乐观目标）

**诚实基线（跳过 Node 后，Tauri + 系统 WebView 的真实地板）：**

| 组件 | 常驻（暖） | 性质 |
|---|---|---|
| Rust 壳 + Tauri | ~20-40 MB | 基线，租来的 |
| WKWebView（单窗空页） | ~50 MB | 基线，租来的 |
| WebView2（单窗空页） | ~80-120 MB | 基线，租来的（比 WebKit 重） |

**因此 idle 目标：macOS ~80-120 MB，Windows ~130-180 MB。** v0.1 写的 40-80MB 目标不成立，如实修正：这部分是 WebView 基线，**不接受"降到 50MB"的预期**（T8：别跟租来的成本较劲，把火力打在自有边际成本上）。

**感知目标（T4：用户体验的是承诺，不是 MB）：**

| 感知点 | 目标 |
|---|---|
| 热键 → 弹窗显示（暖） | <30ms（预创建 + hide/show，禁按热键才建窗） |
| 冷启 → 可交互 | <600ms（macOS）/ <900ms（Win） |
| 翻译首字感知 | <1s（非 LLM）；LLM 流式首 chunk <1.2s |
| 语法检查 | 首条 NDJSON 语义记录即更新界面；20s 硬超时 |
| 后台 CPU | 隐藏时 <0.5%（节流配置见 §4.6） |

**自有边际成本的优化顺序（先于任何"降内存"讨论）：**
1. 设置窗口用后即拆（独立环境/独立 bundle，§10）
2. bundle 按入口拆（面板不背设置依赖）
3. 代码分包 / 不进 sourcemap 到生产（`*.map` 巨大）
4. 泄漏排查：脏页热点优先（React 全局 store 里的大数组、无淘汰的缓存）

**度量纪律（Activity Monitor 会骗人）：**
- 只看**内存压力**（绿=没问题），不看单个进程的数字；共享框架被每个进程重复计入、压缩页被按未压缩计入、clean 页在压力下成本为零。
- 要量就 `vmmap --summary <PID>` 看 dirty 页；趋势看分钟级，不看瞬时快照。

---

## 14. 风险清单（P0 / P1 排序）

1. **P0 · macOS「不抢焦点」**：Tauri 窗口非 NSPanel，需 objc2 补（§4.2）；可降级「激活才显示」+ 开关，但这是体验核心。
2. **P0 · 隐藏窗被 WebKit 节流**：预热弹窗若被判定不可见，首次显示卡顿（§4.6 A.1）。三件套（occlusion off + alpha 预热 + rAF 保活）必须进 M0。
3. **P0 · 选中读取的权限与失败链**：AX 授权后需重启；Windows UIA 覆盖不全，剪贴板兜底必须通。
4. **P1 · CORS**：所有引擎 HTTP 在 Rust（D1），WebView 内绝不对引擎域名 fetch。
5. **P1 · LLM 不稳定输出**：JSON 解析失败重试/降级；错误统一分类（密钥缺失 / 限流 / 网络）。
6. **P1 · 中文输入法（IME）**：WebView2/WKWebView 的 Pinyin 候选框问题（§4.6 B.5），双语应用必测。
7. **P1 · 首帧白闪**：mac `_doAfterNextPresentationUpdate` / Win `NavigationCompleted` 后才 show（§4.6 A.2 / B.1）。
8. **P1 · 密钥明文落盘**：v1 用 `keys.json` 换取不弹密码；M5 签名公证后可升级加密库 / Keychain（§6.2）。WebView 始终拿不到明文。Key 不得写入历史 SQLite。
9. **P2 · Windows 透明/圆角受限**：v1 稳妥视觉，Mica 增值。
10. **P2 · macOS 自启**：SMAppService 要求 App 在 /Applications。
11. **P2 · 国内网络可达性**：Provider 排序要贴合用户网络（有道/DeepSeek）。
12. **P2 · 内存感知**：WebView2 基线 100MB+，发布文案别承诺"50MB 级"（§13）。

---

## 15. 里程碑计划

| 里程碑 | 内容 | 阶段产物 |
|---|---|---|
| **M0 骨架** | tauri 初始化、Vue3 无边框隐藏窗、热键注册/显示/隐藏、**WebView 存活三件套（§4.6 A.1）**、首帧同步显示（A.2）、设置 store | 热键秒开的灰壳（不闪、不卡） |
| **M1 翻译** | translate 命令（DeepL + 有道 + LLM）、选中捕获链（AX/UIA + 剪贴板兜底）、翻译 Tab、历史入库；**密钥三分：Key 进 `keys.json`，不进 Keychain、不进历史库**（§6） | 选中即译可用，保存 Key 不再弹系统密码 |
| **M2 语法** | LLM NDJSON 流式 + 一次结构重试、错误卡片、grammar 历史；LanguageTool 仅留分支 | 语法检查可用 |
| **M3 错题本** | CRUD + 过滤 + 导出 Markdown + AI 总结；语法自动/手动收录 | 错题本能用 |
| **M4 设置完善** | 自启、i18n、主题、独立设置窗口、IME 专项 QA（快捷键重配已落地） | 可交付内测（已接通） |
| **M5 打磨 + 门禁** | 流式优化、原生约定审计（§8.4 全过）、**ship-readiness 70 项审计（§12）**、**签名公证双端打包**（签名后 Keychain 可静默访问，为密钥升级铺路） | 可对外分发 |
| **后置** | **密钥升级（可选，§6.2）**：`keys.json` → 加密库或 Keychain，一次性迁移、IPC 不变、Key 永不进历史表；Remote transport（SaaS 预留）、云同步、内联纠错评估、液态玻璃深度定制评估 | 无 |

---

## 附录 A · Ship-Readiness 关键项（从 70 项中截取本项目红线）

（完整清单见技能 `checklists/ship-readiness.md`，M5 全量执行。红线摘录：）

- 热键→可见窗口 暖 <200ms / 冷 <600ms；无白黑闪（A.2/B.1）
- 弹窗出现在激活屏、记住位置；初始焦点在输入框、可直接打字
- ⌘W 关窗 / ⌘M 最小化 / 绿点缩放；点击外部按配置隐藏（可预测）
- 设置是独立窗口（⌘, / Ctrl-,）；无 DOM 遮罩"对话框"
- 无 `cursor:pointer`（行/按钮/Tab）；chrome 不可选中文本
- 原生右键菜单（或彻底移除），非 WebKit 的
- Pinyin IME 候选框跟随光标
- 全键盘可达 + 平台焦点环；Esc 永远有意义；列表 type-ahead
- 平台材质（vibrancy/mica）+ 系统深色模式 + 系统字体；窗口圆角/阴影交给 OS
- 隐藏窗不被节流（定时更新在 re-show 后立刻触发）
- 单实例（Win 二次启动聚焦已有实例）
- 网络超时 10s 内用户可见错误，不是 60s
- >200ms 有加载态，<200ms 什么都不显示
- 无僵尸进程；退出即退出

---

## 16. 参考资料（选型依据）

**框架与平台**
- Tauri 官网（v2.0，最小 ≈600KB，原生 WebView）：https://tauri.app/
- Tauri 全局快捷键插件 ：https://tauri.app/plugin/global-shortcut/
- Tauri 2.9.3 文档（窗口/焦点/透明 macos-private-api）：https://docs.rs/tauri/2.9.3/tauri/
- tauri-specta（Rust 单源 → TS 绑定生成）：https://crates.io/crates/tauri-specta
- Wails（Go 版轻量方案，备选）：https://github.com/wailsapp/wails
- WebView2 运行时分发（Win11 预装 / Win10 多数已装 / bootstrapper 2MB）：https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution
- NSPanel（nonactivatingPanel / orderFrontRegardless / becomesKeyOnlyIfNeeded）：https://developer.apple.com/documentation/appkit/nspanel
- Swift 平台支持 ：https://www.swift.org/platform-support/
- UniFFI（Rust ↔ Swift/C# 绑定生成，预留演进路径）：https://github.com/mozilla/uniffi-rs
- tauri-plugin-store（settings.json / keys.json）：https://v2.tauri.app/plugin/store/

**引擎**
- LanguageTool（开源语法检查、可自托管）：https://dev.languagetool.org/
- Raycast 开发者平台（React/TS 扩展体系参考）：https://developers.raycast.com/
- Raycast 安全说明（加密本地库 + Keychain；password 偏好不进普通 JSON）：https://developers.raycast.com/information/security

**设计 / 原生手感**
- native-feel-cross-platform-desktop 技能（本架构 v0.2 的审计来源）：`references/01-philosophy.md` · `02-architecture.md` · `03-webview-survival.md` · `04-ipc-contract.md` · `05-memory-truths.md` · `06-native-conventions.md` · `checklists/decision-tree.md` · `checklists/ship-readiness.md`
- design-taste-frontend 技能（本架构 §8 UI 规范的来源）：排版纪律 / 色彩校准 / 形态一致 / 状态完整 / 动效动机 / 文案自审
- Apple Human Interface Guidelines → Materials（Liquid Glass）：https://developer.apple.com/design/human-interface-guidelines/materials

---

*本文档随需求变化维护。弹窗位置（跟随鼠标/居中/记住）与 LanguageTool 适配器仍后置，不在 M4 范围。*