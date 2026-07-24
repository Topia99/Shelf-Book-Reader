# Shelf 全平台进度看板（progress.md）

> 全局唯一进度事实来源。每完成一项任务即时更新本文件。
> **新会话接手先读 [PROJECT.md](PROJECT.md)（项目主文档，单文件即可接手）。**
> 任务定义与验收标准见 [implementation_plan.md](implementation_plan.md)，架构与决策见 [全平台开发文档.md](全平台开发文档.md)。
> 状态：✅ 完成（已验收）｜🔄 进行中｜⏳ 待开始｜🚫 被依赖阻塞｜⛔ 验收打回

**最后更新**：2026-07-22（v0.3.0 全平台协调发布）
**当前阶段**：阶段 4 基本收尾——iOS 真机通过 ✅、P4-6~8 ✅、P4-9 内存初筛通过（待真机复核）🔄、**P4-10 TestFlight 流水线完成** ✅（本地+GitHub Actions 双路径端到端验证，tag→自动发布）；剩余：P4-11 邀测试员（用户，教程已给）、桌面端人工回归清单（用户）、P4-9 真机 Instruments 复核（用户）
**里程碑**：M1 Mac 版可用 🔄（开发完，待人工走查）｜M2 双端接着读 🔄（书目同步已实证，进度互通待验）｜M3 TestFlight 可分享 ✅（本地+CI 双发布路径打通，2 个构建在 TestFlight，就差用户邀测试员 P4-11）｜M4 商店上线 ⏳

---

## 前置任务（外部依赖，尽早启动）

| ID | 任务 | 状态 | 备注 |
|---|---|---|---|
| H-1 | Apple Developer 注册（$99/年） | ✅ | 2026-07-12 用户确认；Xcode 16.2 已登录 Apple ID |
| H-2 | Azure Trusted Signing 开通 | ⏳ | 阻塞 P1-7 的 Windows 签名 |
| H-3 | Cloudflare + Supabase 账号 | ✅ | Supabase 已 login + link；R2 凭证经 .env.r2.local 提供（不入库），写读删全链路验证通过 |
| H-4 | 域名注册 | ⏳ | 阻塞 P6-1 |

## 阶段 0：项目整备 ✅（2026-07-08 完成）

| ID | 任务 | 状态 | 执行者 | 验收记录 |
|---|---|---|---|---|
| P0-1 | 数据目录跨平台化（APPDATA → app_data_dir + Windows 迁移） | ✅ | Codex 窗口1 → 打回 → Claude 返工 | 2026-07-10 真机回归打回（DB 存绝对路径迁移后悬空 + 迁移判据 `!new_base.exists()` 过脆）；同日返工：DB 改存相对路径（`books/<hash>.pdf`，返回前端/删文件时拼回绝对，iOS 容器路径变化同样受益）、init_db 幂等改写存量绝对路径、迁移判据改"哪边有 library.db"+ 空骨架清除 + 全异常路径回退旧目录；14 单测全绿，真机实测 DB 改写生效。UI 回归须在 Claude 容器外做（AppData 虚拟化） |
| P0-2 | 跨平台假设审计 | ✅ | Codex 窗口2 | 20 条清单（4 阻塞/13 重要/3 次要）→ [docs/跨平台审计清单.md](docs/跨平台审计清单.md) |
| P0-3 | 快捷键抽象层（src/platform.ts） | ✅ | Codex 窗口3 | tsc 通过；Reader 滚轮缩放已接入 |
| P0-4 | tauri.conf.json 修复（+app/dmg 目标、+icns、scope 对齐） | ✅ | Codex 窗口4 | JSON 校验通过 |
| P0-5 | CI 工作流（前端 + Win/mac 双平台 Rust） | ✅ | Codex 窗口5 | GitHub 实跑三 job 全绿（[run 29127549159](https://github.com/Topia99/Shelf-Book-Reader/actions/runs/29127549159)）；首轮 Windows clippy 打回一次已修复 |

审计清单遗留项流向：触屏交互阻塞项 → P4-4/P4-5；配置项已由 P0-4 消化；platform.ts UA 判断的健壮性 → P4-1 时复核。

## 阶段 1：macOS 版

| ID | 任务 | 状态 | 依赖 |
|---|---|---|---|
| P1-1 | macOS tauri dev 跑通 | ✅ | P0-1 ✅（2026-07-11：进程/窗口 1240×820/数据目录+WAL/日志零错误全过；14 单测复验；UI 人工走查并入 M1 演示） |
| P1-2 | dmg/app 打包 + icns | ✅ | Shelf.app + Shelf_0.2.2_aarch64.dmg 产出；icns 生效、dict.db 入包（24MB）、ad-hoc 签名（分发签名待 H-1/H-2） |
| P1-3 | 原生菜单栏 | ✅ | 缩容关闭：Tauri 2 macOS 默认菜单已含 App/File/Edit/View/Window/Help（截图证实），无需自定义代码 |
| P1-4 | 窗口控件适配 | ✅ | 缩容关闭：应用用原生窗口装饰（无 decorations:false、无自绘标题栏），红绿灯无遮挡风险 |
| P1-5 | 触控板手势 | ✅ | Codex 交付捏合缩放（WebKit gesture 事件）；tsc 打回一次（Event 断言），主控修正后通过 |
| P1-6 | Cmd 快捷键全量接入 | ✅ | 审计验证关闭：全部按键为平台中立键，修饰键已走 isModKey，无 Ctrl 文案残留 |
| P1-7 | 签名/公证/发布流水线 | 🚫 | P1-2, H-1, H-2 |
| P1-8 | 自动更新 | 🚫 | P1-7 |

## 阶段 2：云服务 MVP

| ID | 任务 | 状态 | 依赖 |
|---|---|---|---|
| P2-1 | Supabase 项目 dev/prod | ✅ | 用户已有项目（us-east-1）用作 dev 并已 link；prod 名额留待上线前 |
| P2-2 | 云端 schema + RLS migration | ✅ | 本地栈实跑打回一次（缺表级 GRANT，RLS≠授权），补齐后 db push 成功，远端 migration 一致 |
| P2-3 | RLS 隔离测试 | ✅ | 7 断言全绿（隔离/越权写/越权改/配额硬拒/触发器）；supabase/tests/ 入库可重复执行 |
| P2-4 | Auth 三渠道配置 | 🚫 | P2-1, H-1 |
| P2-5 | R2 bucket + token | ✅ | bucket shelf-book-storage；签名请求 PUT/GET/DELETE 往返验证通过 |
| P2-6 | Edge Function：预签名 + 配额 | ✅ | 本地栈端到端全绿（401/403/413/200 四用例 + 真实 R2 上传下载往返 + 配额精确入账）；已部署云端并配 secrets |
| P2-7 | Edge Function：删除账号级联 | ✅ | 打回一次（DOMParser 不存在于 Edge Runtime，改正则解析）；重试语义实测成立；R2 前缀/DB 行/auth 用户三级清理端到端验证；已部署云端 |
| P2-8 | 本地 SQLite v2 迁移（同步字段） | ✅ | user_version=2 迁移/updated_at 写入钩子/墓碑删除/同 hash 复活；clippy 零警告，单测 14→18 全绿 |
| P2-9 | 集成测试骨架 | 🚫 | P2-3, P2-6 |

## 阶段 3：账号 + 进度同步

| ID | 任务 | 状态 | 依赖 |
|---|---|---|---|
| P3-1 | SyncBackend trait + 数据模型 | ✅ | src-tauri/src/sync.rs：5 模型 + SyncError + 10 方法 trait；clippy 零警告 |
| P3-2 | Supabase 后端实现 | ✅ | GoTrue+PostgREST+sign-url 客户端；打回一次（闰日转换差一天 + 3 处 clippy 含逻辑笔误），返工后全绿 |
| P3-3 | Token 安全存储 | ✅ | keyring 系统钥匙串（macOS Keychain/Win 凭据管理器/iOS 同栈），40 测试绿；真实钥匙串读写留 M2 联调 |
| P3-4 | SyncEngine 核心（LWW/墓碑/游标） | ✅ | Codex 交付后主控修正关键缺陷：本地 LWW 胜出时误推进 synced_at 导致本地值永不上推——改为「只有采纳远端值才变干净」 |
| P3-5 | SyncEngine 单测（最高优先级测试） | ✅ | 15 测试全分支覆盖（LWW 双向/墓碑/窗口/游标/幂等重放）；全库 39 测试绿、clippy 零警告 |
| P3-6 | SyncEngine 接线（触发器/退避） | ✅ | sync_runtime.rs：专用线程/命令通道/5s 防抖+30s 最小间隔/5min 心跳/指数退避/钥匙串会话恢复/6 个 Tauri 命令 + sync-status 事件；40 测试绿 |
| P3-7 | 登录/注册/账号 UI + 删除账号 | ✅ | AccountPanel（登录/注册/删除账号需输 DELETE 二次确认）+ 工具栏账号徽标 + 事件订阅；tsc 通过；真机点击走查归 M2 |
| P3-8 | Win↔Mac 双端联调（M2 里程碑） | 🔄 | 2026-07-13 用户双 Mac 实测：注册/登录/云端书目跨设备同步 ✅；发现远端书打开报 403（文件本体属阶段 5）→ 已加云端角标 + 打开拦截提示；进度互通测试待用户在两端导入同一 PDF 验证 |

## 阶段 4：iOS / iPadOS

| ID | 任务 | 状态 | 依赖 |
|---|---|---|---|
| P4-1 | tauri ios init + 模拟器跑通 | ✅ | 2026-07-12：iPhone 15 Pro 模拟器实跑截屏验收——UI 完整渲染、同步引擎启动、library.db 落 iOS 数据容器、dict.db 随包（P4-8 或可免做）；真机待 M3 |
| P4-2 | 页面虚拟化（可见页 ±1） | ✅ | 按设计满足关闭：Reader 为翻页式（固定 2 canvas 只渲染当前跨页），无滚动列表，无需虚拟化 |
| P4-3 | 渲染分辨率预算（dpr≤2 + 单 canvas 1600 万像素预算） | ✅ | 布局/位图尺寸解耦，超预算按 √ 比例降渲染 scale；tsc 通过（真机内存压测仍归 P4-9） |
| P4-4 | 触屏手势（滑动/捏合/长按取词）+ 审计阻塞项（轻触打开、操作菜单） | ✅ | 点击翻页区/横滑翻页/长按取词（caretRangeFromPoint）/工具栏点击显隐/触屏单击开书/⋯管理菜单；双重守卫（isTouchDevice+pointerType）桌面零变化；tsc 通过，模拟器人工走查待用户 |
| P4-5 | 移动端布局（安全区/44pt/响应式）+ 审计 CSS Top5 | ✅ | 审计 5 项全修 + 安全区 + 44pt + :active 反馈；iPhone 模拟器截屏实证两行工具栏生效；遗留：目录抽屉无点击关闭遮罩（CSS-only 边界） |
| P4-6 | 文件导入（选择器 + 分享到 Shelf） | ✅ | 应用内选择器 + **分享到 Shelf / 用 Shelf 打开**（fileAssociations 自动生成 CFBundleDocumentTypes + RunEvent::Opened 缓存转发 + 前端 importPaths 复用入库）；2026-07-21 用户真机确认分享 PDF 到 Shelf 入库通过 ✅ |
| P4-7 | 生命周期持久化 | ✅ | 进后台立即 flush 进度：阅读页加 `visibilitychange(hidden)` + `pagehide` 监听→`flushProgress`（同步落库 + updateProgress 内发 SyncNow 尽力 push）。补 iOS/移动端 `beforeunload` 不触发导致防抖内翻页丢失的缺口；模拟器探针实证 WKWebView 进后台确触发 visibilitychange（hidden 0→1）。桌面切窗口也 flush，正向无害 |
| P4-8 | dict.db 首启拷贝 | ✅ | 免拷贝：dict.db 随 bundle（Shelf.app/assets/resources/dict.db 24MB），`open_dict` 经 BaseDirectory::Resource 解析。模拟器实测 `lookup_word` 四词全命中（book/reading 精确、ran→run 词形还原、better 后缀），iOS 查词链路端到端可用；长按取词手势 P4-4 已真机验证 |
| P4-9 | 内存压测（峰值 <600MB） | 🔄 | 模拟器初筛**通过**：302MB/122 页扫描版 PDF、自动翻 60 页+反复缩放，Shelf 相关进程(app+WebContent+GPU+Net) phys_footprint 峰值 **583MB<600MB**、无终止、压测后回落 468MB 无泄漏（D9 预算生效）。主导项=pdf.js 持有的 ~300MB 完整文件缓冲（WebContent）。**余量薄(~17MB)+模拟器≠真机，待真机 Instruments 权威复核**。工具：tools/gen-scan-pdf.py |
| P4-UI-1 | iOS 壳层：结构拆分（controller/stage/双壳） | ✅ | Codex 拆分 + 主控验收；tsc 通过；macOS 启动冒烟零错误（完整桌面回归清单留用户过） |
| P4-UI-2 | iOS 壳层：平台路由（isIos 含 iPad 判定） | ✅ | 修订 2 判定实现正确（含 iPad 伪装 UA）；iPhone 模拟器实跑命中 iOS 壳、macOS 命中桌面壳 |
| P4-UI-3 | iOS 壳层：顶部 pill/页码 badge/沉浸显隐 | ✅ | 模拟器实拍：双 pill/badge/fit-width/浅灰舞台全达标；主控修复 stage 顶距被尾部触屏规则覆盖的层叠 bug（提高特异性） |
| P4-UI-4 | iOS 壳层：底部缩略图 strip（内存预算硬约束） | ✅ | 主控实现：±5 页窗口/近处优先补齐/150px 上限/JPEG dataURL 化即销毁 canvas/24 张 LRU/空闲启动+翻页代际作废；模拟器实拍 5 页缩略图全渲染、当前页赭色描边（4.4 大文件压测并入 P4-9） |
| P4-10 | TestFlight 流水线 | ✅ | 本地流水线打通并**首次成功上传 TestFlight**（App 6793296218，build 0.2.2）：tauri ios build（app-store-connect 导出，ASC API 密钥自动签名）→ fastlane pilot 上传；凭证走 .env.asc.local(gitignored)。新 Apple 账号 Team=R59Q7NXAWD（旧 AZ3MB6QPUV 全量替换）。**关键坑**：xcodebuild 云端分发签名要求 API 密钥 **Admin** 角色（App Manager 报 Cloud signing permission error）。加 ITSAppUsesNonExemptEncryption=false 免加密合规询问。build 号自增已接（时间分钟数→tauri --build-number 追加为 0.2.2.N，实测 0.2.2.3448562 导出成功）；GitHub Actions 发布工作流 `.github/workflows/release-ios.yml` **端到端验证通过**（推 tag v0.2.2-ci1 → macOS runner 自动构建+云端签名+上传 TestFlight 全绿，[run 29961220161](https://github.com/Topia99/Shelf-Book-Reader/actions/runs/29961220161)）。本地+CI 两条发布路径均通。**待办**：P4-11 邀请测试员（用户，教程已给） |
| P4-11 | TestFlight 公测（M3 里程碑） | 🚫 | P4-10 |

## 阶段 5：书籍文件同步

| ID | 任务 | 状态 | 依赖 |
|---|---|---|---|
| P5-1 | 上传队列 | 🔄 | 代码实现+单测完成（待真实账号端到端验证）：run_cycle 元数据同步后加上传趟，`collect_uploadable`(cloud_state=local) → 读文件 → `upload_book_file`（sign PUT + 直传 R2 + PATCH 回填 file_key）→ 置 synced；配额满停止本轮、瞬时失败回退重试、>100MB 暂跳过（MVP 单次 PUT，决策：登录后自动上传）。踩坑待办：sign 时预扣配额致失败重试双计（P5-4 修） |
| P5-2 | 封面缩略图上传 | 🚫 | P5-1 |
| P5-3 | 云书架 + 按需下载 | 🔄 | 下载核心完成（待端到端验证）：`sync_download_book` 命令（reply-style，180s 超时）→ 引擎推导对象键 → 预签名 GET → 校验 SHA-256 → 写书库 → synced；前端 Library 把 remote 书拦截换成点击即下载（下载中角标+防重复+完成打开）。封面显示归 P5-2 |
| P5-4 | 传输策略设置页 | 🚫 | P5-1 |
| P5-5 | 四端联调 | 🚫 | P5-3, 阶段4 |

## 阶段 6：上架

| ID | 任务 | 状态 | 依赖 |
|---|---|---|---|
| P6-1 | 官网 + 隐私政策 | 🚫 | H-4 |
| P6-2 | App Store 审核材料 | 🚫 | P4-11 |
| P6-3 | Mac App Store 变体 | 🚫 | P1-7 |
| P6-4 | Microsoft Store msix | 🚫 | P1-7 |
| P6-5 | 三商店提审（M4 里程碑） | 🚫 | P6-2~4 |

## 阶段 7：商业化（可选）

| ID | 任务 | 状态 | 依赖 |
|---|---|---|---|
| P7-1 | 小型企业计划 | 🚫 | P6-5 |
| P7-2 | iOS StoreKit 订阅 | 🚫 | P6-5 |
| P7-3 | 桌面端支付 | 🚫 | P6-5 |
| P7-4 | 配额升降级 | 🚫 | P7-2/P7-3 |

---

## 执行日志（倒序）

- **2026-07-22（阶段 5 启动：P5-1 上传 + P5-3 下载核心实现）**：读设计文档确认地基已就位（元数据同步/签名+配额 Edge Function/cloud_state 语义/CloudBook 带 file_key 全就绪），P5 本质=补文件字节传输层。关键简化：R2 键内容寻址+用户前缀（`{user_id}/books/{hash}.pdf`），下载端凭自身 user_id+hash 自推导，**无需改本地 schema**。决策（用户拍板）：登录后自动上传（移动端默认 Wi-Fi）+ MVP 单次 PUT（≤100MB）。实现：① sync_engine 加 `collect_uploadable`/`set_cloud_state`（+2 单测）；② sync_supabase 加 `upload_book_file`（sign PUT+直传 R2+PATCH file_key）/`download_book_file`（sign GET，404→尚未就绪）；③ sync_runtime run_cycle 元数据同步后加上传趟（配额满停止、瞬时失败回退重试、>100MB 跳过），加 `DownloadBook` reply 命令 + `download_book`（校验 SHA-256 写库置 synced）；④ lib.rs 加 `sha256_of_bytes` + `sync_download_book` 命令；⑤ 前端 Library 把 remote 书拦截换成点击下载（下载中角标/防重/完成打开）。clippy 干净、tsc 干净、cargo test 42 passed。**待端到端验证**：需登录真实账号跑上传→R2、另端下载往返（同 M2 由真实会话验证）。
- **2026-07-22（v0.3.0 全平台协调发布 + Windows 版追平）**：用户发现 GitHub 上 Windows exe 停在 v0.2.2（7/5，早于「极简白」重设计+云同步，48 个提交前），澄清=发布二进制滞后而非代码分叉（代码一套统一，iOS/Mac 已是当前新 UI）。因本机 Mac 无法交叉编译 Windows（§4.2 坑6），新增 `.github/workflows/release-desktop.yml`（tauri-action 多平台：windows-latest 出 NSIS exe + macos-latest 出 dmg，发布到 GitHub Release）。版本 bump 0.2.2→0.3.0（tauri.conf/package.json/Cargo.toml/Cargo.lock/iOS Info.plist），CHANGELOG 加 0.3.0。推 tag v0.3.0 一键触发**三端协调发布**：release-desktop（[Win exe 12.4MB + Mac dmg 16.2MB 发到 Release](https://github.com/Topia99/Shelf-Book-Reader/releases/tag/v0.3.0)）+ release-ios（TestFlight 0.3.0）全绿。**至此四端同版本 0.3.0，Windows 用户可下载到含新 UI+云同步的更新版**。以后发版一条 `git tag vX.Y.Z && git push origin vX.Y.Z` 即四端出包。
- **2026-07-22（macOS 构建验证 + DMG 上架 GitHub Releases + README 全平台改版）**：用户确认 iPad、macOS 均 Pass。① 重新 `npm run tauri build` 验证 macOS 版经历大量 iOS 改动后仍正常：产出 Shelf.app + Shelf_0.2.2_aarch64.dmg（15MB，ad-hoc 签名），open 启动进程存活不崩。② 把 DMG 上传到现有 v0.2.2 GitHub Release（用 keychain 存的 token 走 API，token 不入库/不外显；首次上传超时残留 starter 态资产，删后台重传成功 uploaded 16.2MB），并 PATCH release 标题→「Shelf v0.2.2 · Windows + macOS」、说明改多平台+macOS/Windows 安装步骤（含 ad-hoc 绕过 Gatekeeper 指引）。③ README 全平台改版：Win-only→四端、加下载表/云同步/触屏/多端构建命令。**版本卫生待办**：当前 code 仍 0.2.2 但已含 iOS/iPad/同步等远超旧 0.2.2 的功能，与旧 Windows 0.2.2 exe 同号易混，建议后续 bump 到 0.3.0 统一各端。macOS 正式公证发布=P1-7（现有 Apple 账号可解锁，未做）。
- **2026-07-22（P4-10 流水线增强：build 号自增 + GitHub Actions）**：① build 号自增——release 脚本用「自 2020-01-01 起分钟数」作单调整数传 `tauri ios build --build-number`，tauri 追加到版本得 CFBundleVersion `0.2.2.N`（实测 0.2.2.3448562 导出成功、> 首个 0.2.2 构建）；构建会把号写回被跟踪 Info.plist，脚本外重置回基线保持仓库干净（tauri 每次构建重新注入）。② GitHub Actions 发布工作流 `.github/workflows/release-ios.yml`：打 tag `v*` 或手动触发 → macOS runner → 从 4 个 repo secrets（ASC_KEY_ID/ASC_ISSUER_ID/ASC_TEAM_ID/ASC_KEY_P8_BASE64）还原 .p8+.env → 复用 release 脚本 build+upload。踩坑：release 脚本 echo 里 `$BUILD_NUM` 紧挨全角 `）` 致 bash 把多字节当变量名报 unbound，改 `${BUILD_NUM}` 花括号界定。**用户配好 4 个 repo secrets 后，推 tag v0.2.2-ci1 实测 CI 端到端全绿**（[run 29961220161](https://github.com/Topia99/Shelf-Book-Reader/actions/runs/29961220161)）——macOS runner 从 secrets 还原 .p8、云端签名、上传 TestFlight 全自动成功，TestFlight 现有 2 个构建（手动 0.2.2 + CI 自增号）。P4-10 关闭 ✅。以后发版一条命令 `git tag v0.2.3 && git push origin v0.2.3`。
- **2026-07-22（P4-10 TestFlight 首次上传成功）**：搭本地发布流水线并首次成功上传 TestFlight。新建脚手架：fastlane/{Appfile,Fastfile}（validate/beta lane，API 密钥认证）、tools/release-ios-testflight.sh（source .env.asc.local → tauri build 自动签名 → fastlane 上传）、.env.asc.local.example 模板、.gitignore 加忽略 *.p8。**新 Apple 账号 Team ID 变更 AZ3MB6QPUV→R59Q7NXAWD**，全量替换（tauri.conf.json + pbxproj）。踩坑串：① 上次 debugging 构建残留的 build/ExportOptions.plist 携旧 teamID 致导出用错 Team（删陈旧文件重生成）；② **xcodebuild 云端分发签名要求 ASC API 密钥 Admin 角色**——用户初建的 App Manager 报 `Cloud signing permission error / No profiles found`（查证 Apple 论坛确认），改用 Admin 密钥后 EXPORT SUCCEEDED；③ .p8 路径笔误（private_key→private_keys）修正；④ Homebrew fastlane 在 ruby 4.0 缺一批默认 gem（bigdecimal/digest-crc/nkf 等），批量补装；⑤ Fastfile IPA 相对路径受 cwd 影响，改 File.expand_path(__dir__)。产物 Shelf.ipa(17MB) 经用户确认后上传成功（App 6793296218，version/build 0.2.2）。加 Info.plist `ITSAppUsesNonExemptEncryption=false` 免后续加密合规询问（本次 0.2.2 可能仍需在 ASC 手动答一次）。上传流水线端到端完成（构建处理完毕、changelog 已设）。**M3 里程碑逼近**。凭证（.p8/.env.asc.local）全程未入库；CI 三 job 全绿（[run 29951436991](https://github.com/Topia99/Shelf-Book-Reader/actions/runs/29951436991)）。
- **2026-07-21（P4-8 dict.db 查词实测）**：验证 iOS 离线词典可用性。dict.db（24MB）随 bundle 打包在 `Shelf.app/assets/resources/dict.db`，`open_dict` 经 `BaseDirectory::Resource` 解析（无需首启拷贝到用户目录）。临时前端探针在 iOS 调 `lookup_word` 查四词全部命中：book→「n. 书，书籍」、reading→「n. 阅读，知识」精确命中；**ran→run 词形还原**（forms 表反查）；better→「a. 较好的」后缀/词形处理。证明 iOS 上 dict.db 解析+三级查询链路端到端工作。长按取词手势（caretRangeFromPoint）P4-4 已真机验证，故 P4-8 直接关闭 ✅。无源码改动（纯验证，dict.db 免拷贝方案成立）。
- **2026-07-21（P4-9 内存压测·模拟器初筛）**：生成 302MB/122 页扫描版 PDF（tools/gen-scan-pdf.py：A4@300dpi 位图+噪点纹理+段落黑条，无文字层）注入模拟器，临时 HMR 驱动自动翻 60 页+反复缩放（fit-width/fit-page/放大循环），`footprint` 每秒采样 Shelf 相关全进程 phys_footprint 之和（app 壳+WebContent+GPU+Networking，过滤宿主 WebKit 进程）。**结果：峰值 583MB<600MB ✓、全程无终止 ✓、压测后回落至 468MB 无泄漏 ✓（D9 渲染预算把 canvas/缩略图位图控住，内存不失控）**。关键洞察：峰值主导项是 pdf.js 加载时持有的 ~300MB 完整文件缓冲（在独立 WebContent 进程，非 Shelf 壳进程——壳进程恒 46MB）。踩坑：① WKWebView 网页内容跑在独立 WebContent 进程，测 app 壳 PID 只见 46MB 假象，须测 WebContent；② `pgrep -f WebKit` 会误抓宿主 Mac 浏览器进程（曾算出 6.4GB 假总和），须按 comm 路径含 CoreSimulator 过滤。**余量仅 ~17MB 且模拟器共享 Mac 大内存不代表 iOS jetsam 硬限，标 🔄 待真机 Instruments 权威复核**（真机跑法见 tools/gen-scan-pdf.py 头注）。无源码改动（压测通过，D9 预算无需调整）；CI 三 job 全绿（[run 29866431975](https://github.com/Topia99/Shelf-Book-Reader/actions/runs/29866431975)）。
- **2026-07-21（P4-7 生命周期持久化）**：修复 iOS 阅读中进后台丢进度的缺口。根因：`beforeunload` 在移动端 WKWebView 不触发（App 挂起而非页面卸载），翻页 1000ms 防抖内未落库的最新页码在进后台/被杀时丢失。修复=阅读控制器加 `visibilitychange(hidden)` + `pagehide` 监听 → 立即 `flushProgress`（同步 updateProgress 落本地库——本地是事实来源不会丢；updateProgress 内部再发 SyncNow 尽力 push，来不及则下次启动补推）。模拟器探针实证 WKWebView 进后台确触发 visibilitychange（启动 Safari 令 Shelf 进后台，hidden 计数 0→1）——P4-7 唯一平台特定前提已确定性验证，flush 逻辑复用已证路径。桌面端切窗口/最小化亦 flush，正向无害。tsc/vite build 通过；CI 三 job 全绿（[run 29863451747](https://github.com/Topia99/Shelf-Book-Reader/actions/runs/29863451747)）。P4-6 亦经用户真机确认分享导入通过，标 ✅。
- **2026-07-21（P4-6 分享/打开导入链路）**：实现「分享到 Shelf / 用 Shelf 打开」PDF 导入。方案（查 Tauri 2 官方文档定路径）：① `tauri.conf.json` 加 `bundle.fileAssociations`（ext=pdf/role=Viewer）→ 自动生成 iOS `CFBundleDocumentTypes`（LSItemContentTypes=com.adobe.pdf），实证生成正确；未声明 open-in-place → 系统把分享文件拷进 app Inbox（沙箱内，无需 security-scoped resource）。② lib.rs：`run()` 由 `.run(ctx)` 改为 `.build(ctx).run(|app,event|)` 捕获 `RunEvent::Opened`（macOS/iOS/Android 统一），只缓存 URL + emit `files-opened`，**不碰 DB 锁**；新增 `OpenedUrls` 缓存 + `take_opened_urls` 命令（冷启动兜底，前端就绪前事件投递不到）。Windows/Linux cfg 块编译不到，`let _ = (&app,&event)` 抑制未用参数告警（§4.2 坑6 镜像）。③ 前端：入库**全复用现有 `importPaths`**（复制/SHA-256 查重/封面/刷新，`resolve_input_path` 已支持 file://）——Library mount 取冷启动缓存 + listen `files-opened`。验证：tsc/clippy/40 单测/vite build 全绿；模拟器重建实证 Info.plist 文档类型 + 启动无回归。CI 首轮 Windows clippy 打回一次（`Emitter` import 只在 macOS/iOS/Android cfg 块用到、Windows 成未用 import——§4.2 坑6 镜像），移入 cfg 块内 `use` 后三 job 全绿（[run 29861665735](https://github.com/Topia99/Shelf-Book-Reader/actions/runs/29861665735)）。**真实分享 sheet 投递依赖系统交互，模拟器无 GUI 自动化，待用户真机确认**。
- **2026-07-21**：用户确认 iOS 阅读页满宽修复**真机测试通过** ✅。P4-10 TestFlight 流水线因 Apple Developer/ASC 账号仍在创建中**暂缓**，优先推进其他不依赖外部的开发任务。
- **2026-07-20（iOS 阅读页满宽修复）**：修复 iOS 阅读页书页左右留灰边、默认未铺满宽度的 bug（用户反馈）。根因：`usePdfReaderController` 计算 fit-width 时用 `parseFloat(stageStyle.paddingLeft) || 24` 读舞台内边距，而 iOS 舞台 side-space 为 **0px**，`0 || 24` 因 0 是 falsy 误取回退值 24 → 左右各误减 24px → fit-width 少铺满、书页缩小成带灰边的卡片。修复=改用 NaN 判定的 `px()` 助手（`Number.isFinite(n) ? n : fallback`），真实 0 被保留。模拟器屏上测量探针实证：修复前 `cW=393 canvasCSS=345px scale=0.580`（左右各 24px 灰边）→ 修复后 `canvasCSS=393px scale=0.661`（书页满宽铺满、文字延伸到两侧边缘）。桌面端舞台 padding 为真实 24px、gap 10px，`px()` 返回值与原 `||` 一致，行为零变化。tsc / vite build 通过；CI 三 job 全绿（[run 29788505798](https://github.com/Topia99/Shelf-Book-Reader/actions/runs/29788505798)）。
- **2026-07-20**：真机 UI 修复提交 CI 全绿（[run 29765495460](https://github.com/Topia99/Shelf-Book-Reader/actions/runs/29765495460)）。
- **2026-07-20**：真机 UI 两大 bug 修复收口，用户确认 iOS App 初步通过测试 ✅。① 书库封面粘连：根因为 WKWebView 的 grid 自动行高对「高度随宽度」的三种写法（aspect-ratio/百分比 padding/替换元素固有比例）全部计算错误（行高被錯定为 168px 而封面 239px 溢出盖住下行）——通过应用内测量探针实证定位，修复 = `grid-auto-rows: max-content` + 确定性尺寸（桌面固定 168px 列 + 235px 封面高，手机两列 + calc(50vw) 封面高）；② 阅读器缩放锚点：捏合缩放围绕屏幕中心（视口中心坐标按新旧 scale 换算回填 scroll），舞台尺寸从计算样式实读（不再硬编码 padding），iOS 壳默认沉浸（工具栏初始隐藏）；③ AccountPanel 错误文案中文化映射。主控代码审查一轮：无阻塞缺陷，两处小项记录在案（注册模式 unauthorized 文案不贴切、chrome 隐藏时按钮键盘可聚焦），README 个人绝对路径已泛化；tsc/vite build/clippy/40 单测全绿。新增 tools/seed-ios-test-book.sh（模拟器注数据调试脚本）。

- **2026-07-19**：真机安装链路 CLI 全程实测跑通并写入 [docs/真机安装测试步骤.md](docs/真机安装测试步骤.md)：`tauri ios build --export-method debugging` 产 IPA → `devicectl install` → 远程 launch 成功（iPhone 17 Pro Max，Shelf 0.2.2）。文档含一次性准备、三步日常安装、热调试模式、冒烟清单与 7 个实测过的故障排查项。

- **2026-07-13（P4-UI 收口，主控接管）**：应用户要求停用 Codex，主控接管 iOS 壳层改造。验收 Codex 的 Phase 1~3：结构拆分/平台路由/壳层 UI 全部合格。主控完成 Phase 4 缩略图 strip（修订 3 内存条款全数落实）。过程中修复三个问题：① Xcode 升级致 iOS 模拟器构建缓存指向 clang 16 旧路径（清缓存重建）；② main.tsx 真机调试期的全局错误兜底过于激进——任意游离异步错误（如 pdf.js Transport destroyed 良性竞态）会整页接管灭屏，改为仅首帧前接管 + 良性错误白名单；③ iOS 舞台顶距被文件尾部触屏媒体查询的 .page-stage 覆盖导致书页顶进刘海（提高特异性修复）。iPhone 模拟器实拍验收通过；tsc 零错误；macOS 启动冒烟零错误。

- **2026-07-13（真机 + iOS UI 计划）**：用户真机联调修复 5 个 bug（Vite 局域网 HMR、Xcode 脚本沙箱、file:// URL 导入、Team 签名残留、devicectl 安装链路，详见 fixedbug.md），主控本地复验 clippy/40 单测/tsc 全绿。iOS 真机（iPhone 14 Pro）已可运行。审核用户的 iOS 阅读器壳层改造计划（IOSUI_design.md）：方案通过，四处硬性修订（范围收窄至已有能力、iPad UA 伪装判定、缩略图内存预算、共享核心行为清单）写入 docs/IOSUI_验收标准.md；执行由用户直接分派 Codex，按验收标准逐条自查交付（看板挂 P4-UI-1~4）。

- **2026-07-13（P4-UI-1）**：按 iOS UI 验收标准启动 Phase 1 纯重构：新增 `src/components/reader/usePdfReaderController.ts`、`ReaderPageStage.tsx`、`ReaderDesktopShell.tsx`，把原 `Reader.tsx` 缩成密码/错误回退 + 壳层路由。当前仅做结构拆分，不改 CSS 与现有 DOM 语义；使用 bundled Node 运行 TypeScript 编译检查通过。剩余待验：桌面端人工回归清单与改造前后截图对比。

- **2026-07-13（P4-UI-2）**：完成平台路由代码：`src/platform.ts` 新增 `isIos`，按 `iPhone|iPad|iPod || (Macintosh && maxTouchPoints > 1)` 识别 iPadOS 伪装 UA；新增 `src/components/reader/ReaderIosShell.tsx` 作为独立 iOS 壳入口，当前先透明复用 `ReaderDesktopShell`，把视觉改动继续留在 Phase 3。前端 `tsc --noEmit` 与 `vite build` 均通过。剩余待验：iOS 模拟器进阅读页命中 `ReaderIosShell`、macOS 命中 `ReaderDesktopShell`。

- **2026-07-13（P4-UI-3）**：按 `IOSUI_design.md` 与 `docs/IOSUI_验收标准.md` 实装 iOS 阅读器新壳：`ReaderIosShell` 改为独立悬浮式布局，左 pill（返回/目录）+ 右 pill（适宽/整页），页码 badge 独立浮层，页面中央点击复用现有手势隐藏工具栏，舞台背景改柔和浅灰；`ReaderPageStage` 增加可注入命名空间 class，所有新增样式都收在 `.reader-ios-*` 下；iOS 默认缩放收口为 `fit-width`。前端 `tsc --noEmit` 与 `vite build` 全通过。剩余待验：模拟器截图、横竖屏旋转、取词/目录/进度保存实操。

- **2026-07-13（UI 重设计）**：应用户要求主控完成「极简白」全面改版（参照 Apple Books）：首页只留书——大标题 + ＋/⋯ 两枚小圆钮，搜索（按需展开行）/排序（菜单打勾）/账号登录全部收进 ⋯ 菜单；书卡去标题只留封面 + 元信息行（进度% + 云图标 + ⋯）；全局改纯白主调 + 系统灰控件，赭色降级为点缀（选中态/CTA/同步指示）；保留 Shelf 签名元素（书脊高光、衬线空状态标题与词头、书架插画）。iPhone 模拟器截屏验收通过；tsc 零错误。

- **2026-07-12（阶段4开工）**：环境（iOS Rust 目标/CocoaPods/模拟器）→ tauri ios init → iPhone 15 Pro 模拟器实跑成功，截屏验收：UI 完整、P4-5 移动布局经热更新同屏生效、同步引擎启动（钥匙串空=正确初态）、数据/资源容器路径全部正确。排障：模拟器 Shutdown 态需先 boot 再 deploy。清理了遗留的 functions serve 常驻任务；确认 connection problem 为客户端网络问题，工作状态已全部落盘可无损续接。

- **2026-07-13（深夜）**：M2 首轮真人联调（用户，双 Mac）：跨设备书目同步实测成功。暴露体验问题：远端书（文件未下载，阶段 5 范围）点击打开报 asset 403 裸错误。当轮修复：Book 暴露 cloud_state，书卡加"云端"角标，打开远端书改为友好提示。进度互通的验证方法：两端导入同一 PDF（同哈希自动对上），翻页后另一端进度应跟随。

- **2026-07-13（晚）**：P3-7 批次 CI 三 job 全绿（[run 29206966458](https://github.com/Topia99/Shelf-Book-Reader/actions/runs/29206966458)）。
- **2026-07-13（晚）**：P3-7 账号 UI 交付验收通过。引擎无头端到端实测通过：把本地栈会话写入 macOS 钥匙串 + 本地埋脏书 + 云端埋远端书 → 启动应用 → 引擎自动恢复会话、刷新、完成双向同步（推送/拉取/状态转换全部正确）。排障两轮：钥匙串 ACL（security CLI 写入需 -A）与 GoTrue refresh token 一次性轮换（手动测试消耗了存储的 token）——均为测试操作问题，引擎本身零缺陷。已知待优化：小库场景 next_cursor 不推进导致每轮全量拉取（幂等无害，规模化前修）。云端邮箱验证关闭需用户在 Dashboard 操作或跑 supabase config push（权限策略拦截了自动执行）。

- **2026-07-13（下午）**：P2-7 打回一次（DOMParser 不存在于 Edge Runtime）修复后端到端全绿并部署上云——失败重试语义顺带实测成立。P3-3 钥匙串存储验收通过。P3-6 主控完成：同步引擎专用线程接线（防抖/心跳/退避/会话恢复），update_progress 挂接翻页触发，6 个账号同步命令注册；40 测试 + clippy + tsc 全绿。P3-7 账号 UI 分派中。

- **2026-07-13**：同步核心提交 CI 三 job 全绿（[run 29205849927](https://github.com/Topia99/Shelf-Book-Reader/actions/runs/29205849927)）。
- **2026-07-13**：同步核心三连交付收口。P2-6 sign-url 本地栈端到端全绿（401/403/413/200 + 真实 R2 往返 + 配额精确入账）并已部署云端。P3-2 打回一次：单测抓到手写闰日转换差一天、clippy 抓到 `if a { None } else { None }` 逻辑笔误，返工修复。P3-4 主控审查抓到关键缺陷：本地 LWW 胜出时把 synced_at 一并推进，导致本地新进度被标记已同步、云端永远收不到——修正为"只有采纳远端值才推进 synced_at，本地胜出保持脏状态等下轮 push"。全库 39 测试绿、clippy 零警告。

- **2026-07-12（晚）**：云端线提交 CI 三 job 全绿（[run 29204895532](https://github.com/Topia99/Shelf-Book-Reader/actions/runs/29204895532)）；随即三窗并发派出 P2-6/P3-2/P3-4+5。
- **2026-07-12（晚）**：云端线贯通。四项外部依赖全解锁（Supabase login/Docker/Xcode Apple ID/R2 凭证）。R2 签名请求写读删往返验证通过（P2-5 ✅）。P2-2 schema 在本地栈实跑抓到真缺陷：只写了 RLS 策略没写表级 GRANT，authenticated 角色无表权限——补授权后 RLS 测试 7 断言全绿（P2-3 ✅），db push 上云成功，远端 migration 一致。P3-1 SyncBackend trait + 云端模型交付验收通过。教训入库：RLS 是行过滤器，表级授权是另一层，两者缺一不可；本地栈实跑是 schema 的"CI 实跑"。

- **2026-07-12**：P2-8/P4-3 提交 CI 三 job 全绿（[run 29203652757](https://github.com/Topia99/Shelf-Book-Reader/actions/runs/29203652757)）。
- **2026-07-12**：H-1 ✅（Apple Developer）、Supabase 账号 ✅（CLI 已装待 login）。并行交付：P2-8（本地库 v2 迁移，单测 14→18）、P4-3（渲染预算）均一次验收通过；P4-2 按设计满足关闭（翻页式阅读器无滚动列表）。环境盘点：Xcode 16.2 ✅、Docker 装了但守护进程未启动、Cloudflare R2 未注册（下一个外部瓶颈）。
- **2026-07-11**：阶段 1 提交 CI 实跑三 job 全绿（[run 29163138657](https://github.com/Topia99/Shelf-Book-Reader/actions/runs/29163138657)）。
- **2026-07-11**：阶段 1 开发完成。P1-1：macOS tauri dev 一次跑通（进程/窗口/数据目录/WAL/零错误日志），无需任何代码改动——阶段 0 的跨平台整备直接兑现。P1-3/P1-4 缩容关闭（Tauri 2 默认菜单栏与原生窗口装饰已覆盖需求）。P1-5 Codex 交付捏合缩放，tsc 打回一次主控修正。P1-6 审计关闭。P1-2 产出 Shelf.app + dmg（arm64，ad-hoc 签名）。待用户：M1 人工走查（导入/阅读/捏合缩放/菜单/dmg 安装）；H-1/H-2 注册后解锁 P1-7/P1-8。

- **2026-07-10（下午）**：P0-1 返工 CI 实跑通过：Frontend / Rust-Windows / Rust-macOS 三 job 全绿（[run 29142726574](https://github.com/Topia99/Shelf-Book-Reader/actions/runs/29142726574)），macOS 侧 migrate_old_base 的 dead_code 处理如预期。阶段 0 仅剩容器外人工 UI 回归。
- **2026-07-10（下午）**：P0-1 返工完成。方案：① books 表 file_path/cover_path 改存相对路径（'/' 分隔），后端 to_abs/absolutize_book 在返回前端和碰文件系统前拼回绝对路径，前端零改动；② init_db 里 normalize_book_paths 把存量绝对路径按文件名幂等改写为相对（入库布局固定为 books/<hash>.pdf，安全）；③ 迁移逻辑抽成纯函数 migrate_old_base：判据改"哪边有 library.db"，新目录空骨架先清再整体原子 rename，新目录有真实文件但无库/rename 失败等异常一律回退用旧目录。clippy -D warnings 零警告；单测 6→14（迁移五分支 + 改写幂等/NULL 封面 + to_abs）；本机 tauri dev 实跑确认存量库启动后被改写为相对路径。待办：容器外人工 UI 回归。
- **2026-07-10**：Windows 真机回归（P0 阶段验收）。静态检查全绿：tsc ✅、cargo clippy -D warnings ✅（CI 首轮修复在本地 Windows 复验通过）、cargo test 6/6 ✅。**发现 P0-1 阻塞缺陷**：本机真实 library.db 中 file_path/cover_path 均为 `%APPDATA%\Shelf\...` 绝对路径，迁移重命名目录后无任何代码改写这些行 → 升级老用户封面全断、书打不开。P0-1 状态改 ⛔ 打回。次要发现：① 迁移条件 `!new_base.exists()` 遇到预先存在的新目录会静默跳过迁移（本机即复现：残留的 7/4 早期 com.shelf.reader 目录导致 dev 启动读到旧快照书库）；② asset 协议 scope 因 setup 里运行时 allow_directory 双目录，不受迁移落点影响，无风险。测试环境备注：本次会话运行于 Claude 桌面 MSIX 容器内，AppData 被虚拟化，`tauri dev` 的手动 UI 回归（导入/阅读/取词）无法在本会话内代表真实用户环境，修复后需在普通终端/双击安装版下人工过一遍。用户真实数据（%APPDATA%\Shelf，4 本书）未受影响，已另备份。

- **2026-07-08**：CI 首轮实跑：Frontend ✅、Rust-macOS ✅、Rust-Windows ❌（P0-1 的 cfg(windows) 迁移函数触发 clippy collapsible_if，本地 Mac 编译不到该分支所以未暴露——印证了"必须 GitHub 实跑"的要求）。已用最小复现 crate 定位并修复，二轮 CI 验证中。
- **2026-07-08**：阶段 0 全部完成。5 个 Codex 窗口两波并行交付（波1：P0-1/P0-2/P0-3；波2：P0-4/P0-5），全部一次验收通过。发现并修复关键坑：Windows 老用户数据目录迁移（%APPDATA%\Shelf → com.shelf.reader）。CI 推送后待 GitHub 实跑验证 Windows 编译。
- **2026-07-08**：《全平台开发文档》《implementation_plan.md》定稿入库；Codex CLI 分派通道验证可用。
