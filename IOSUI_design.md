# iOS UI Design

## 当前判断

当前阅读页不是 Adobe PDF 浏览器，而是我们自己的 `pdf.js` 阅读器：

- PDF 打开和渲染在 [src/pdf.ts](/Users/jasonzeng/Developer/shelf-book-reader/src/pdf.ts)
- 阅读器状态、翻页、缩放、目录、文字层、取词逻辑在 [src/components/Reader.tsx](/Users/jasonzeng/Developer/shelf-book-reader/src/components/Reader.tsx)

所以这次改造不应该换 PDF 内核，而应该只改 iOS 的阅读器外壳 UI。

## 目标

把当前 iOS 阅读页从“桌面工具栏 + PDF 舞台”的风格，改成更接近 Apple Books 的阅读体验：

- 顶部使用悬浮式圆角控制条
- 页码独立显示为浮层 badge
- 页面更沉浸，弱化工具栏存在感
- 底部增加缩略页胶片导航
- 保留现有 PDF 渲染、目录、取词、进度保存能力

## 跨平台约束

这个项目运行在 Windows、macOS、iOS 上，所以改动必须隔离。

### 不能做的事

- 不能直接重写共享 `Reader.tsx` 的布局并让三端一起吃到新样式
- 不能替换现有 `pdf.js` 渲染链路
- 不能在共享样式上直接覆盖，避免桌面端 UI 回归

### 必须做的事

- iOS UI 走单独壳层
- Windows / macOS 保持现有阅读器布局
- 共享的只是阅读核心逻辑，不共享 iOS 的视觉外壳

## 设计方案

### 1. 保留阅读核心，拆出平台壳层

把阅读器拆成两层：

- 共享核心
  - 文档加载
  - 页码状态
  - 缩放状态
  - 目录数据
  - 文字层与取词
  - 进度保存
- 平台壳层
  - 桌面端工具栏
  - iOS 顶栏 / 底栏 / 动画 / 布局

建议结构：

- `src/components/reader/usePdfReaderController.ts`
- `src/components/reader/ReaderPageStage.tsx`
- `src/components/reader/ReaderDesktopShell.tsx`
- `src/components/reader/ReaderIosShell.tsx`
- `src/components/reader/ReaderThumbnailStrip.tsx`

### 2. iOS 顶部控制条

参考 Apple Books 的方向，iOS 顶栏建议做成两组悬浮式 pill：

- 左侧
  - 返回
  - 目录
- 右侧
  - 外观 / 主题
  - 大小
  - 搜索
  - 书签

页码不要继续塞进主工具栏，而是单独做成悬浮 badge，例如：

- `158/246页`

### 3. iOS 底部缩略页胶片

增加 Apple Books 风格的底部横向缩略图 strip：

- 展示当前页附近的小范围页面缩略图
- 点击缩略图跳页
- 当前页高亮
- 滑动时懒加载

为了性能，第一版不要渲染全部页面缩略图，只渲染当前页前后少量页面。

### 4. iOS 默认阅读布局

只在 iOS 上调整默认阅读体验：

- 默认更偏 `fit-width`
- 页面舞台背景更柔和
- 减少工具栏占用面积
- 点击页面时自动隐藏 / 唤出工具栏

### 5. 功能优先级

建议先做外壳，再补功能入口的完整能力：

1. 拆分核心与壳层
2. iOS 顶栏布局
3. 页码 badge
4. 底部缩略图 strip
5. 工具栏显隐动画
6. 搜索 / 书签能力补齐

## 执行计划

### Phase 1. 结构拆分

- 从 [src/components/Reader.tsx](/Users/jasonzeng/Developer/shelf-book-reader/src/components/Reader.tsx) 提取共享阅读控制器
- 提取共享页面渲染舞台
- 保留桌面端现有外观作为 `ReaderDesktopShell`

### Phase 2. iOS 平台判断

- 在 [src/platform.ts](/Users/jasonzeng/Developer/shelf-book-reader/src/platform.ts) 增加明确的 iOS 运行时判断
- 进入阅读器时：
  - iOS -> `ReaderIosShell`
  - Windows / macOS -> `ReaderDesktopShell`

### Phase 3. iOS 样式隔离

- iOS 专属样式使用独立命名空间，例如：
  - `.reader-ios`
  - `.reader-ios-toolbar`
  - `.reader-ios-badge`
  - `.reader-ios-strip`
- 不直接污染当前桌面端类名

### Phase 4. 缩略图导航

- 为 iOS 增加缩略图生成和缓存逻辑
- 第一版只实现局部窗口渲染
- 后续再做更完整的滚动式缩略页浏览

## 结论

最安全的路线是：

- 不换 PDF 引擎
- 不动 Windows / macOS 现有阅读器 UI
- 只给 iOS 增加 Apple Books 风格的独立壳层

这样可以最大程度避免跨平台回归，同时把当前第一张图的桌面感阅读页，改成第二张图那种更自然的 iPhone 阅读体验。
