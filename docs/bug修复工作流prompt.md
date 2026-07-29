# Shelf Bug 修复工作流 · 新对话 Prompt

> 用法：遇到新 bug 时，**把下面「==== 复制线以下 ====」的全部内容贴进新对话窗**，
> 在开头【BUG】处填上问题描述（越具体越好：现象、复现步骤、截图、报错文案、哪个平台）。
> 其余流程 Claude 会自动照做。

---

==== 复制线以下 ====

你接手 Shelf 全平台阅读器项目（Tauri 2，Windows/macOS/iOS 四端 + Supabase/R2 云同步）。本机是所有者 Jason 的 MacBook，仓库 https://github.com/Topia99/Shelf-Book-Reader ，主分支 main 直推。

【BUG】<在这里填：问题现象 / 复现步骤 / 截图 / 报错文案 / 出现在哪个平台>

请严格按下面这套流程处理，一步不跳：

## 1. 先读，别动手
- 读 `PROJECT.md`（项目主文档：技术选型、11 条不可推翻决策 D1–D11、16 个已踩的坑、命令速查、工作流协议）。
- 读 `progress.md` 头部「当前阶段」+ 执行日志前 3 条，掌握最新状态。
- `git log --oneline -8` + `git status` 确认工作区。
- 若涉云端/同步，附读 `docs/多账号本地隔离设计.md`、`全平台开发文档.md §5–6`。

## 2. 诊断根因（不猜）
- 读**真实代码**定位根因，用 `file:line` 给出证据链，说清「为什么会这样」，而不是打补丁盖症状。
- 对照 PROJECT.md §4.2 的 16 个坑，先排除已知陷阱（尤其 WKWebView grid 行高、模拟器缓存、RLS≠授权、Tauri 同步命令阻塞主线程、Cargo.lock/JSON 过 shell 变量等）。

## 3. 先给方案，等批准（硬规则）
- **改任何代码前**，先给出：根因 + 具体修改方案（到函数/文件级）+ 影响面 + 验证方式。
- 若有取舍，给推荐而非罗列。等用户明确说「批准/可以/做」后再动手。**不要边说边改。**

## 4. 实现
- 改动读起来要像周围既有代码（命名、注释密度、惯用法一致）。
- 守住 D1–D11，别推翻既定决策；桌面阅读器视觉零改动（D7）。
- 凭证（R2/service key/.p8/.env.*.local）**绝不入库**；anon key 是公开的可入库。

## 5. 本地验证（按改动类型）
- Rust：`cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings` + `cargo test --manifest-path src-tauri/Cargo.toml`（须零警告、全绿）。
- 前端：`npx tsc --noEmit` + `npm run build`。
- 改 iOS 相关：验证前先 `npm run build`，警惕 WKWebView 缓存陷阱（PROJECT.md §3.4：uninstall 重装 + 双探针判定改动是否到达）。
- 触屏手势/真机视觉这类环境限制验不了的，**说清由用户真机验**，不要假装验过。

## 6. 看板闭环（强制，同一轮内）
- 更新 `progress.md`：底部「执行日志」**倒序**追加一条（日期 + 根因 + 修法 + 验证结果 + 待验项），并更新头部「最后更新」；若对应某任务行/表，同步其状态。
- 状态标记：✅ 完成（已验收）｜🔄 进行中｜⏳ 待开始｜🚫 阻塞｜⛔ 打回。**CI 类只有 GitHub 实跑绿灯才可标 ✅。**

## 7. 提交（用户发话才 commit）
- 未获指示不要 commit/push。用户说提交后：中文提交信息，尾行 `Co-Authored-By:`（按 PROJECT.md §4.4 规范），main 直推无 PR。
- 代码与看板同 commit 或紧随。提交前 `git status` 看清楚，别把构建垃圾/凭证扫进去。

## 8. CI 实跑验证（每次 push 后必做）
- 本地绿不算数。push 后轮询 GitHub Actions，确认三 job（Frontend / Rust-Windows / Rust-macOS）全绿才算数。
- 匿名 API 轮询法（gh 未装、JSON 过 shell 变量会坏 → curl 落盘 + python 读文件解析）：
  ```bash
  SHA=$(git rev-parse HEAD)
  curl -s "https://api.github.com/repos/Topia99/Shelf-Book-Reader/actions/runs?head_sha=$SHA" -o /tmp/ci.json
  python3 -c "import json;d=json.load(open('/tmp/ci.json'));r=d['workflow_runs'][0];print(r['status'],r['conclusion'])"
  ```
- 绿灯后再在看板把该项标 ✅。失败则本地复现定位、修、重推。

## 9. 出包（仅当用户明确说「打包/出包 vX.Y.Z」）
- 先确认目标 tag **没被占用**（`git ls-remote --tags origin`）；已发布的 tag 绝不复用/覆盖，被占就进下一个补丁号。
- bump 5 处版本串 + CHANGELOG：`package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`（**只改 `name="shelf"` 那一行的 version，别全局 sed 误伤同版本号的依赖**）、`src-tauri/gen/apple/shelf_iOS/Info.plist`（两处）、`CHANGELOG.md` 加一节。
- commit → `git tag vX.Y.Z && git push origin vX.Y.Z` → 触发 `release-desktop`（Win exe + Mac dmg → GitHub Release）+ `release-ios`（TestFlight）两条工作流。
- 轮询两条工作流直到都 `completed/success`（同 §8 的匿名 API，按 `head_branch==vX.Y.Z` 过滤）。纯前端改动也需重新出包用户方能真机复验。

## 10. 交接要诚实
- 报告结果如实：测试失败就贴输出说失败；跳过的步骤说跳过；确实验证通过才说通过，别硬说「已修复」。
- 明确区分「已本地/CI 验证」与「待用户真机/双端复验」，并给出用户该验的具体清单。

遇到需要用户操作的外部动作（部署 Edge Function `supabase functions deploy`、App Store Connect、域名、Azure 签名等）——这些你可能被环境拦或无权限，**说清让用户执行**，给出确切命令，别硬绕。

先做第 1、2 步（读文档 + 诊断），然后进第 3 步给我方案，等我批准。

==== 复制线以上 ====
