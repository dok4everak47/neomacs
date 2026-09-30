# NEO Emacs 功能总览（中文）

> 本文按仓库当前代码整理，用于快速了解"现在真有什么"。
> 状态标记沿用 `README.md` 的 Status 表：🚧 可用并在打磨 / 🔬 设计或实验阶段。
> 核对时间：2026-09-30，基线 `91e573547`。
> 本文只描述代码中已实现的能力；某台机器上是否跑得起来取决于该机的构建与原生依赖。

## 1. 定位

NEO Emacs 是 GNU Emacs 的硬分叉：Elisp 的 Lisp 树随 GNU 同步，**C core 已整体替换为 Rust**，
显示层跑在 GPU 上。它与 GNU Emacs 的契约是**可观测行为等价**——你的 `init.el`、包、肌肉记忆照常工作，
而每个被重写的子系统都拿 GNU Emacs 当 oracle 逐个比对。

## 2. 核心运行时（纯 Rust）

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| Elisp 求值器 / 字节码 VM / GC / portable dump | 🚧 | `crates/neovm-core`，无 C 依赖 |
| JIT 编译 + inline caching | 🚧 | Cranelift 分层 JIT，默认开启；`--no-default-features` 可退回纯解释器 |
| 零暂停 GC | 🚧 | 精确 GC；缺 root 类缺陷由 `cargo xtask gc-stress` 常备探测 |
| 多线程 Elisp | 🔬 | `neovm-worker` 调度器 + isolate 模型，仍在设计/实验 |
| GNU 兼容性 | 🚧 | oracle 套件 + TUI 网格对比 + GUI parity；`crates/neovm-oracle-tests` 下 327 个 divergence 模块 |

## 3. 显示引擎

- **GPU 渲染**：wgpu（Vulkan / Metal / DX12 / GL）；约数千行 Rust 替代约 50,000 行 `xdisp.c`
- **布局引擎**：`crates/neomacs-layout-engine`，含行/段布局、hscroll、display 属性求值
- **字体**：`crates/neomacs-font-materializer`；macOS 走 CoreText，Linux 走 Fontconfig + FreeType
- **TUI**：`neomacs -nw`，同一二进制既出 GUI 也出终端；终端能力经 `crates/neomacs-terminfo`
- **窗口/帧/菜单**：`emacs_core/display/{frame,window_cmds,menu,hscroll,chrome_dirty,...}`

## 4. Buffer 内联富媒体

| 能力 | 入口 | 说明 |
| --- | --- | --- |
| 内联视频 | `lisp/neomacs-video.el`（`neomacs-video-insert`、播控、循环） | Linux 走 GStreamer + VA-API，DMA-BUF 零拷贝；macOS/Windows 该 capability 为 `none` |
| 大图显示 | `lisp/neomacs-image.el` | GPU 解码、分带上传、解码即预览；`max-image-size` 等 GNU 语义已对齐 |
| 内联浏览器 | `lisp/neomacs-webkit.el` | Linux 用 WPE WebKit（DMA-BUF）；macOS 用系统 `WKWebView` 原生视图叠加 |
| 内联终端 | `lisp/neo-term.el`（`neo-term-shell`、行列数等 defcustom） | GPU 后端终端；README 标注仍在开发中 |

## 5. 动画（Elisp 可配，全部跑在渲染线程）

详见 `docs/animations.md`。渲染线程有集中的帧调度器，按显示器刷新率驱动交互式动画，
环境型效果可声明更低节奏（光标色循环默认 24 Hz）。

- **光标**：8 种粒子/视觉效果（`smooth`、`railgun`、`torpedo`、`pixiedust`、`sonicboom`、`ripple`、`wireframe`、`none`）
  × 7 种插值风格（`exponential`、`spring`、`ease-out-quad/cubic/expo`、`ease-in-out-cubic`、`linear`）；
  spring 风格另有四角拖尾（`trail-size`）。
- **滚动**：21 种效果，分 2D / 3D / 形变 / 后处理 / 创意五类，另配 5 种缓动。
- **buffer 切换**：10 种过渡（`crossfade`、`slide`、`parallax`、`card-flip`、`page-curl`、`scale-zoom` …）；
  效果、轴向、方向三者正交，不兼容的轴向被忽略而不是报错。

## 6. Elisp 可编程前端（GNU Emacs 没有的部分）

- `lisp/neomacs-shaders.el` — 以 frame 为单位的后处理画廊（CRT、glow、matrix、ghostty 系列等）
- `lisp/neomacs-shader-playground.el` — `M-x neomacs-shader-playground`，实时编辑 WGSL 并预览
- `lisp/neomacs-surface.el` — 从 Lisp 创建 GPU 纹理 surface（`neomacs-surface-insert` / `-attach` / `-create-and-insert`）
- `lisp/neomacs-gradients.el` — `:background-gradient` face 属性与渐变动画（GNU 会忽略该属性，故不破坏兼容）

## 7. 兼容性与验证基础设施

- **oracle 对比**：`crates/neovm-oracle-tests`（327 个 divergence 模块）、`crates/neomacs-tui-tests`（网格逐行）、GUI parity
- **基线钉死**：`parity-reference.toml` + `crates/neomacs-parity-reference`，harness 拒绝未认证的 GNU
- **MELPA parity**：`crates/neomacs-melpa-tests`，覆盖数百个真实包的安装与行为
- **可观测**：`(neomacs--frame-snapshot FRAME FORMAT)` 输出屏幕语义（文本/JSON）；
  `NEOMACS_DEBUG_SURFACE_READBACK=1` + `..._PNG=path` 取 GPU 实际绘制结果
- **日常测试床**：Doom Emacs 配置

## 8. 分发与平台

- **二进制**：`neomacs`、`neomacsclient`（daemon：`--daemon` / `--fg-daemon`）、Windows 的 `runneomacs.exe`、`cmdproxy`
- **Linux**：`.deb` / `.rpm`（在 el9 容器内构建）/ tarball / AppImage；x86_64 与 aarch64
- **macOS**（实验性）：`.dmg` / `.zip` / `.tar.gz`（自包含 `neomacs.app`）；Homebrew cask
- **Windows**（实验性）：installer `.exe` / portable `.zip`；x86_64 与 aarch64
- **容器**：`evalexec/neomacs`（Docker Hub）与 GHCR
- **计划中**：WASM、Android、iOS

## 9. 可以立刻试的入口

```elisp
M-x neomacs-shaders-gallery        ; shader 画廊
M-x neomacs-shader-playground      ; 实时改 WGSL
M-x neomacs-surface-demo           ; GPU surface 演示
M-x neomacs-rainbow-mode-line      ; 动画渐变 mode-line
M-x neo-term                       ; 内联 GPU 终端
(neomacs-video-insert "clip.mp4")  ; 内联视频（需 Linux 的 GStreamer 后端）
```

## 10. 未完成但常被误读为"已有"

- 真正的多线程 Elisp（当前是设计/实验）
- 并发零暂停 GC 的成熟形态
- WASM / Android / iOS 端口
- Windows 的完整验证（README 标注"等待测试"）
- `docs/ongoing-tasks.md` 等早期文档里出现的 `test/neovm/vm-compat`、`check-neovm` 属**已删除的旧路径**，
  现以 `crates/neovm-oracle-tests` 与 CI workflow 为准。
