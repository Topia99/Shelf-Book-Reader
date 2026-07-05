# 产品计划书：Windows 11 PDF 书库阅读器（暂定名：Shelf）

版本：**v1.0（已冻结）**
日期：2026-07-03
状态：需求确认完毕，进入技术 Spike 阶段

---

## 0. 已确认的关键决策

| 决策项 | 结论 |
|--------|------|
| 文件管理模式 | **复制入库**（像 iOS Books）：添加时把 PDF 复制到软件专属书库文件夹，源文件之后可自由移动/删除 |
| 技术栈 | **Tauri 2 + React + TypeScript + PDF.js + SQLite** |
| MVP 范围 | 只做核心闭环（R1–R6 + 阅读器基础能力），深色模式、标注、统计全部推到 v1.1 |
| 使用范围 | 默认自用优先（dogfooding），暂不投入代码签名/自动更新 |

## 1. 产品概述

一款运行在 Windows 11 上的本地 PDF 书库阅读软件，体验对标 iOS「图书」App：
用户把 PDF 添加进书库后，之后所有阅读都在这一个软件内完成。软件以书籍封面墙展示书库，记住每本书的阅读进度，点开即回到上次读到的那一页。

**一句话定位**：把散落在文件夹里的 PDF，变成一个有封面、有进度、开箱即读的个人书架。

## 2. MVP 功能规格（冻结）

### R1. 添加书籍（复制入库）
- 「添加书籍」按钮选择文件（可多选）+ 拖拽 PDF 到窗口入库
- 入库流程：计算文件 SHA-256 → 查重（已存在则提示并跳过）→ 复制到 `%APPDATA%/Shelf/books/{hash}.pdf` → 前端用 PDF.js 渲染第 1 页 → 保存封面到 `%APPDATA%/Shelf/covers/{hash}.png` → 写入数据库
- 书名默认取 PDF 元数据 Title，为空则用文件名（去扩展名）；入库后可重命名
- 加密/损坏 PDF：入库成功但使用默认封面（纯色 + 书名文字），打开时提示输入密码或报错

### R2. 一站式阅读
- 启动直达书库页；双击封面在软件内打开阅读

### R3. 书库页
- 封面网格视图：封面、书名、进度（"137/420 · 33%"）
- 排序：最近阅读（默认）/ 添加时间 / 书名
- 顶部搜索框按书名过滤

### R4. 删除书籍
- 右键封面 →「从书库移除」→ 二次确认
- 由于是复制入库，移除即删除书库文件夹内的副本和封面缓存（不影响用户的源文件）

### R5. 阅读进度
- 翻页时记录当前页码，停留 1 秒后写库（节流），窗口关闭时强制落盘
- 打开书籍自动跳到上次页码，首次打开从第 1 页
- 强杀进程/断电后进度最多丢失最后 1 秒

### R6. 封面
- 自动取第一页渲染图；渲染失败用默认封面

### 阅读器基础能力
- 翻页：← → / PageUp/Down / 滚轮 / 点击左右翻页区
- 缩放：适合页宽 / 适合整页 / Ctrl+滚轮自定义
- 页码跳转输入框 + 总页数显示
- 目录侧边栏（读取 PDF outline 书签）
- 单页 / 双页视图切换
- Esc 或返回按钮回书库

### 明确不做（MVP）
深色模式、书签/高亮标注、阅读统计、EPUB、云同步、账号、.pdf 文件关联、代码签名、自动更新 → 全部进 v1.1+ 待办池。

## 3. 技术设计要点

### 架构分工
- **前端（React + TS）**：书库 UI、阅读器 UI、PDF.js 渲染（含封面提取——在前端渲染第 1 页到 canvas 后导出 PNG，避免 Rust 侧引入 PDF 库）
- **Rust 侧（Tauri commands，代码量很小）**：文件复制/删除、SHA-256 计算、封面 PNG 落盘、SQLite 读写
- **插件**：`tauri-plugin-sql`（SQLite）、`tauri-plugin-dialog`（文件选择）、`tauri-plugin-fs`

### 数据模型

```sql
CREATE TABLE books (
  id INTEGER PRIMARY KEY,
  hash TEXT UNIQUE NOT NULL,        -- SHA-256，查重键
  title TEXT NOT NULL,
  file_path TEXT NOT NULL,          -- 书库内副本路径
  cover_path TEXT,
  total_pages INTEGER,
  current_page INTEGER DEFAULT 1,
  added_at TEXT NOT NULL,
  last_opened_at TEXT
);
```

### 目录结构

```
%APPDATA%/Shelf/
  library.db
  books/    {hash}.pdf
  covers/   {hash}.png
```

### Tauri Commands（前后端接口约定，随开发维护）
`add_books(paths) -> [BookMeta]`、`remove_book(id)`、`list_books(sort, query)`、`update_progress(id, page)`、`rename_book(id, title)`

## 4. 开发流程

1. **技术 Spike（0.5 天）**——最小闭环验证：Tauri 窗口里用 PDF.js 打开一个 PDF → 渲染翻页 → 页码写入 SQLite → 重启恢复。同时验证：canvas 导出封面 PNG、500MB 大文件打开速度。任一失败再评估方案
2. **UI 原型（0.5 天）**——书库页 + 阅读页低保真草图
3. **MVP 开发（1–2 周业余时间）**，任务按依赖排序：
   - T1 项目骨架（create-tauri-app）+ SQLite 初始化 + 插件接入
   - T2 添加书籍全流程（选择/拖拽 → 哈希查重 → 复制 → 封面 → 入库）
   - T3 书库网格页（排序、搜索、右键删除）
   - T4 阅读器页（PDF.js 渲染、翻页、缩放、目录、页码跳转）
   - T5 进度记录与恢复（节流写库 + 关闭落盘）
   - T6 异常处理（加密 PDF、损坏文件、中文路径、磁盘满）
4. **自测（1 周日常使用）**——用自己的真实书库跑，记录问题
5. **v1.0 打磨 + 打包**——`tauri build` 出 NSIS 安装包
6. **迭代 v1.1**——从待办池按痛感排优先级

### 测试重点 Checklist
- [ ] 1000+ 页 / 500MB+ 大 PDF 打开与翻页流畅度
- [ ] 扫描版纯图片 PDF 渲染
- [ ] 加密 PDF 的入库和打开降级
- [ ] 文件名/路径含中文、空格、特殊字符
- [ ] 重复添加同一文件（含改名后的同一文件——哈希应识别）
- [ ] 强杀进程后进度恢复
- [ ] 移除书籍后文件与封面确实清理
- [ ] 磁盘空间不足时复制入库的报错提示

## 5. 文档清单

| 文档 | 产出时机 |
|------|----------|
| PRD（本文档） | ✅ 已冻结 |
| 技术设计文档（架构、Schema、commands 详细定义） | Spike 完成后 |
| 测试用例清单（上方 checklist 展开） | MVP 完成前 |
| README（安装、快捷键、FAQ） | 打包时 |
| CHANGELOG | 持续 |

## 6. v1.1 待办池（不承诺顺序）

深色/夜间模式 → 书签与高亮标注 → 阅读时长统计与历史 → 手动更换封面 → .pdf 文件关联 → EPUB 支持

---

*下一步：技术 Spike。建议直接用 Claude Code 从 create-tauri-app 脚手架开始。*
