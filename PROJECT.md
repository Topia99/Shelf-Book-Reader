# PROJECT.md — Shelf 全平台阅读器 · 项目主文档

> 本文件是项目的唯一交接入口：新会话仅凭此文件即可完全接手。
> 细节文档索引：[全平台开发文档.md](全平台开发文档.md)（架构/成本设计）、[implementation_plan.md](implementation_plan.md)（原子任务定义与验收标准）、[progress.md](progress.md)（实时看板，**开工前必读**）、[docs/IOSUI_验收标准.md](docs/IOSUI_验收标准.md)、[docs/真机安装测试步骤.md](docs/真机安装测试步骤.md)、[docs/跨平台审计清单.md](docs/跨平台审计清单.md)、[fixedbug.md](fixedbug.md)（真机联调 bug 修复记录）。
> 最后更新：2026-07-20

---

## 1. 项目概述

### 1.1 目标与背景

Shelf 原本是一款 Windows 11 本地 PDF 书库阅读器（Tauri 2，v0.2.2：复制入库/SHA-256 查重/封面书架/进度记忆/离线取词词典）。所有者 Jason（GitHub: Topia99）希望把它做成**全平台阅读软件**：在 iPad、iPhone、Mac、Windows 上随时接着读同一本书，并最终上架 App Store 与 Microsoft Store 分发给其他用户。

最终交付物：
1. Windows / macOS / iOS(iPhone+iPad) 三端同一套代码的应用（Android 留待后续）
2. 账号系统 + 云端同步（书目、阅读进度先行；文件本体同步在阶段 5）
3. TestFlight 公测渠道 → App Store / Microsoft Store 上架

### 1.2 产品原则（不可推翻的决策，见 §4.1）

- **本地优先，账号可选**：不登录 = 完整单机应用；登录才解锁同步。审核员不注册也能体验核心功能。
- **个人云书房，不做书籍分享**：用户间无任何互传通道，避免 UGC 审核与版权风险。
- **一套代码四端**：继续 Tauri 2，不换 PDF 内核（pdf.js），不做原生重写。

### 1.3 技术选型与原因

| 层 | 选型 | 版本 | 原因 |
|---|---|---|---|
| 应用框架 | Tauri 2 | 2.x | 原有栈；官方支持 Win/macOS/iOS/Android，前端 90% 复用 |
| 前端 | React + TypeScript + Vite | 18.3 / 5.6 / 6 | 沿用 |
| PDF 渲染 | pdfjs-dist | 4.10 | 沿用；iOS 上以渲染预算控内存（见 §4.2-坑7） |
| 本地库 | rusqlite (bundled) | 0.32 | 沿用；WAL 模式支持 UI/同步引擎双连接 |
| 云认证+库 | Supabase (GoTrue + Postgres + RLS) | — | 免运维；RLS 行级隔离；标准 Postgres 无锁定 |
| 文件存储 | Cloudflare R2 | — | **出口流量 $0** 是决定性理由（PDF 大文件）；预签名 URL 直传 |
| HTTP(Rust) | reqwest blocking | 0.12 | 同步引擎跑专用线程，阻塞客户端最简单（**严禁在 tauri 异步运行时线程调用**） |
| 会话存储 | keyring | 3 | 系统钥匙串（macOS Keychain/Win 凭据管理器/iOS 同栈） |
| CI | GitHub Actions | — | PR/main 触发：tsc + vite build + clippy -D warnings + cargo test × (windows-latest, macos-latest) |

### 1.4 模块划分与目录结构

```
src/                          # React 前端（四端共享）
  App.tsx                     # 书库/阅读器路由（薄）
  main.tsx                    # 挂载 + 启动错误面板（仅首帧前接管 + pdf.js 良性错误白名单）
  api.ts                      # Tauri invoke 类型化封装（含 6 个 sync_* 命令 + SyncStatus 类型）
  platform.ts                 # isMac / isIos(含 iPad 伪装判定) / isTouchDevice / isModKey
  pdf.ts  dict.ts             # pdf.js 封装 / 取词词典
  styles.css                  # 全部样式。「极简白」体系 + .reader-ios-* 命名空间（见 §4.1-D6/D7）
  components/
    Library.tsx               # 书库页：大标题 + ＋/⋯ 圆钮；搜索/排序/账号收进 ⋯ 菜单
    BookCover.tsx  AccountPanel.tsx  WordPopup.tsx
    reader/
      usePdfReaderController.ts  # 共享阅读核心（加载/翻页/缩放+锚点/手势/取词/进度）≈800 行
      ReaderPageStage.tsx        # 共享渲染舞台（可注入 classNames 命名空间）
      ReaderDesktopShell.tsx     # 桌面壳（原工具栏布局）
      ReaderIosShell.tsx         # iOS 壳（悬浮 pill/页码 badge/沉浸显隐）
      ReaderThumbnailStrip.tsx   # iOS 底部缩略图胶片（内存预算硬约束，见 §4.1-D8）
src-tauri/
  tauri.conf.json             # identifier com.shelf.reader；targets nsis/app/dmg；iOS developmentTeam R59Q7NXAWD
  src/
    lib.rs                    # 书库 CRUD 命令、resolve_base_dir(Windows 数据迁移)、SQLite 迁移(user_version=2)
    sync.rs                   # SyncBackend trait + 云端模型（可插拔后端）
    sync_supabase.rs          # GoTrue/PostgREST/sign-url 阻塞实现（毫秒↔RFC3339 自实现）
    sync_engine.rs            # 纯逻辑：collect_dirty/mark_synced/LWW 合并/墓碑/游标（15 个单测覆盖）
    sync_runtime.rs           # 引擎线程：命令通道/5s 防抖+30s 最小间隔/5min 心跳/指数退避/钥匙串恢复
    token_store.rs            # keyring 封装（service = com.shelf.reader.sync）
  gen/apple/                  # tauri ios init 产物（shelf.xcodeproj）
supabase/
  migrations/20260712000001_init_sync_schema.sql  # books/reading_progress/user_quota + RLS + 表级 GRANT + 触发器
  functions/sign-url/          # R2 预签名 + 配额检查（401/403/413 协议见文件头注释）
  functions/delete-account/    # R2 前缀→DB 行→auth 用户三级级联删除（正则解析 S3 XML）
  tests/rls_isolation_test.sql # 7 断言 RLS 隔离测试（psql 执行）
.github/workflows/ci.yml
tools/seed-ios-test-book.sh   # 模拟器注入测试书（调试用）
```

### 1.5 同步架构（核心设计）

- **书籍身份 = 文件 SHA-256**（跨设备天然对齐，复用原查重逻辑）
- **本地 SQLite 是唯一事实来源**；`sync_runtime` 专用线程后台追平（push 脏行 → pull 增量合并）
- 脏行判定 `updated_at > synced_at`；合并 LWW + 墓碑（`deleted`），进度冲突 2 分钟窗口内取大页码
- 本地 books 表：`hash/title/file_path(相对路径!)/current_page/updated_at/deleted/synced_at/cloud_state(local|remote)`
- 云端三表带 `server_updated_at` 服务器游标（增量拉取索引）；`user_quota` 由新用户触发器自动建行
- 已知待办：`next_cursor` 只在整页拉满时推进 → 小库每轮全量重拉（幂等无害，阶段 5 优化）

---

## 2. 执行计划

> 权威状态在 [progress.md](progress.md)（每完成一项即时更新并随码提交）。下表为接手速览。

### 2.1 阶段与状态总览

| 阶段 | 内容 | 状态 |
|---|---|---|
| 0 项目整备 | 跨平台假设清理 + CI | ✅ 2026-07-08 |
| 1 macOS 版 | dev 跑通/打包/手势/快捷键 | ✅（P1-7 签名流水线、P1-8 自动更新 ⬜ 被 H-2 阻塞） |
| 2 云服务 MVP | schema/RLS/R2/Edge Functions | ✅（P2-4 Auth 三渠道 ⬜、P2-9 集成测试骨架 ⬜） |
| 3 账号+进度同步 | trait/实现/引擎/UI | ✅（P3-8 M2 双端联调 🔄：书目同步已实证，进度互通待用户两端同书验证） |
| 4 iOS/iPadOS | 工程/触屏/壳层改造/真机 | 🔄 **当前主战场**（细分见下） |
| 5 文件同步 | 上传队列/云书架/按需下载 | ⬜ |
| 6 上架 | 官网/审核材料/三商店 | ⬜ |
| 7 商业化 | IAP/桌面支付/配额 | ⬜（可选） |

### 2.2 阶段 4 细分（当前）

✅ P4-1 工程+模拟器 ｜ ✅ P4-2 虚拟化(按设计免做) ｜ ✅ P4-3 渲染预算 ｜ ✅ P4-4 触屏手势 ｜ ✅ P4-5 移动布局 ｜ ✅ P4-UI-1~4 iOS 阅读壳（pill/badge/沉浸/缩略图 strip）｜ ✅ 真机直装链路（docs/真机安装测试步骤.md）｜ ✅ 真机 UI bug 修复（网格行高/缩放锚点），**用户已确认 iOS App 初步通过**

⬜ 待做：P4-6 文件导入完整链路（Info.plist 文档类型声明 +「分享到 Shelf」）｜ P4-7 生命周期持久化（进后台立即 flush+push）｜ 🔄 P4-8 dict.db 验证（或可免做，待真机查词实测关闭）｜ P4-9 内存压测（300MB 扫描版，峰值 <600MB）｜ P4-10 TestFlight 流水线 ｜ P4-11 公测（M3 里程碑）

### 2.3 里程碑

M1 Mac 版可用 🔄（开发完，待用户人工走查 12 项清单：docs/IOSUI_验收标准.md §三）｜ M2 双端接着读 🔄 ｜ M3 TestFlight 可分享 ⬜ ｜ M4 商店上线 ⬜

### 2.4 当前进度与下一步

**刚完成**：真机 UI 两大 bug 修复（CI 全绿 run 29765495460），iOS 初步通过用户测试。

**下一步（按价值排序）**：
1. **P4-10 TestFlight 流水线**：需要用户在 App Store Connect ① My Apps → ＋ 新建 App（Bundle ID `com.shelf.reader`，若下拉无此 ID 先用 Xcode 注册）② Users and Access → Integrations → App Store Connect API → 生成 App Manager 角色密钥，`.p8` 存本地（如 `~/.appstoreconnect/`，**不进对话不进仓库**），Key ID/Issuer ID 填入 `.env.asc.local`（gitignored）。然后：本地 fastlane pilot 上传跑通 → 再迁 GitHub Actions（证书导出为 Secrets）。
2. P4-9 内存压测：生成 300MB 扫描版 PDF 注入真机/模拟器，Instruments 或 `simctl spawn log` 观测。
3. M2 复验：用户在 Win/Mac 两端导入同一 PDF 验证进度互通。
4. 外部依赖清欠：H-2 Azure Trusted Signing（Windows 签名）、H-4 域名（官网）。

### 2.5 关键依赖链

P4-10 → P4-11(M3) → P6-2 → P6-5(M4)；P1-7 ← H-2；P5-* ← P2-6(已好)+P3-6(已好)，可随时开工；P6-1 ← H-4。

---

## 3. 工作流 / Skills

> 本项目未注册自定义 Claude skill；以下是文档化的工作协议，新会话直接照做。

### 3.1 Codex 分派工作流（存在但当前停用）

- 用途：把 implementation_plan.md 标 `[派]` 的原子任务交给 Codex CLI 执行，Claude 主控验收。
- 触发：`/Users/jasonzeng/.nvm/versions/node/v24.14.0/bin/codex exec --sandbox workspace-write "$(cat 提示词文件)"`（后台并行多窗，每窗单一职责、文件不相交；提示词含：允许改动文件白名单、禁止 git commit、验收标准）。
- 四步验收（缺一不可）：① git diff 逐行审查不越界 ② `cargo clippy -D warnings`+`cargo test` / `npx tsc --noEmit` ③ 任务专属验收标准 ④ 不达标附原因打回，两轮不过主控收回。
- **状态：2026-07-13 起用户明确要求停用 Codex，全部主控直做**。恢复需用户指示。

### 3.2 进度看板闭环（强制）

每完成/打回一项任务，**同一轮工作内**更新 progress.md（状态行 + 底部执行日志倒序追加），随代码一起 commit。CI 类验证必须等 GitHub 实跑绿灯才标 ✅。闭环：全平台开发文档(设计) → implementation_plan(拆解) → 执行 → progress.md(追踪)。

### 3.3 CI 实跑验证（每次 push 后必做）

本地绿不算数（教训：Windows-only clippy 分支、mac 编译不到 cfg(windows)）。gh CLI 未装且匿名 API 拿不到 job 日志（403，需仓库管理员权限），用匿名轮询确认结论、失败时本地复现定位：

```bash
SHA=$(git rev-parse HEAD)
curl -s "https://api.github.com/repos/Topia99/Shelf-Book-Reader/actions/runs?head_sha=$SHA"
# → workflow_runs[0].status/conclusion；jobs: /actions/runs/<id>/jobs
# 轮询用 curl 落盘 + python 读文件解析（JSON 经 shell 变量会被控制字符破坏——踩过）
```

### 3.4 iOS 模拟器验证流水线

```bash
# 启动（模拟器 iPhone 15 Pro，UDID 911AA2E1-084E-438F-83A5-FA19473D9B97）
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:$PATH"; export LANG=en_US.UTF-8
npm run tauri ios dev "iPhone 15 Pro"   # 后台跑；端口 1420 被占先 lsof -ti :1420 | xargs kill -9
# 注入测试书（应用先启动一次建库）
bash tools/seed-ios-test-book.sh
# 截屏验收
xcrun simctl launch <UDID> com.shelf.reader && sleep 10
xcrun simctl io <UDID> screenshot /tmp/shot.png
```

**⚠️ 前端改动在模拟器不生效时**：WKWebView 磁盘缓存顽固 + 曾有从内嵌 dist 加载的形态。标准操作：`npm run build` 重建 dist → `xcrun simctl uninstall <UDID> com.shelf.reader` → 重跑 `tauri ios dev`。改动是否到达用「标题文字探针 + CSS 红色探针」二分判定（JS 到了 CSS 没到 ≠ 缓存，是规则被引擎另类解释）。

### 3.5 真机安装 / TestFlight 前置

见 docs/真机安装测试步骤.md（全流程 2026-07-19 实测）。要点：全新环境首次部署必须走一次 Xcode GUI（命令行拿不到账号会话）；日常三步 `tauri ios build --export-method debugging` → `devicectl install` → `launch`；开发签名 7 天过期。

### 3.6 Supabase 本地栈测试

```bash
supabase start          # 需 Docker Desktop 运行；首次拉镜像 ~2GB
supabase db reset       # 重放 migrations
docker exec -i supabase_db_shelf-book-reader psql -U postgres -d postgres -v ON_ERROR_STOP=1 \
  < supabase/tests/rls_isolation_test.sql   # 期望输出：RLS 隔离测试全部通过
supabase functions serve <fn> --env-file <含 R2 凭证的 env>   # Edge Function 本地实测
supabase db push / functions deploy <fn>    # 上云（已 login + link）
```

引擎端到端（无 UI）验法：本地栈注册用户 → 会话 JSON 写钥匙串（`security add-generic-password -s com.shelf.reader.sync -a session -A -w '<json>'`）→ 本地库埋脏行/云端埋远端行 → `SHELF_SUPABASE_URL=http://127.0.0.1:54321 SHELF_SUPABASE_ANON_KEY=<本地anon> npm run tauri dev` → 双向核对。**注意 GoTrue refresh token 一次一换**（§4.2-坑9）。

### 3.7 待建 Skills（建议，未实施）

- `release-desktop`：tag → macOS 公证 DMG + Windows 签名 NSIS + GitHub Release（对应 P1-7，规格见 implementation_plan.md）
- `release-ios`：`tauri ios build`(export app-store-connect) → fastlane pilot 上传（对应 P4-10；需 `.env.asc.local` 的 ASC_KEY_ID/ASC_ISSUER_ID/ASC_KEY_PATH）

---

## 4. 关键信息

### 4.1 已定决策（禁止推翻）

- D1 本地优先账号可选；D2 永不做用户间书籍分享；D3 书籍 ID=SHA-256；D4 同步=LWW+墓碑+2min 窗口取大页码，不上 CRDT
- D5 云端栈=Supabase+R2；anon key 是公开凭证可入库（安全边界在 RLS），service key/R2 凭证**绝不入库**
- D6 UI=「极简白」：白底、系统灰控件、赭色 `#b45327` 仅点缀；首页只留书，控件收进 ⋯ 菜单；书卡无标题只有封面+元信息行
- D7 iOS 阅读器走独立壳层（`.reader-ios-*` 命名空间），桌面端阅读器视觉零改动；共享层只有 controller+stage
- D8 缩略图内存条款（docs/IOSUI_验收标准.md 修订 3）：≤150 物理 px、渲染即 toDataURL(jpeg,0.7) 销毁 canvas、24 张 LRU、主渲染优先
- D9 渲染预算：dpr≤2 + 单 canvas 1600 万像素上限（改 Reader 渲染的 PR 不得突破）
- D10 本地 DB 存**相对路径**（`books/<hash>.pdf`），碰文件系统/返前端时拼绝对（iOS 容器路径会变 + Windows 迁移教训）
- D11 书库网格用**确定性尺寸**（桌面 168px 列+235px 封面；手机两列+`calc((50vw-26px)*1.4)`）+ `grid-auto-rows: max-content`，不得改回任何「高度随宽度推导」写法（§4.2-坑1）

### 4.2 踩过的坑（新会话必读，按杀伤力排序）

1. **WKWebView grid 行高**：aspect-ratio / 百分比 padding / 替换元素固有比例三种写法在 grid 自动行高里全部失算（封面溢出盖住下行或塌陷）。桌面引擎复现不了；定位手段=应用内测量探针（把 getComputedStyle 画到屏幕上截图）。解法见 D11。
2. **模拟器改动不生效**：见 §3.4 ⚠️。三次「修复无效」其实是缓存；先证明改动到达（双探针），再怀疑逻辑。
3. **RLS ≠ 授权**：只写 policy 不写表级 GRANT，authenticated 角色连表都摸不到。migration 里两者都要有。本地栈实跑是 schema 的"CI"。
4. **Edge Runtime 无 DOMParser**：浏览器 API 在 Deno Edge 不存在，S3 XML 用正则解析。
5. **Windows 数据迁移双坑**：%APPDATA%\Shelf→com.shelf.reader 迁移后 DB 里的绝对路径悬空（→D10 相对路径 + init_db 幂等改写）；迁移判据 `!new_base.exists()` 遇残留空目录静默跳过（→判据改"哪边有 library.db"+空骨架清除+异常回退旧目录）。
6. **cfg(windows) 分支 mac 上编译不到**：clippy 本地绿、CI Windows 红（collapsible_if）。交叉 `cargo check --target x86_64-pc-windows-msvc` 因 C 依赖不可行；用最小复现 crate 验证 lint，CI 兜底。
7. **iOS 内存**：WKWebView 超限即杀。预算见 D9；页面位图 = 宽×高×4B 心算校验。
8. **keyring/钥匙串**：`security` CLI 写入的条目默认 ACL 只允创建者，测试注入要加 `-A`；应用自身读写无此问题。
9. **GoTrue refresh token 旋转**：一次一换，手动 curl 刷新会作废存储的 token（引擎会正常轮换，测试时别抢跑）。
10. **SyncEngine 曾有的正确性缺陷**（已修，改动时警惕回归）：本地 LWW 胜出时误推进 synced_at → 本地值永不上推。原则「只有采纳远端值才变干净」；mark_synced 用行自身 updated_at 而非 now。
11. **Xcode 升级后 iOS 构建炸**：缓存指向旧 clang 路径（`clang_rt.iossim not found`）→ `rm -rf src-tauri/target/aarch64-apple-ios*`。
12. **zsh**：`UID` 是只读内置变量，脚本变量换名（TUID）。
13. **JSON 过 shell 变量会坏**（控制字符）→ curl 落盘、python 读文件。
14. **全局错误兜底会灭屏**：任意 unhandledrejection 整页接管的写法把 pdf.js 良性取消竞态（Transport destroyed）当致命错误。现 main.tsx 只在首帧前接管 + 良性白名单，勿回退。
15. **git add -A 扫进构建垃圾**：.xcode-derived-data 曾误入库（已 gitignore）。提交前看 status。
16. **tauri ios dev 端口残留**：vite 1420 被占 → `lsof -ti :1420 | xargs kill -9`。

### 4.3 环境与外部依赖（本机 = Jason 的 MacBook）

| 项 | 值 |
|---|---|
| 仓库 | https://github.com/Topia99/Shelf-Book-Reader （本地 /Users/jasonzeng/Developer/shelf-book-reader，主分支 main 直推） |
| Supabase | 项目 ref `dyhpapzyyuxlqpqupsfo`（us-east-1，用作 dev；prod 名额留上线前）；CLI 已 login+link；anon key 内嵌在 src-tauri/src/sync_runtime.rs |
| Cloudflare R2 | bucket `shelf-book-storage`；凭证在 `.env.r2.local`（被 .gitignore 第 5 行 `*.local` 覆盖，含 R2_ACCOUNT_ID/R2_ACCESS_KEY_ID/R2_SECRET_ACCESS_KEY/R2_BUCKET）；云端函数 secrets 已配 |
| Apple | **Team ID `R59Q7NXAWD`**（新付费账号，2026-07-22 起；旧 `AZ3MB6QPUV` 已弃用）；设备：iPhone 17 Pro Max、iPhone 14 Pro、iPad Pro 11"。**TestFlight 上传**：ASC API 密钥须 **Admin 角色**（App Manager 云端签名报权限错），凭证在 `.env.asc.local`(gitignored)，.p8 存 `~/appstoreconnect/private_keys/`（仓库外）；流水线 `tools/release-ios-testflight.sh`；App Store Connect App ID `6793296218` |
| Edge Functions | sign-url、delete-account 已部署云端 |
| 工具链 | rustup stable（~/.cargo）、node v24.14.0（nvm）、supabase CLI 2.x（brew）、CocoaPods、Docker Desktop（本地栈需手动启动）、codex-cli 0.144.1（已登录、当前停用） |
| 未就绪 | gh CLI（未装，CI 日志拿不到）、Azure Trusted Signing（H-2）、域名（H-4）｜已就绪：App Store Connect API 密钥（Admin，P4-10 首次上传成功） |

### 4.4 常用命令速查

```bash
npx tsc --noEmit                                        # 前端类型检查
npm run build                                           # 前端构建（iOS 验证前必跑，见 §3.4）
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml         # 当前 40 个单测
npm run tauri dev                                       # macOS 桌面开发
npm run tauri ios dev "iPhone 15 Pro"                   # 模拟器
npm run tauri ios build -- --export-method debugging    # 真机 IPA → src-tauri/gen/apple/build/arm64/Shelf.ipa
xcrun devicectl list devices / device install app / device process launch
git push 后：§3.3 轮询 CI；绿灯 → progress.md 记录
```

提交规范：中文信息，尾行 `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`；main 直推（无 PR 流程）；每逻辑批次一 commit，看板同 commit 或紧随。

### 4.5 自查（新会话接手清单)

1. 读本文件 → 读 progress.md 头部「当前阶段」与执行日志前三条
2. `git log --oneline -5` + `git status` 确认工作区干净
3. 若做 iOS：先过 §3.4 的缓存注意事项；若做云端：`supabase start` 前开 Docker Desktop
4. 动手前确认任务在 §2.4「下一步」清单内或用户新指示；完成后按 §3.2 闭环更新看板
