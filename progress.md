# Shelf 全平台进度看板（progress.md）

> 全局唯一进度事实来源。每完成一项任务即时更新本文件。
> 任务定义与验收标准见 [implementation_plan.md](implementation_plan.md)，架构与决策见 [全平台开发文档.md](全平台开发文档.md)。
> 状态：✅ 完成（已验收）｜🔄 进行中｜⏳ 待开始｜🚫 被依赖阻塞｜⛔ 验收打回

**最后更新**：2026-07-12
**当前阶段**：阶段 2 主体推进中——云端 schema 已上线（P2-1/2/3/5/8 ✅）、SyncBackend trait 就位（P3-1 ✅）；下一步：P2-6 预签名 Edge Function、P3-2 Supabase 后端实现、P2-4 Auth 配置
**里程碑**：M1 Mac 版可用 🔄（开发完，待人工走查）｜M2 双端接着读 ⏳｜M3 TestFlight 可分享 ⏳｜M4 商店上线 ⏳

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
| P2-6 | Edge Function：预签名 + 配额 | 🔄 | Codex 分派中（JWT 鉴权/键前缀隔离/配额检查/aws4fetch 预签名） |
| P2-7 | Edge Function：删除账号级联 | 🚫 | P2-6 |
| P2-8 | 本地 SQLite v2 迁移（同步字段） | ✅ | user_version=2 迁移/updated_at 写入钩子/墓碑删除/同 hash 复活；clippy 零警告，单测 14→18 全绿 |
| P2-9 | 集成测试骨架 | 🚫 | P2-3, P2-6 |

## 阶段 3：账号 + 进度同步

| ID | 任务 | 状态 | 依赖 |
|---|---|---|---|
| P3-1 | SyncBackend trait + 数据模型 | ✅ | src-tauri/src/sync.rs：5 模型 + SyncError + 10 方法 trait；clippy 零警告 |
| P3-2 | Supabase 后端实现 | 🔄 | Codex 分派中（GoTrue + PostgREST + reqwest blocking） |
| P3-3 | Token 安全存储 | 🚫 | P3-2 |
| P3-4 | SyncEngine 核心（LWW/墓碑/游标） | 🔄 | Codex 分派中（纯逻辑 + P3-5 全分支单测一并交付） |
| P3-5 | SyncEngine 单测（最高优先级测试） | 🔄 | 随 P3-4 同窗交付（要求 ≥12 测试全分支覆盖） |
| P3-6 | SyncEngine 接线（触发器/退避） | 🚫 | P3-2, P3-4 |
| P3-7 | 登录/注册/账号 UI + 删除账号 | 🚫 | P3-2 |
| P3-8 | Win↔Mac 双端联调（M2 里程碑） | 🚫 | P3-6, P3-7, 阶段1 |

## 阶段 4：iOS / iPadOS

| ID | 任务 | 状态 | 依赖 |
|---|---|---|---|
| P4-1 | tauri ios init + 真机跑通 | 🚫 | P0-1 ✅, H-1 |
| P4-2 | 页面虚拟化（可见页 ±1） | ✅ | 按设计满足关闭：Reader 为翻页式（固定 2 canvas 只渲染当前跨页），无滚动列表，无需虚拟化 |
| P4-3 | 渲染分辨率预算（dpr≤2 + 单 canvas 1600 万像素预算） | ✅ | 布局/位图尺寸解耦，超预算按 √ 比例降渲染 scale；tsc 通过（真机内存压测仍归 P4-9） |
| P4-4 | 触屏手势（滑动/捏合/长按取词）+ 审计阻塞项（轻触打开、操作菜单） | 🚫 | P4-1, P4-3 |
| P4-5 | 移动端布局（安全区/44pt/响应式）+ 审计 CSS Top5 | 🚫 | P4-1 |
| P4-6 | 文件导入（选择器 + 分享到 Shelf） | 🚫 | P4-1 |
| P4-7 | 生命周期持久化 | 🚫 | P4-1, P3-6 |
| P4-8 | dict.db 首启拷贝 | 🚫 | P4-1 |
| P4-9 | 内存压测（峰值 <600MB） | 🚫 | P4-3, P4-4 |
| P4-10 | TestFlight 流水线 | 🚫 | P4-1, H-1 |
| P4-11 | TestFlight 公测（M3 里程碑） | 🚫 | P4-10 |

## 阶段 5：书籍文件同步

| ID | 任务 | 状态 | 依赖 |
|---|---|---|---|
| P5-1 | 上传队列（分片/续传/500MB 上限） | 🚫 | P2-6, P3-6 |
| P5-2 | 封面缩略图上传 | 🚫 | P5-1 |
| P5-3 | 云书架 + 按需下载 | 🚫 | P5-1, P5-2 |
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
