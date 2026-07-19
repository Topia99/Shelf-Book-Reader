# Fixed Bug

后续所有已修复问题，统一追加到这个文件。

## 2026-07-13

### 1. iOS 真机开发模式白屏

- Bug
  - `tauri ios dev` 时，手机连不上前端开发服务器，页面白屏。
- 修复
  - 修改 [vite.config.ts](/Users/jasonzeng/Developer/shelf-book-reader/vite.config.ts)，让 Vite 在 iOS 真机开发时使用 `TAURI_DEV_HOST` 暴露局域网地址，并补上对应的 HMR 配置。

### 2. iOS Xcode 构建脚本被沙箱拦截

- Bug
  - Xcode 的 `Build Rust Code` 阶段报 `Operation not permitted`，无法继续构建。
- 修复
  - 修改 [src-tauri/gen/apple/shelf.xcodeproj/project.pbxproj](/Users/jasonzeng/Developer/shelf-book-reader/src-tauri/gen/apple/shelf.xcodeproj/project.pbxproj)，将 iOS target 的 `ENABLE_USER_SCRIPT_SANDBOXING` 关闭。

### 3. iOS 从“文件”App 导入 PDF 失败

- Bug
  - 选择 PDF 后提示 `文件不存在`，路径是 `file:///...` 形式。
- 修复
  - 修改 [src-tauri/src/lib.rs](/Users/jasonzeng/Developer/shelf-book-reader/src-tauri/src/lib.rs)，新增输入路径规范化逻辑，把 `file://` URL 转成真实本地路径再导入。
  - 修改 [src-tauri/Cargo.toml](/Users/jasonzeng/Developer/shelf-book-reader/src-tauri/Cargo.toml)，增加 `url` 依赖。

### 4. Xcode Team / 账号残留导致真机签名不稳定

- Bug
  - 项目 Team 与本机可用 Team 不一致，签名、profile、账号状态反复报错。
- 修复
  - 修改 [src-tauri/tauri.conf.json](/Users/jasonzeng/Developer/shelf-book-reader/src-tauri/tauri.conf.json) 和 [src-tauri/gen/apple/shelf.xcodeproj/project.pbxproj](/Users/jasonzeng/Developer/shelf-book-reader/src-tauri/gen/apple/shelf.xcodeproj/project.pbxproj)，统一回当前可用 Team。
  - 清理本机 Xcode 偏好里的旧账号和旧 Team 残留。

### 5. 离线真机版本安装链路不稳定

- Bug
  - `tauri ios run` 在 export 尾阶段偶发失败，导致命令退出，但 App 本体已经构建成功。
- 修复
  - 保留构建产物，直接使用 Xcode 产出的 `.app` 通过 `devicectl` 安装到真机，绕过导出尾阶段问题。

### 6. iOS 独立安装包首屏白屏

- Bug
  - iOS 真机和模拟器安装独立包后，首页白屏；根因是书库页在首屏执行了桌面专用的拖拽监听。
- 修复
  - 修改 [src/components/Library.tsx](/Users/jasonzeng/Developer/shelf-book-reader/src/components/Library.tsx)，在触屏/iOS 环境跳过拖拽监听，并对 `onDragDropEvent` 做安全兜底。
  - 修改 [src/main.tsx](/Users/jasonzeng/Developer/shelf-book-reader/src/main.tsx)，增加启动异常兜底显示，避免后续再出现无信息白屏。
