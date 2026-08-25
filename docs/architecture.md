# Jiti — 快速翻译 / 语法检查桌面工具 · 技术架构设计

> 工作代号：**Jiti**（可随时改名）
> 状态：v0.1 草案 · 2026-08-25 · 面向 macOS（10.13+）与 Windows 10/11
> 目标形态：Raycast 风格的小弹窗，全局快捷键唤起，常驻后台，秒级显示

---

## 1. 产品定位与需求

### 1.1 一句话定位
一个由全局快捷键唤醒的系统级浮层工具：自动读取你当前**选中的文字**，按需执行**中英翻译**或**英语语法检查**，错题自动归档成**错题本**，可 AI 总结复习。

### 1.2 确认后的需求清单（对应 v1）

| # | 需求 | 说明 |
|---|---|---|
| 1 | 快速翻译 | 中↔英（双向、自动判向），选中即弹，可手动输入 |
| 2 | 语法检查 & 改错 | 针对英语句子：找错、给修改、讲解；输出一张张错误卡片 |
| 3 | 错题本 | 自动收录每次语法错误；按类型/时间过滤；导出 Markdown |
| 4 | AI 复习 | 一键总结高频错误、生成复习要点 |
| 5 | 历史记录 | 最近查询历史，可翻看 |
| 6 | 设置 | 快捷键配置、开机自启、多语言设定、Provider（引擎）管理与测试 |
| 7 | 交互 | 选中即弹 + 手动输入双模式；`Tab` 切换功能 Tab；不同快捷键直达对应模式 |
| 8 | 引擎 | 可配置云端大模型（翻译/语法），以及 DeepL / 有道 / Google 等翻译 API（可插拔） |
| 9 | 服务模式 | v1 用户自带 Key、纯本地；**架构预留**未来的后端 / SaaS 切换 |

### 1.3 非目标（v1 明确不做，避免范围失控）
- ❌ **Grammarly 式内联纠错**（在其它 App 输入框里实时划红线）。需要输入法/文本注入级别的系统能力，复杂性高一个量级，放到二期评估。
- ❌ 多端云同步错题本（Schema 预留字段，不实现）。
- ❌ 自有后端 / 账号 / 计费（仅预留边界）。

---

## 2. 技术选型总览

### 2.1 选型表

| 层 | 选型 | 理由 |
|---|---|---|
| 应用壳（原生） | **Tauri 2**（Rust） | 一套代码出 macOS + Windows；不捆绑 Chromium；官方宣传最小≈600KB（实际工程通常几 MB）；官方插件覆盖全局快捷键 / 自启 / SQL / 托盘 |
| 渲染引擎 | macOS **WKWebView** / Windows **WebView2** | 系统自带、无捆绑浏览器、内存与启动接近原生 |
| 前端框架 | **Vue 3 + Vite + TypeScript** | 你熟悉；轻量；生态成熟 |
| 后端语言 | **Rust** | Tauri 原生层；做热键、窗口、选中读取、密钥托管、网络代理 |
| 数据库 | **SQLite**（`tauri-plugin-sql`） | 错题本 / 历史 本地持久化 |
| 密钥存储 | OS 级 Keychain / DPAPI（`keyring` crate） | API Key 不进 WebView、不进明文配置 |
| 引擎 | **可插拔 Provider 层** | LLM（OpenAI 兼容协议）：Claude / OpenAI / DeepSeek / 自定义；翻译 API：DeepL / 有道 / Google / Baidu 等 |

### 2.2 为什么「不用 Electron」是对的决定，以及 Tauri 替代了什么
Electron 的体量主要来自捆绑一套 Chromium 核心（安装包 100MB+、内存数百 MB）。Tauri 的方案是「**薄原生壳 + 系统 WebView**」：

- macOS 直接用 Apple 的 **WKWebView**（系统级内核，与 Safari 同源）；
- Windows 用微软的 **WebView2**（Edge 内核）。WebView2 Evergreen 运行时 **Win11 系统内置**，Win10 绝大多数设备已通过 Windows Update 装上，极少数缺失时可引导安装 2MB 的 bootstrapper。
- 前端照样写 Vue/React，业务能力由 Rust 补足，几乎不损失任何你想要的「原生底子」。

### 2.3 你之前在 macOS 上想用 Swift —— 现在还需要吗？
不需要。Tauri 2 的 macOS 底就是 Rust，但两条口子都是开放的：

1. Tauri 允许在 Rust 里通过 `objc2` 直接调用 AppKit / CoreGraphics 等原生 API（比如我们做「不抢焦点的面板」「读取选中文本」都靠它）；
2. 若将来某一天你想把 macOS 版改写成**纯 Swift + SwiftUI + WKWebView**，完全可以：本架构把「UI + 业务逻辑」打包成一套**与壳无关**的 Web 资源，Swift 壳只是另一个薄容器（Swift 里包一个 `WKWebView` 加载同一份 bundle 即可）。所以 UI/核心层的干净边界本身就是对「将来用 Swift 重写 macOS 壳」的预留。

### 2.4 前端 Vue 3 的形态
面板是**一个无边框、自绘 UI 的窗口**（非浏览器页面网站），Vue 负责：弹窗整体、模式 Tab、翻译/语法结果渲染、错题本列表与管理、设置表单、快捷键/权限引导。状态用轻量 `Pinia`，样式用 CSS 变量做明暗主题。

---

## 3. 总体架构

### 3.1 架构图

```
┌───────────────────────────────────────────────────────────────┐
│                    Vue 3 + Vite + TS（弹窗 UI）                  │
│  翻译 Tab  ·  语法 Tab  ·  错题本 Tab  ·  历史 Tab  ·  设置 Tab   │
│  core/（纯 TS 业务：store、normalize、provider 契约、i18n）      │
└──────────────┬─────────────────────────┬──────────────────────┘
        invoke()（命令）          事件 event（热键→UI、进度流）
┌──────────────▼─────────────────────────▼──────────────────────┐
│                     Rust（Tauri 2 原生层）                      │
│  commands: translate / grammar_check / mistakes / history /    │
│            settings / hotkeys / autostart / export              │
│  services: selection(选中读取)  panel(窗口/焦点)  keyring        │
│            engine_http(统一出网)  store(SQLite/JSON)             │
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
| TS `core` | 纯函数：结果归一化、Provider 请求构建、设置校验、i18n 文案 | 不碰文件系统/密钥 |
| Rust 原生 | 热键、窗口显示/定位/焦点、选中文本读取、剪贴板、密钥托管、SQLite、**所有出网 HTTP**、进程/单例/自启 | 不放 UI 逻辑 |
| 引擎 Provider | 具体 API 适配 | 互相不通 |

### 3.3 三个关键架构决策

**D1 · 业务编排放 TS，出网请求走 Rust。**
最重要的一条。原因：WebView 里 `fetch` 会受 CORS 约束，而 DeepL / 自定义 LLM 端点 / 有道等未必允许任意 Origin。因此**所有引擎 HTTP 一律由 Rust `reqwest` 发出**（统一超时、重试、错误分类、LLM 流式 SSE）。Vue 只通过 `invoke('translate')` / `invoke('grammar_check')` 等语义命令调用。这同时让未来「本地 → 远程后端」的切换只替换 Rust 这一跳，UI 完全不动。

**D2 · API Key 与引擎配置永不出现在 WebView。**
Key 存 OS Keychain（macOS Keychain / Windows Credential Manager，用 `keyring` crate）。Rust 命令内部根据 Provider 配置 + Key 拼请求；WebView 只能拿到「哪个 Provider 已配置 / 是否可用 / 测试结果」，拿不到密钥明文。将来切 SaaS，Vue 侧连「Key 已配置」都不需要关心——它只关心「调用哪个 transport」。

**D3 · 原生壳保持「薄」：UI 与业务核心是可复用资产。**
所有界面与核心逻辑打包进一个 Web bundle（嵌入 Tauri 资源）。原生壳只做「系统能力代理」。这一边界保证：重写任一平台壳、或迁移到纯 Swift / 纯 C#，成本都可控。

### 3.4 数据流示例

**翻译（点按热键 → 结果出现，目标 <1s 感知）**
```
热键触发 ─▶ Rust: 读选中文本(AX/剪贴板) ─▶ event 'hotkey://pressed{translate}'
Vue 收到 ─▶ 输入框填入选中文案 ─▶ invoke translate{text,to:'en',engine:default}
Rust: 取 keyring→DeepL key → reqwest POST → 归一化 → 写 history → 返回
Vue: 渲染译文 + 来源引擎 + 耗时；⌘C 复制 / Esc 隐藏
```

**语法检查（LLM 流式，给「打字机」体验）**
```
Vue ─▶ invoke grammar_check{text, engine:'llm', model:'...', on_progress: Channel}
Rust: 组装提示词 → reqwest SSE 流式 → 每一个 chunk 通过 Channel 推给 Vue
Vue: 骨架 + 流式文本 → 结束时 Rust 返回结构化 JSON(GrammarResult)
Vue: 渲染错误卡片；用户点「收录错题」──▶ invoke mistakes_save
```

### 3.5 IPC 面（Rust ↔ Vue 契约，v1 全量）

**invoke 命令**（`#[tauri::command]`）：

| 命令 | 说明 |
|---|---|
| `get_selected_text()` | 读取当前焦点应用选中文本（见 §4.5 捕获链） |
| `show_popup(mode?)` / `hide_popup()` | 显示/隐藏面板（常驻窗口，非重建） |
| `set_popup_anchor(anchor)` | 定位：跟随鼠标 / 屏幕中央 / 记住位置 |
| `translate(req) → TranslateResult` | 翻译（DeepL/有道/Google/LLM） |
| `grammar_check(req, on_progress) → GrammarResult` | LLM 流式语法检查（含 Languagetool 非流式分支） |
| `ai_review_mistakes(scope) → ReviewResult` | 错题本 AI 总结 |
| `mistakes_list(filter)` / `mistakes_save` / `mistakes_delete` / `mistakes_export` | 错题本 CRUD + 导出 |
| `history_list(page,limit)` / `history_clear` | 历史 |
| `settings_get()` / `settings_set(partial)` | 设置（白名单字段） |
| `test_provider(engine, cfg)` | 设置里「测试连接」 |
| `keyring_set/has/delete` | 密钥托管（WebView 只见 is_set） |
| `autostart_set/enabled/status` | 开机自启 |
| `open_privacy_settings()` | 引导用户去系统设置开无障碍/输入监控 |
| `save_hotkeys(cfg)` | 运行时重新注册全局热键 |

**事件（Rust → Vue）**：
- `hotkey://pressed` {mode} —— 热键命中，UI 切模式
- `capture://changed` {text?, sourceApp?, via} —— 选中文本已就绪
- `panel://visibility` {visible}
- `engine://error` {provider, code, message} —— 统一错误浮窗（密钥缺失/限流/网络）

---

## 4. 弹窗与唤醒（Raycast 式体验）

### 4.1 生命周期：常驻 + 预热，而不是「唤醒即启动」

「秒开」的唯一正解是**进程常驻、窗口预创建**：

- App 常驻后台：macOS `activationPolicy = .accessory`（无 Dock 图标，做菜单栏驻留）；Windows 隐藏主窗口 + 可选托盘图标。
- 启动即注册全局热键、创建**隐藏**的面板窗口、加载 Web bundle 并保持 WebView 存活。
- 热键触发：`window.set_position(...)` → `show()` → 需要输入时 `set_focus()`。**不销毁、不重建**，因此从热键到画面 <30ms。
- 关闭 = `hide()` 而非退出；退出走托盘菜单 / 设置里「完全退出」。
- 单例插件 `tauri-plugin-single-instance` 防止多实例竞态。

### 4.2 macOS「不抢焦点」方案（体验核心，单独立项打磨）

快速词典类工具最关键的一点：弹窗出现时**不应打断用户正在原应用里的输入**。系统原生做法是 `NSPanel` + `nonactivatingPanel` + `orderFrontRegardless` + `becomesKeyOnlyIfNeeded`（已确认这些 API 存在）。Tauri 的窗口是普通 `NSWindow`，需要绕一层：

1. `app.set_activation_policy(Accessory)` —— 应用不成为前台主应用；
2. 显示时**优先用不激活应用的方式**：越过 `activate()`，配合 objc2 调用 `NSApp` 级别的 `orderFrontRegardless`；
3. 焦点策略：弹窗默认不夺焦（`becomesKeyOnlyIfNeeded` 语义）；用户点进输入框（手动模式）时才 `set_focus()`；
4. 可选给弹窗加透明/毛玻璃（`macosPrivateApi: true` 支持透明窗口；毛玻璃 vibrancy 属增值项）。

> ⚠️ 若 v1 为保进度接受「显示逻辑/手动输入必须焦点」的简化（很多同类工具也会短暂激活），请在设置里留开关。**默认目标是不激活。**

### 4.3 Windows 表现差异
- Win11 支持 Mica/Acrylic 背景与圆角（需 `transparent: true` + 系统 backdrop API，WebView2 透明受组合限制，成熟度低于 macOS）。
- 焦点语义与 macOS 不同：Windows 上「不抢焦点显示」可以用 `WS_EX_NOACTIVATE` 等；默认策略是：**无手动输入需要时尽量避免激活，需要输入时正常激活**。托盘图标常驻是 Windows 的主流形态。

### 4.4 位置策略
- 默认**跟随鼠标**（读取光标坐标 → 换算到当前显示器工作区 → 窗口贴边出现在光标附近，而不是遮住光标）。
- 设置可切「屏幕中央 / 记住上次位置」。
- 光标坐标跨平台读取：Rust `mouse_position` crate。

### 4.5 「选中即弹」：读取当前选中文本的机制链

这是 `选中即弹` 的核心，按**降级链**实现（每个平台各写一个 capture 模块）：

**macOS**
1. **AX（首选）**：`objc2` 调 `AXUIElement` —— 取前台 App → 聚焦元素 → `kAXSelectedTextAttribute`。需用户授予**辅助功能（Accessibility）权限**。
2. **剪贴板模拟（兜底）**：`CGEvent` 模拟 `Cmd+C` → 等 120ms → 读 `NSPasteboard` → 立刻把剪贴板恢复为原内容（先存原值、事后回写）。
3. 都拿不到 → Vue 进入「手动输入」空态（不报错，友好引导）。

**Windows**
1. **UI Automation（首选）**：`windows` crate 取聚焦元素 `TextPattern2/ValuePattern`。可编辑类应用普遍支持，只读选区不一定。
2. **Ctrl+C 模拟（兜底）**：同上剪贴板恢复策略。
3. 空 → 手动输入态。

> 权限与安全的说明：读取**自己拿到的文本**（翻译/检查）是用户明确意图驱动的操作。AX 权限由用户在系统设置里一次性授予，应用内要做**权限状态卡 + 一键跳系统设置**。

---

## 5. 引擎与 Provider 层

### 5.1 Provider 注册表（设置里统一管理）

```
providers:
  llm:       { baseUrl, apiKey?(keyring), kind: 'openai'|'claude'|'deepseek'|'custom', model, temperature, maxTokens }
  deepl:     { apiKey?(keyring), formality }
  youdao:    { appKey?(keyring), appSecret?(keyring) }
  google:    { apiKey?(keyring) }
  languagetool: { mode: 'off'|'local'|'online', localPort? }
defaults:
  translate: deepl  (fallback: llm)
  grammar:   llm    (fallback: languagetool)
```

- 每个 Provider 一个适配器（Rust trait / enum match），实现「构建请求 → 发 reqwest → 解析 → 归一化」。
- 设置里提供「测试连接」按钮（`test_provider`），读 keyring 后真实打一次 API，只返回 ok/fail + 原因。
- 注意网络可达性：若用户在国内，**DeepL/Google 可能直连不顺，有道/DeepSeek/国内镜像更可用**；设置里给出可访问性标注。

### 5.2 翻译适配器差异点

| Provider | 认证 | 说明 |
|---|---|---|
| DeepL | `auth_key` header | 免费档有限额；自由档收费；质量好 |
| 有道智云 | appKey+appSecret 签名 | 国内直连最稳，有免费额度 |
| Google Cloud Translation | apiKey | 按量收费；国内需要可达网络 |
| Baidu | appKey/appSecret | 备选 |
| LLM | OpenAI 兼容协议 | 可做「高级翻译」，质量最高但慢且贵；仅作为 fallback/可选 |

统一进出参：`TranslateRequest{text, from?, to}` → `TranslateResult{output, detectedFrom, engine, durationMs}`。

### 5.3 语法检查适配器

- **LLM（主力，流式）**：按 §5.5 提示词返回结构化 JSON。能讲清「错在哪、为什么」，这是对比 Grammarly 的差异化价值（面向英语学习者）。
- **Languagetool（可选，离线）**：Java 本地服务（或在线免费 API），返回 rule-based 错误；归一化成同一 Schema。离线优先/隐私优先的用户可用。

### 5.4 结果标准化 Schema（TS 类型，两端共用）

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

- 提示词固定为「系统提示 + 用户原文」，强制只输出 JSON（给出精确 schema 与少量示例；解析失败自动重试 1 次并降级为宽松解析）。
- 默认模型选最便宜的档位（如 `gpt-4o-mini` / `deepseek-chat` / `claude-haiku`），`maxTokens` 上限（如 1024），按单句/段落粒度调用。
- 成本/性能优化：相同输入的**内存 LRU 缓存**（归一化后命中则不重复调）；设置里可配「老是调贵的模型」的提示；请求级别超时（语法 20s / 翻译 8s）。
- 错题本的 AI 总结复用同一 LLM Provider，单独 prompt（只读历史错误，聚合高频类型）。

### 5.6 商业化（SaaS）预留边界

v1 只交付「用户自带 Key」，但架构上划清边界，未来不动 UI/DB 即能切换：

1. **Transport 抽象**：Rust 侧把「出网」封装成 `Transport` trait，v1 实现 `LocalTransport`（reqwest+keyring）；未来加 `RemoteTransport`（请求你的后端），UI 代码零改动（UI 从来不知道 Key 在哪）。
2. **Schema 预留**：`mistakes` 表预留 `server_id` / `synced_at`；provider 配置预留 `remote_profile`（endpoint + token）字段。
3. **契约文档先行**：`docs/api-contract.md` 记录 `translate`/`grammar_check` 的进出参，既是 Rust 命令契约，也是将来后端 OpenAPI 的初稿。

---

## 6. 数据层

### 6.1 SQLite（`tauri-plugin-sql`，迁移用版本化 SQL）

```sql
-- 错题本
CREATE TABLE mistakes (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  source_text TEXT NOT NULL,            -- 触发检查的原句/原段
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

-- 历史
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
- 一律 `keyring` crate：macOS Keychain、Windows Credential Manager。
- 设置/配置（非密钥）放 `tauri-plugin-store` 的 `settings.json`（JSON，含热键、Provider 列表、默认引擎、主题、位置偏好）。

### 6.3 导出
- 错题本导出 **Markdown**：按错误类型分组、给出例句/修改/讲解，可直接贴进笔记软件做复习材料。
- 文件位置用系统对话框让用户选；导出用 Rust 写盘后返回路径。

---

## 7. 设置与权限

### 7.1 设置项清单

| 分组 | 项 |
|---|---|
| 快捷键 | 全局热键（翻译 / 语法 / 统一面板）、是否激活焦点、冲突检测 |
| 引擎 | 各 Provider 的 Key 录入与状态、默认引擎、fallback 顺序、测试连接 |
| 语言 | 翻译方向（中英）、语法目标语言（英语）、界面语言（zh/en）跟随系统或手动 |
| 通用 | 开机自启、弹窗位置（跟随鼠标/居中/记住）、主题（浅/深/系统）、是否写历史 |
| 错题本 | 是否自动收录、默认 status、导出位置 |
| 权限 | macOS 无障碍 / Tauri 权限状态卡 + 一键跳系统设置 |

### 7.2 权限矩阵

| 能力 | macOS | Windows |
|---|---|---|
| 全局热键 | Carbon `RegisterEventHotKey`（Tauri global-shortcut 底层）；个别新系统可能要求辅助功能/输入监控，需引导 | `RegisterHotKey`（Win32） |
| 读取选中文本 | 辅助功能权限（AX） | UI Automation（大多数可编辑控件支持） |
| 剪贴板兜底 | 无需特殊权限 | 无需 |
| 开机自启 | SMAppService（需 App 位于 Applications；开发期需注意） | 注册表 Run 键 |
| Keychain | 签名应用无感；未签名开发构建可能弹「访问钥匙串」 | Credential Manager |

> macOS 辅助功能授权后常需重启应用才能生效，设置页面要做提示。

### 7.3 快捷键配置
- 内置两套默认（均可改）：`⌥⌘T` 翻译 / `⌥⌘G` 语法 / `⌥⌘Space` 统一面板（切到上次模式）；Windows 用 `Ctrl+Alt+T/G/Space`。
- 变更即 Unregister+Register（`tauri-plugin-global-shortcut` 自带 `unregister`），冲突弹提示。
- 面板内 `Tab` 切换模式、`Esc` 收起、`⌘Enter` 手动触发、`⌘C` 复制结果。

---

## 8. 前端 UI（Vue 3）设计

### 8.1 布局（Raycast 风）

```
┌────────────────────────────────────────────┐  ← 无边框、圆角、主题色、可选毛玻璃
│  [ 搜索/输入框  placeholder: 输入或粘贴文本… ] │  有选中自动填充, 全选态
│  [ 翻译 ] [ 语法 ] [ 错题本 ] [ 历史 ] [ ⚙ ] │  Tab 切换; 支持直达模式的快捷键
│                                            │
│   ┌──────────────────────────────────────┐ │
│   │ 模式内容区:                            │ │
│   │  翻译: 原文 / 译文 [+复制][+听](二期)   │ │
│   │  语法: 错误卡片列表(类型/严重度/修改/讲解)│ │
│   │  错题本: 过滤chip × 错误列表 × [AI总结]  │ │
│   │  历史: 列表 + 清空                     │ │
│   └──────────────────────────────────────┘ │
│  status bar: 引擎·耗时·Provider·权限告警    │
└────────────────────────────────────────────┘
```

- 面板默认尺寸约 640×440，可拖拽改变；随内容自适应高度（`autoResize`）。
- 错误卡片语义：`严重度色条 + 原文高亮 + 修改 + 类型标签 + 中文讲解 + [收录错题]`。
- 从「透明都能看到»做一步到位：loading 态用骨架 + 流式打字机，避免 LLM 等待的空窗。

### 8.2 状态管理
- `Pinia` store：`panel`（模式/聚焦状态）、`engines`（provider 状态）、`translate` / `grammar`（当前结果）、`mistakes`（列表/过滤）、`history`、`settings`、`i18n`。
- 一次热键触发 = 一组 saga：capture → set input → invoke 对应命令 → 写 store → 渲染。

### 8.3 主题与平台差异
- CSS 变量定义浅/深双主题 + 跟随系统；圆角、阴影在 Web 层实现（不受壳限制）。
- 毛玻璃：macOS 用 WKWebView `backdrop-filter` + `macosPrivateApi` 透明窗口；Windows 因 WebView2 透明组合限制，先做「不透明 + 深色/浅色」的稳妥视觉，Win11 Mica 作为增值。**别让 Windows 视觉拖累 macOS 发布。**

### 8.4 i18n
- `vue-i18n` + `zh-CN`（默认）/`en-US`；设置项「界面语言」与「翻译语种」分开。

---

## 9. 安全与能力最小化

- **CSP**：仅允许 `self`；无远程资源加载（毛玻璃图标等全部本地）。
- **能力白名单**：Tauri 2 capability 只给 UI 真正需要的权限（global-shortcut / autostart / store / sql / window 最小集合），不开 shell、不开 http 远程域。
- 密钥不进 WebView（§3.3 D2）。
- 导出/导入设置不做（避免把 Key 带走）；「测试连接」只返回 ok/fail。
- LLM baseUrl 支持自定义但做协议限制（http/https），避免意外打到文件路径等。

---

## 10. 工程结构（Monorepo 预留）

```
jiti/
├─ packages/
│  ├─ ui/                  # Vue 3 + Vite + TS 弹窗前端
│  │  ├─ src/panel/        #  模式 Tab / 弹窗组件
│  │  ├─ src/core/         #  纯 TS：providers 契约、normalize、settings 校验
│  │  └─ tests/            #  Vitest
│  └─ native/              # = src-tauri（Rust）
│     ├─ src/commands/     #  IPC 命令层（薄）
│     ├─ src/services/     #  selection / panel / keyring / engine_http / store
│     ├─ src/providers/    #  llm / deepl / youdao / google / languagetool
│     └─ capabilities/     #  权限白名单
├─ docs/
│  ├─ architecture.md      # 本文档
│  └─ api-contract.md      # IPC 与引擎契约（未来 OpenAPI 初稿）
└─ .github/workflows/      # tauri-action 双端打包
```

> 起步可以更简单：单包 `src/`（Vue）+ `src-tauri/`（Rust），但保持 `src/core` 与 `src-tauri/src/providers` 的清晰边界，方便日后抽包。

---

## 11. 构建、分发与签名

| 平台 | 产物 | 注意 |
|---|---|---|
| macOS | `.dmg` / `.app` | 公开发布需 Apple Developer 账号做签名 + 公证（$99/年）；v1 自用可不签（Gatekeeper 右键打开） |
| Windows | `.msi` / `.nsis` | 建议代码签名（EV/OV），可先用自签；WebView2 缺失时引导装 bootstrapper |
| CI | GitHub Actions + `tauri-action` | macOS 构建需 macOS runner；双端矩阵构建 |

- `tauri.conf.json`：`transparent`、`decorations:false`、`alwaysOnTop`、`visible:false`、`macosPrivateApi` 开关都集中在这里。
- 更新：v1 不做自动更新；二期可接 `tauri-plugin-updater`。

---

## 12. 测试策略

| 层级 | 工具 | 覆盖 |
|---|---|---|
| TS 核心 | Vitest | normalize、provider 请求构建、设置校验、错误分类 |
| Rust | cargo test + `mockito` | provider 适配器（mock HTTP）、SSE 解析、SQL 迁移 |
| E2E（增值） | tauri-driver / WebdriverIO | 弹窗开合、热键→面板、（假引擎）翻译/语法流程 |
| 手工 QA | 真机双端 | 权限引导、快捷键冲突、中文输入法、断网降级 |

---

## 13. 性能目标

| 指标 | 目标 |
|---|---|
| 安装体积 | < 20MB（不含 Win WebView2 系统运行时） |
| 常驻内存 | macOS 40–80MB 级（WKWebView 冷页面）；监控 ≤120MB 警戒线 |
| 热键→弹窗显示 | < 30ms（预创建 + hide/show，**禁止**按热键才建窗） |
| 翻译首字 | < 1s 感知（非 LLM 引擎）；LLM 流式首 chunk < 1.2s |
| 语法检查 | 骨架先出 + 流式，20s 硬超时 |

---

## 14. 风险与平台坑（排优先级）

1. **macOS 不抢焦点**（P0 体验）：Tauri 窗口非 NSPanel，需 objc2 方案；v1 可降级「激活才显示」并加开关，但这是体验核心，尽量做成默认不激活。
2. **选中读取的权限与失败链**（P0）：AX 授权后需重启；Windows UIA 覆盖不全——兜底链（剪贴板模拟）必须通。
3. **CORS**（P1 工程）：所有引擎 HTTP 必须在 Rust 走（§3.3 D1），WebView 内绝不对引擎域名 fetch。
4. **LLM 不稳定输出**（P1）：JSON 解析失败重试/降级宽松解析；错误分类统一（密钥缺失 / 限流 / 网络）。
5. **密钥存储**：未签名 macOS 开发构建的 Keychain 弹窗；设计「测试连接」在开发期用临时内存 Key 验证，减少打扰。
6. **自启在 macOS 的限制**：SMAppService 要求 App 在 /Applications；引导文案要说明。
7. **Windows 视觉效果**：透明/圆角受限，v1 用稳妥方案，别卡在视觉上。
8. **国内网络可达性**：Provider 选择要贴合用户网络环境（有道的价值）。
9. **I18n**：界面语言和翻译语气（面向英语学习者）分开管理，避免把 UI 文案硬编码进引擎 prompt。

---

## 15. 里程碑计划

| 里程碑 | 内容 | 阶段产物 |
|---|---|---|
| **M0 骨架** | tauri 初始化、Vue3 无边框隐藏窗、热键注册/显示/隐藏、生命周期、设置 store | 双击能唤醒的灰壳 |
| **M1 翻译** | `translate` 命令（DeepL + 有道 + LLM）、选中捕获链、翻译 Tab、历史入库 | 选中即译可用 |
| **M2 语法** | LLM 语法提示词 + 流式 + 结构化解析、错误卡片、Languagetool 可选适配器 | 语法检查可用 |
| **M3 错题本** | CRUD + 过滤 + 导出 Markdown + AI 总结 | 错题本能用 |
| **M4 设置完善** | 快捷键重配、自启、Provider 管理+测试、权限引导、i18n、主题 | 可交付内测 |
| **M5 打磨** | 流式优化、动画、托盘菜单、内存/性能压测、签名公证双端打包 | 可对外分发 |
| **后置** | Remote transport（SaaS 预留）、云同步、内联纠错评估 | — |

---

## 16. 参考资料（选型依据）

- Tauri 官网（v2.0，最小≈600KB，原生 WebView）—— https://tauri.app/
- Tauri 全局快捷键插件 —— https://tauri.app/plugin/global-shortcut/
- Tauri 2.9.3 文档（窗口/焦点/透明 `macos-private-api`）—— https://docs.rs/tauri/2.9.3/tauri/
- Wails（Go 版轻量方案，v2 稳定 / v3 beta）—— https://github.com/wailsapp/wails
- WebView2 运行时分发（Win11 预装 / Win10 绝大多数已装 / bootstrapper 2MB）—— https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution
- NSPanel（`nonactivatingPanel` / `orderFrontRegardless` / `becomesKeyOnlyIfNeeded`）—— https://developer.apple.com/documentation/appkit/nspanel
- LanguageTool（开源语法检查、可自托管 HTTP 服务）—— https://dev.languagetool.org/
- Swift 平台支持（Windows 可部署但 AppKit/SwiftUI 仍属苹果生态）—— https://www.swift.org/platform-support/
- Raycast 开发者平台（React/TS 扩展体系，插件生态参考）—— https://developers.raycast.com/
- `keyring` crate（macOS Keychain / Windows Credential Manager）—— https://crates.io/crates/keyring

---

*本文档随需求变化维护。任何「预留」字段/边界改动需和 M4 一起评审。*