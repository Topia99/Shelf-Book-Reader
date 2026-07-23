# Shelf — 全平台 PDF 书库阅读器

把散落在文件夹里的 PDF，变成一个有封面、有进度、开箱即读的个人书房——
并在 iPhone、iPad、Mac、Windows 上随时接着读同一本书。

体验对标 Apple「图书」：添加书籍后所有阅读都在应用内完成，书库以封面墙展示，
记住每本书读到哪一页，点开即回到上次那一页。

![技术栈](https://img.shields.io/badge/Tauri%202-React%20%2B%20TypeScript-blue)
![平台](https://img.shields.io/badge/macOS-Apple%20Silicon-black)
![平台](https://img.shields.io/badge/Windows-10%2F11-brightgreen)
![平台](https://img.shields.io/badge/iOS%20%2F%20iPadOS-TestFlight-lightgrey)

**本地优先，账号可选**：不登录 = 完整的单机阅读应用，数据全在本地；
登录后才解锁云端同步，在多台设备间接着读。

## 下载与安装

前往 [Releases](../../releases/latest) 下载：

| 平台 | 文件 | 备注 |
|---|---|---|
| **macOS**（Apple Silicon / M 系列） | `Shelf_x.x.x_aarch64.dmg` | 双击 → 拖入「应用程序」 |
| **Windows** 10 / 11 | `Shelf_x.x.x_x64-setup.exe` | 双击一路下一步，当前用户安装，无需管理员 |
| **iPhone / iPad** | — | 经 TestFlight 分发（内测中，需邀请） |

**macOS 首次打开**：当前为 ad-hoc 签名（尚未做 Apple 公证），系统可能提示
「已损坏 / 无法验证开发者」，任选其一放行：右键 App →「打开」；或终端
`xattr -cr /Applications/Shelf.app`；或系统设置 → 隐私与安全性 →「仍要打开」。

**Windows 首次打开**：安装包未做代码签名，SmartScreen 提示时点「更多信息」→「仍要运行」。

## 功能

- **复制入库**：添加 PDF 时复制进应用专属书库，源文件之后可自由移动/删除；
  以 SHA-256 查重（同一文件改名再加也能识别，也是跨设备对齐同一本书的依据）
- **封面书架**：自动提取首页做封面，按最近阅读 / 添加时间 / 书名排序，书名搜索
- **进度记忆**：翻到哪记到哪，重开自动回到上次那一页；移动端切后台即刻落盘防丢
- **阅读器**：单页 / 双页、适宽 / 整页 / 自由缩放、目录跳转、页码跳转、加密 PDF 输密码
- **离线取词翻译**：读英文书时双击（桌面）或长按（触屏）任意单词，原地弹出中文释义、
  音标与发音，内置约 36 万词条离线词典（基于 ECDICT），变形词自动还原（went → go），无需联网
- **触屏适配**（iOS / iPadOS）：滑动/点击翻页、捏合缩放、长按取词、悬浮工具栏、底部缩略图胶片
- **云端同步（账号可选）**：登录后书目与阅读进度在多设备间自动同步（后端 Supabase + Cloudflare R2，
  行级隔离）。书籍文件本体的按需下载/上传正在开发中

## 使用

- **添加书籍**：点右上「＋」选文件（可多选），或把 PDF 拖进窗口；移动端还可从其他 App「分享到 Shelf」
- **打开阅读**：点封面
- **重命名 / 移除**：桌面右键封面 / 移动端 ⋯ 菜单（移除只删书库内副本，不动源文件）
- **查单词**：桌面双击、触屏长按英文单词即弹释义；🔊 朗读；点弹窗外 / Esc / 翻页关闭
- **登录同步**：⋯ 菜单 → 账号，注册/登录后自动同步（可随时退出，回到纯单机）

### 桌面快捷键

| 按键 | 功能 |
|------|------|
| `←` `→` / `PageUp` `PageDown` / 空格 | 翻页 |
| 滚轮（页面边缘）/ 点击画面左右两侧 | 翻页 |
| `Ctrl`（macOS `⌘`）+ 滚轮 | 缩放 |
| `Home` / `End` | 第一页 / 最后一页 |
| 页码框输入数字回车 | 跳转指定页 |
| `Esc` | 返回书库 |

## FAQ

**Q: 添加书籍后源文件还要保留吗？** 不用。入库即复制，之后源文件随便动。

**Q: 数据存在哪？** 本地应用数据目录：macOS `~/Library/Application Support/com.shelf.reader`、
Windows `%APPDATA%\Shelf`——含 `library.db`（书目+进度）、`books/`（PDF 副本）、封面缓存。

**Q: 不登录能用吗？** 能，完整单机使用，无任何网络请求。登录只为多设备同步。

**Q: 扫描版（纯图片）PDF 能取词吗？** 不能，没有文字层；双击/长按会提示。

**Q: 支持 EPUB / 深色模式 / 高亮标注？** 暂不支持，在后续计划中。

## 开发

技术栈：Tauri 2 · React + TypeScript + Vite · PDF.js · SQLite（rusqlite）· Supabase · Cloudflare R2

前置：Node.js、Rust 工具链（`curl https://sh.rustup.rs -sSf | sh`）；iOS 另需 Xcode + CocoaPods。

```bash
npm install                                    # 装前端依赖
npm run tauri dev                              # 桌面（macOS / Windows）开发运行
npm run tauri build                            # 打桌面安装包（macOS .app/.dmg、Windows NSIS）
npm run tauri ios dev "iPhone 15 Pro"          # iOS 模拟器开发
npm run tauri ios build -- --export-method app-store-connect   # iOS App Store IPA
```

- 前端开发服务在 `http://localhost:1420`；真正运行的是 Tauri 拉起的原生窗口
- 发布：桌面产物见 `src-tauri/target/release/bundle/`；iOS 经 `tools/release-ios-testflight.sh` 或
  推 `v*` tag 由 GitHub Actions 自动上传 TestFlight

产品规划见 [PDF阅读器产品计划书_v1.0.md](PDF阅读器产品计划书_v1.0.md)，更新记录见 [CHANGELOG.md](CHANGELOG.md)。
