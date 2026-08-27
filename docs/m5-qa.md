# M5 RC 双端 QA

覆盖 macOS 13+ / Apple Silicon 与 Windows 10/11。IME 专项仍走 [`docs/m4-ime-qa.md`](m4-ime-qa.md)。本页是系统集成证据，不替代单元测试，也不用单测冒充这些步骤。

**RC 产物性质**：内部测试包。macOS 为 ad-hoc 签名，未经公证；Windows 未代码签名。不要对外宣称可信发布，也不要让测试者永久关闭 Gatekeeper。

## 环境

| 项 | macOS | Windows |
|---|---|---|
| 系统 | 13+，Apple Silicon（x64 另测一份若有包） | 10 或 11，x64 |
| 安装 | 见下方 Gatekeeper 步骤，放到 `/Applications` 再测自启 | 安装 MSI/NSIS；缺 WebView2 时走 bootstrapper |
| 用户目录 | **全新** `~/Library/Application Support/com.jiti.app` | **全新** `%AppData%\com.jiti.app` |
| 主题 / 语言 | 浅、深、跟随系统；中文、English、跟随系统 | 同左 |

### 未公证 macOS 内部安装（不要关 Gatekeeper）

1. 从 Actions artifact 下载 `.dmg` 或 `.app`，核对 SHA-256。
2. 若出现「已损坏 / 无法打开」：在终端对 **该一份** 包执行  
   `xattr -dr com.apple.quarantine /path/to/Jiti.app`  
   这只清除隔离属性，不关闭系统 Gatekeeper。
3. 把 App 拖进 `/Applications` 后再测开机自启（SMAppService 要求）。
4. 不要执行 `spctl --master-disable`，不要教用户关 Gatekeeper。

Windows：首次运行可能被 SmartScreen 拦。内部测试选「仍要运行」。正式签名在 M5.1。

## 冒烟清单

每步记 ✓ / ✗ / 跳过。macOS 与 Windows **各留一份**结果表。

### 安装与权限

| # | 步骤 | 期望 |
|---|---|---|
| S1 | 全新用户目录安装并启动 | 无白闪；托盘出现；无僵尸第二进程 |
| S2 | 冷启到可交互 | macOS <600ms，Windows <900ms（尽量记实测） |
| S3 | macOS 辅助功能引导 | 全屏引导，不是遮罩；「去开启」跳系统设置；「稍后再说」可手动输入 |
| S4 | 授权后读选区 | 选中文字后热键填入；失败走剪贴板兜底 |
| S5 | Windows 无 AX 引导 | 直接可用 |

### 热键与选区

| # | 步骤 | 期望 |
|---|---|---|
| H1 | 翻译热键 | 直达翻译并检查选区 |
| H2 | 语法热键 | 直达语法并检查选区 |
| H3 | 统一面板热键 | 保留上次模式 |
| H4 | 设置里重配热键 | 立即生效；冲突有提示 |
| H5 | 录制热键时按 Esc | 取消录制，面板不关 |

### 键盘与焦点

| # | 步骤 | 期望 |
|---|---|---|
| K1 | Tab / Shift+Tab | 在控件间移动，不切模式 |
| K2 | 焦点在 Tab 条时 ←/→ | 切换模式 |
| K3 | ⌘←/→ 或 Ctrl+←/→ | 切换模式，焦点策略可预测 |
| K4 | Esc | 先关错题筛选等弹层，再隐藏面板 |
| K5 | ⌘W / Ctrl+W | 主面板隐藏且下次热键仍秒开（预热还在） |
| K6 | 设置 ⌘W / 关窗 | 设置窗销毁；再开是新窗 |
| K7 | 列表输入字母 | 跳到匹配的历史/错题行 |

### IME

按 [`m4-ime-qa.md`](m4-ime-qa.md) 全表走一遍。

### 原生约定

| # | 步骤 | 期望 |
|---|---|---|
| N1 | 右键面板/设置 | 无 Reload / 检查元素 / 浏览器菜单 |
| N2 | macOS 对单词 force-touch | 无词典预览 |
| N3 | 按钮/Tab 悬停 | 箭头指针，不是手型 |
| N4 | 输入框悬停 | I-beam |
| N5 | chrome 标签拖选 | 选不中；结果/讲解可选 |
| N6 | 多屏：光标在副屏按热键 | 面板出现在该屏并夹在工作区内 |
| N7 | 点面板外 | 未固定则隐藏；固定则保持 |
| N8 | 二次启动（尤其 Win） | 聚焦已有实例，不新建进程 |
| N9 | 托盘退出 | 进程消失 |

### 语法流式

| # | 步骤 | 期望 |
|---|---|---|
| G1 | 快速返回（<200ms 缓存） | 不闪 spinner |
| G2 | 慢请求 | 超过约 200ms 才出现 spinner；记录逐条出现 |
| G3 | 检查中再按语法热键 | 旧请求不再写历史/错题；界面只显示新结果 |
| G4 | 检查中隐藏面板 | 不把过期结果写进当前视图 |
| G5 | 断网 / 拔线 | **10 秒内**可见错误，不是干等到 20–60s |
| G6 | 结构失败重试 | 可看到「正在重新解析」；总耗时仍受 20s 预算约束 |

### 设置 / 主题 / 语言 / 自启

| # | 步骤 | 期望 |
|---|---|---|
| P1 | ⌘, 打开设置 | 独立窗；关后内存里窗口应消失 |
| P2 | 切主题 | 两窗同步，无白闪 |
| P3 | 切界面语言 | 标签切换；语法讲解仍为中文 |
| P4 | 开机自启开 | macOS 须在 /Applications；Win 重启仍在 |
| P5 | 开发目录启动自启 | macOS 提示无法注册并回滚开关 |

### 错题 / 历史 / 导出

| # | 步骤 | 期望 |
|---|---|---|
| M1 | 语法错误自动收录 | 错题本出现对应条目 |
| M2 | 筛选弹层 | Esc 只关弹层 |
| M3 | 导出 Markdown | 原生保存对话框；取消不写盘 |
| M4 | AI 复习 | 有总结；空筛选有明确错误 |

### 安装升级卸载

| # | 步骤 | 期望 |
|---|---|---|
| U1 | 覆盖安装同版本或更新 RC | 设置与错题还在 |
| U2 | 卸载 | 可注明数据目录是否保留（当前不主动删 Application Support） |
| U3 | 包内无 `.map`、无 `keys.json`、无用户数据库 | `find` / 解包抽查 |

## 感知指标（记实测，不要编）

沿用 [`docs/architecture.md`](architecture.md) §13。测前说明机器型号、是否接电源。

| 指标 | 目标 | macOS 实测 | Windows 实测 |
|---|---|---|---|
| 暖启热键→可见 | <30ms（<200ms 门禁） | | |
| 冷启→可交互 | <600ms / <900ms | | |
| 语法首条语义记录 | 流式尽快；硬超时 20s | | |
| 断网错误时延 | ≤10s | | |
| 隐藏时 CPU | <0.5% | | |
| 空闲内存（压力绿即可） | mac ~80–120MB，Win ~130–180MB | | |

度量：内存看压力不看单进程虚高；若记录数字请注明来源（Activity Monitor / 任务管理器）。

### 本机 RC 打包记录（2026-08-27）

开发机 macOS 13 / x86_64 已跑通 `pnpm --dir packages/native tauri build`：

- `packages/native/target/release/bundle/macos/Jiti.app`（ad-hoc，`adhoc,runtime`）
- `packages/native/target/release/bundle/dmg/Jiti_0.5.0-rc.1_x64.dmg`
- SHA-256 见 [`m5-readiness.md`](m5-readiness.md) 自动门禁表
- 未公证：内部安装用 `xattr -dr com.apple.quarantine`，不要关 Gatekeeper
- 当时已有 `tauri dev` 占用热键/单实例，**未**用 RC `.app` 填上方实测列；Windows 包等 Actions `rc-build.yml`

打内部 tag `v0.5.0-rc.1` 会触发 CI 上传双端 artifact；A–G 无未解释 Red，但上表「待核」填完前不要宣称双端验收完成。

## 结果模板

复制两份，分别填 macOS / Windows。

```
平台：
构建：0.5.0-rc.1 / artifact SHA-256：
机器：
日期：
测试人：

冒烟：S__ H__ K__ N__ G__ P__ M__ U__
IME：见 m4-ime-qa.md
Red / 阻断：
备注：
```
