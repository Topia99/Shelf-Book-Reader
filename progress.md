# Shelf 全平台进度看板（progress.md）

> 全局唯一进度事实来源。每完成一项任务即时更新本文件。
> **新会话接手先读 [PROJECT.md](PROJECT.md)（项目主文档，单文件即可接手）。**
> 任务定义与验收标准见 [implementation_plan.md](implementation_plan.md)，架构与决策见 [全平台开发文档.md](全平台开发文档.md)。
> 状态：✅ 完成（已验收）｜🔄 进行中｜⏳ 待开始｜🚫 被依赖阻塞｜⛔ 验收打回

**最后更新**：2026-07-20
**当前阶段**：iOS App 已过用户真机初步测试 ✅；剩余：P4-9 内存压测、P4-10 TestFlight 流水线、P4-6 文件导入完整链路、桌面端人工回归清单（用户）
**里程碑**：M1 Mac 版可用 🔄（开发完，待人工走查）｜M2 双端接着读 🔄（书目同步已实证，进度互通待验）｜M3 TestFlight 可分享 ⏳（真机直装已通，流水线待建）｜M4 商店上线 ⏳

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
| P4-6 | 文件导入（选择器 + 分享到 Shelf） | 🚫 | P4-1 |
| P4-7 | 生命周期持久化 | 🚫 | P4-1, P3-6 |
| P4-8 | dict.db 首启拷贝 | 🔄 | 结构验证或可免做（dict.db 已随 bundle，Resource 解析 iOS 可用）；待模拟器实测查词后关闭 |
| P4-9 | 内存压测（峰值 <600MB） | 🚫 | P4-3, P4-4 |
| P4-UI-1 | iOS 壳层：结构拆分（controller/stage/双壳） | ✅ | Codex 拆分 + 主控验收；tsc 通过；macOS 启动冒烟零错误（完整桌面回归清单留用户过） |
| P4-UI-2 | iOS 壳层：平台路由（isIos 含 iPad 判定） | ✅ | 修订 2 判定实现正确（含 iPad 伪装 UA）；iPhone 模拟器实跑命中 iOS 壳、macOS 命中桌面壳 |
| P4-UI-3 | iOS 壳层：顶部 pill/页码 badge/沉浸显隐 | ✅ | 模拟器实拍：双 pill/badge/fit-width/浅灰舞台全达标；主控修复 stage 顶距被尾部触屏规则覆盖的层叠 bug（提高特异性） |
| P4-UI-4 | iOS 壳层：底部缩略图 strip（内存预算硬约束） | ✅ | 主控实现：±5 页窗口/近处优先补齐/150px 上限/JPEG dataURL 化即销毁 canvas/24 张 LRU/空闲启动+翻页代际作废；模拟器实拍 5 页缩略图全渲染、当前页赭色描边（4.4 大文件压测并入 P4-9） |
| P4-10 | TestFlight 流水线 | 🚫 | P4-1, H-1；**2026-07-21 暂缓**：Apple Developer/ASC 账号创建中，等就绪再启动 |
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
