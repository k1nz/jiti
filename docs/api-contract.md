# Jiti IPC 契约

> 版本：**SCHEMA_VERSION = 13** · 2026-08-31 · 收藏 + 复习课独立窗
> 事实源：`packages/native/src/ipc.rs`（tauri-specta 生成 `packages/ui/src/ipc/bindings.ts`）
> 本文同时是未来远程后端 OpenAPI 初稿。破坏性变更必升 `SCHEMA_VERSION`；新字段一律 optional。

命令与事件是互不相交的两套形状。WebView 只 `invoke` 下列命令、监听下列事件，不直连引擎 HTTP，不持有 API Key。

## 命令

| 命令 | 入参 | 出参 | 说明 |
|---|---|---|---|
| `translate` | `TranslateRequest` | `TranslateResult` / `EngineErrorPayload` | 翻译；成功可写 `history(kind=translate)`。英语短词（英译中看原文、中译英看译文）经 `translate://enriched` 后台补发词卡，失败不影响译文 |
| `grammar_check` | `GrammarRequest` + progress Channel | `GrammarCheckOutcome` / `EngineErrorPayload` | SSE+NDJSON；`requestId` 可选；被替换的请求返回 `cancelled` 且不写历史/错题；`mistakeIds` 与 `result.errors` 按下标对齐 |
| `mistakes_list` | `MistakeFilter` | `MistakeList` | 分页/全量筛选 |
| `mistakes_create` | `NewMistake` | `Mistake` | 手动收录；缺省 status 用偏好 `defaultStatus` |
| `mistakes_update` | `id`, `MistakePatch` | `Mistake` | 改 status / tags |
| `mistakes_delete` | `id` | `number` | 删除行数 |
| `mistakes_preferences` | — | `MistakePreferences` | `settings.json` 的独立 `mistakes` 键 |
| `mistakes_set_preferences` | `MistakePreferences` | `MistakePreferences` | 默认 `autoCollect=true`、`defaultStatus=open` |
| `mistakes_export` | `MistakeFilter` | `string \| null` | 当前筛选全集 Markdown；对话框取消返回 `null`，不写盘 |
| `mistakes_ai_review` | `MistakeFilter` | `AiReviewResult` / `EngineErrorPayload` | 聚合频次+代表例句后请求 LLM；成功写 `history(kind=ai_review)`。**不**生成可练习方案 |
| `clips_save` | `NewClip` | `ClipSaveResult` | 英语进 `text`、中文释义进 `note`（写反会互换）；规范化去重，已存在则补全缺的一侧且 `created=false` |
| `clips_list` / `clips_update` / `clips_delete` | 筛选 / patch / id | 列表 / 行 / 删除行数 | `status`：open / learned / archived |
| `open_study` | — | — | 按需创建复习课窗口（关即毁）；先收起主面板 |
| `review_plan_generate` | — | `ReviewPlan` / `EngineErrorPayload` | 开放错题+收藏本地分桶后一次 LLM 写文案；只保留一套 active |
| `review_plan_active` | — | `ReviewPlan \| null` | 当前方案 |
| `review_plan_grade` | `ReviewGradeRequest` | `ReviewGradeResult` | `answer` 比对填空/默写；`markLearned` 才改源行 status |
| `review_plan_complete_day` | `planId`, `dayIndex` | `ReviewPlan` | 标记该轮完成 |
| `history_list` / `history_clear` | — | 条目 / 删除行数 | 含 translate / grammar / ai_review / review_plan |
| `providers_*` / `provider_*` | 见绑定 | 快照 / 测试结果 | Key 只进 `keys.json` |
| `preferences_snapshot` | — | `PreferencesSnapshot` | locale / theme / 唤起聚焦 / 真实自启状态 |
| `preferences_update` | `PreferencesPatch` | `PreferencesSnapshot` / string | 自启失败时返回错误，UI 回滚 |
| `open_settings` | — | — | 独立设置窗口：已有则聚焦，否则创建；关闭即销毁 |
| `app_version` | — | `string` | 当前 `CARGO_PKG_VERSION` |
| `check_for_updates` | — | `UpdateCheckResult` | 对比 GitHub Releases / tags；不自动下载 |
| `open_external_url` | `url` | — / string | 仅允许打开 `https://github.com/k1nz/jiti…` |
| `hotkeys_*` / `permissions_*` / `panel_*` / `settings_*` | 见绑定 | — | 壳层能力 |

## 关键类型

```ts
GrammarCheckOutcome {
  result: GrammarResult
  mistakeIds: (number | null)[]   // 与 errors 等长；未收录为 null
}

MistakeFilter {
  errorType?: string | null       // grammar | spelling | punctuation | word_choice | style
  status?: string | null          // open | learned | archived
  timeRange?: "sevenDays" | "thirtyDays" | "all"
  limit?: number | null
  offset?: number | null
}

MistakePreferences { autoCollect: boolean, defaultStatus: string }

AiReviewResult { summary: string, analyzedCount: number, engine: string, durationMs: number }

Clip { id, createdAt, text, note?, kind: "word"|"phrase"|"sentence", status, ... }
ClipSaveResult { clip: Clip, created: boolean }
ReviewPlan { id, horizonDays, currentDay, summary, days: ReviewPlanDay[], items: ReviewPlanItem[] }
ReviewGradeRequest { itemId, answer?, outcome, markLearned }

TranslateResult.enrichment?: {
  word?, phonetic?, audioDataUrl?, mnemonic?, mnemonicZh?, roots?,
  examples: { text, translation? }[],
  senses: { pos?, definition, translation? }[]
}
TranslateResult.enrichmentWord?: string
TranslateResult.enrichmentPending?: boolean

UpdateCheckResult {
  status: "upToDate" | "available" | "error"
  currentVersion: string
  latestVersion?: string | null
  releaseUrl?: string | null
  message?: string | null
}
```

```ts
PreferencesSnapshot {
  locale: "system" | "zh-CN" | "en-US"
  theme: "system" | "light" | "dark"
  autostart: { enabled: boolean, hint: string | null }
  focusOnInvoke: boolean          // 默认 true；缺省按 true
}
```

`GrammarResult` / `GrammarError` 不含持久化字段。空筛选的 AI 复习返回 `bad_request`（「当前筛选没有错题」）；空词库的复习课生成返回 `bad_request`（「没有可复习的错题或收藏」）。网络/鉴权走既有 `EngineErrorPayload`。面板「AI 总结」与导出**不修改**错题行。复习课会话中用户点「已掌握」才把对应错题/收藏标为 learned。

收藏热键成功时**不唤起面板**；选区为空才 show 面板，并经 `clip://saved` 提示。

## 事件

| 事件 | payload |
|---|---|
| `engine://error` | `EngineErrorPayload` |
| `hotkey://pressed` | `{ mode, epoch, selection }` |
| `capture://changed` | `{ epoch, selection }` |
| `translate://enriched` | `{ input, output, word, enrichment? }` |
| `clip://saved` | `{ status: saved\|duplicate\|empty\|error, text?, created }` |
| `panel://visibility` | `"shown" \| "hidden"`（非 specta 事件） |
| `preferences://changed` | `{ snapshot: PreferencesSnapshot }` |

## 错误码

`missing_key` / `unauthorized` / `rate_limited` / `network` / `invalid_config` / `bad_request` / `invalid_response` / `cancelled` / `upstream`。
被新热键替换或 Channel 关闭的语法检查返回 `cancelled`，不发 `engine://error`，也不写入历史/错题。
