# AGENTS.md — NEO Emacs (neomacs) 协作规则

> 本文件是 AI 助手在本仓库工作时的约定（适用本机 macOS / Apple Silicon 会话）。
> 这不是练习项目：仓库是 GNU Emacs 的硬分叉，Lisp 树随 GNU 同步、C core 已整体换成 Rust，
> 验收标准是与固定版本 GNU Emacs 的逐项对比。
> 规则优先级：**不破坏兼容性契约 > 可验证的正确性 > 速度**。
> 最后更新：2026-09-30

## 1. 项目速览

- **形态**：`crates/` 下的 Rust workspace（`members` 列出 26 个 crate，含 `xtask`）实现运行时与显示栈；
  `lisp/`、`leim/`、`test/`、`etc/`、`docs/` 中的 GNU 派生素材保留在仓库根，
  属于应用资源而不是 Rust 包；**新增 Rust 包一律放 `crates/<name>/`**。
- **发行二进制**：`neomacs`（纯 Rust，无 C 依赖）。GNU Emacs 不是依赖而是 **oracle**：
  oracle 套件 / TUI 网格对比 / GUI parity 三套机制持续把两个编辑器逐项对齐。
- **上游 / 我的 fork**：上游是 `github.com/eval-exec/neomacs`（**只读参照，不往那里推任何东西**）；
  本机是个人克隆，改动推送到自己的 fork `github.com/dok4everak47/neomacs`。
  写代码按上游的提交风格与评审标准来，方便随时对照/挑拣，但推送目标是 fork。
- **关键入口**：`docs/ARCHITECTURE.md`（分层与设计原则）、`docs/building.md`（构建/依赖）、
  `docs/faq.md`（为什么重写、fork 出处）、`docs/ongoing-tasks.md`（移植切片日志）、
  `docs/plans/`（带日期的设计/实施稿）、`docs/gui-agent-testing.md`（GUI 可观测/驱动）、
  `crates/neovm-core/src/emacs_core/README.md`（运行时核心文件归属规则）。

## 2. 协作方式

1. **一次只做被授权的那一步**。完成即停，给出下一步建议（含理由，可 1~3 个候选），
   经用户明确同意再继续；不要顺手重构、顺手升级、顺手加功能。
2. **动结构前先给方案**。涉及公共 API、`neovm-core` 架构、GC/布局/渲染线程、或跨多个 crate 的
   改动，先用不超过 10 行写清：改什么、为什么、影响面、怎么验证；批准后再动手。
3. **对话用中文，代码与文档用英文**。技术术语保留英文（`oracle`、`parity`、`SubrSpec`、
   `rooting`……），第一次出现时给一句中文解释。
4. **引用必须现查**。引用 GNU C 源码锚点（形如 `src/xdisp.c:32492`）、仓库文件或行号之前，
   用 `rg` / `sed -n` 现场核对；本项目大量以 C 行号作为移植依据，引错比不引更糟。
   文档里的版本/行号可能滞后（例如 `docs/building.md` 与 `parity-reference.toml` 曾不同步），
   以权威文件 + 现场命令为准。
5. **用户会在两轮消息之间自己改代码**。每次回应前重新读磁盘（`git status -sb` + 相关文件），
   不要把上一轮读到的内容当现状。
6. **报告格式**（每个任务收尾都按这个形状，失败也照报）：

   ```text
   Implemented:
   Verification:（跑了什么命令，真实输出结论）
   Self-review:（哪里稳、哪里还存疑）
   Suggested next step:
   Waiting for approval.
   ```

## 3. 环境与工具链

- 平台 macOS（aarch64）；本机工具链由**本地精简版** `flake.nix` 提供：
  `nix develop`（rust-overlay `stable.latest` + rustfmt/clippy/rust-analyzer/rust-src）。
  本仓库没有 `.envrc`（和你其他项目不同），别假设 direnv 自动生效。
  `rust-toolchain.toml` 仍钉 `1.96.1`，但那是**仓库声明的版本**，不必然等于本机
  devShell 里 `rustc --version` 的输出——别把两者当成一回事。
  完整的上游 devShell（含 WPE/GStreamer/native 依赖、cachix substituter）在
  `upstream/main:flake.nix` + `nix/` 里，需要时 `nix develop --accept-flake-config` 走那份。
  注意：上游 CI 合同（`nix flake check`、cachix 发布）覆盖的是上游 flake，不是本机这份。
- **绝不把工具链装到全局**；不要随手升级 `rust-toolchain.toml` 或 flake 输入——
  版本在安装脚本与 CI 多处联动（历史上一次工具链更新需要同步一批引用）。
- 网络：本机 HTTP 客户端走 ClashBar 代理 `127.0.0.1:7890`（git 已配置）。
  注意 Clash 开着 fake-ip，**连 `cache.nixos.org` 都解析到 `198.18.0.0/16`**，
  所以"直连 vs 走代理"在本机基本没有区别，个别请求失败多半是抖动——
  不要据此下"某个源在国内不可达"的结论，先重测几次。
  Nix 侧 substituter 按 `/etc/nix/nix.conf`（`cache.nixos.org` + 国内 `cache.numtide.com`）；
  `eval-exec.cachix.org` / `nix-wpe-webkit.cachix.org` 只在上游 flake 的 `nixConfig` 里，
  本机精简 flake 用不到。沙箱内需要联网下载（nix / cargo fetch）或写 `.git` 时，
  按系统流程提权并说明要做什么。
- **不用 rustup，也不需要给 rustup 配镜像**：本机没有 rustup 进程，`rustc`/`cargo` 全部来自
  Nix store（实测 `rustc 1.98.1`），而 rustup 的镜像设置（`RUSTUP_DIST_SERVER` /
  `RUSTUP_UPDATE_ROOT`）对不存在的 rustup 不生效。crate 下载同样**不要**照搬其他项目的
  rsproxy 配置：本仓库 `.cargo/config.toml` 不动 source，`static.crates.io` 与
  `index.crates.io` 实测均可达（走代理或直连都已通过）；给这个近千依赖的 workspace
  引入国内 CDN 单点得不偿失。只有真去装 rustup 工具链或跑 PGO 时才需要 rustup，
  那时也不是镜像能解决的，而是走代理。
- macOS 上内联浏览器走系统 WKWebView；`video` 后端仅在 Linux（linked-gstreamer）。
  带 GUI/GPU 的验证需要真实桌面会话；起不来的话先检查 display 环境，不要把环境问题当代码问题。

## 4. 构建

- 完整构建 = `cargo xtask fresh-build --release`。它不只是 `cargo build`：还跑 GNU 形状的 Lisp
  bootstrap（COMPILE_FIRST 字节编译、生成 Unicode/leim 数据、loaddefs、pdump）。
  产物是 `target/release/neomacs`（bootstrap 阶段还有 `neomacs-temacs` / `bootstrap-neomacs`）。
- 常用开关：`--low-memory`（低内存机器串行编译）、`--skip-build`、`--no-byte-compile`、
  `--native-comp`、`--jobs N`、`--dry-run`、`--features ...`。
- 迭代期**不要反复 fresh-build**：改 Rust 代码先用 `cargo check -p <crate>`（必要时
  `--all-targets`）；涉及 Lisp 行为/启动路径时再整体重建。
- 发行能力不是临时开关：`video` / `webview` / `windows-tools` 由 `Cargo.toml` 的
  `[workspace.metadata.neomacs-production-capabilities]` 表驱动，`xtask` 与 `flake.nix`
  共用同一份；**不要在任何调用方复制这张表**，也不要按平台在代码里硬编码 feature。
- `[patch.crates-io]` 里的 fork（wgpu-hal / cosmic-text / freetype-sys）是上游维护的补丁：
  本地只读理解，要改就走第 8 节的 fork 流程，不要直接改 checkout 或 vendor。

## 5. 测试与验证

- 验证层级：`cargo check -p <crate>` → 目标套件 nextest → 完整套件。
  **任何"通过 / 修好 / 完成"的结论必须来自实际执行**；跑不动就明说跑不动，禁止用
  "应该没问题"代替，也禁止换一个命令跑完再宣称同一个验证通过。
- 主要套件（先 `fresh-build --release`；TUI 默认使用 `target/release/neomacs`）：

  ```bash
  cargo nextest run -p neovm-core --no-fail-fast
  cargo nextest run -p neovm-oracle-tests --no-fail-fast
  cargo nextest run -p neomacs-tui-tests --release --no-fail-fast
  ```

- TUI 环境变量：`NEOMACS_TUI_NEOMACS_BIN` 指定被测二进制；`NEOMACS_TUI_RECORD=on`
  录制 asciicast（产物在 `target/tui-recordings/`，`asciinema play` 回放）；
  录制根目录可用 `NEOMACS_TUI_RECORD_DIR` 改写。
- oracle / TUI / GUI 的 GNU 侧 = `PATH` 上第一个 `emacs`，**必须与 `parity-reference.toml`
  钉住的版本一致**。版本不一致时 pair 用例会集体失败（历史教训：新版本改了 mode-line 的
  padding，对着旧基线几乎全部 TUI pair 都会挂）——先核对 oracle，再怀疑自己的代码。
- 基线只经 `cargo xtask pin-reference --emacs PATH --reason "..."` 重定；
  `parity-reference.toml` 的键值手改会绕过记录日志。harness 拒绝未认证 oracle 是默认行为；
  `NEOMACS_PARITY_REFERENCE=none` 只用于临时无基线环境，产出会带 `UNATTESTED` 标记——
  不许用它把对齐问题糊过去。
- MELPA parity（按 `crates/neomacs-melpa-tests/README.md` 的配方）：
  `TMPDIR="$PWD/tmp" NEOMACS_BIN="$PWD/target/release/neomacs" cargo nextest run -p neomacs-melpa-tests --no-fail-fast`
  —— 包缓存/解包都落在 `./tmp`；GNU 侧选取顺序是 `NEOMACS_MELPA_ORACLE_EMACS` →
  `NEOVM_ORACLE_EMACS` → `ORACLE_EMACS` → 相邻的本地 GNU 源码树 → `PATH` 上的 `emacs`。
  只有终端类用例（`--test melpa_tui`）需要 `--release` 配置。
  nextest 已挂 `scripts/melpa-infra-preflight.sh` 做 fixture 物化；fixture 缺失时相关用例按设计跳过。
- 别绕开测试配置：`.config/nextest.toml` 里 `test-threads = 24`、600s 慢超时、
  Linux 上 8 GiB 地址空间包装（`prlimit`）、`RUST_MIN_STACK=128MB`（保守 GC 扫描主线程栈）。
  这些是有原因的护栏，不是可以本地调掉的噪声。
- `cargo xtask gc-stress` 是**缺 root 类 bug 的常备探测器**：普通套件全绿不能证明 GC 安全
  （DIVERGENCES 161/162 就是这类）。触碰 evaluator / GC / rooting / 内建注册边界的改动，
  验证链里必须包含它。
- 性能改动用 `cargo xtask perf list / run / compare / profile`；对比要给出基线与候选两个二进制。
- 格式与 lint 门禁：`cargo fmt --all --check` + `cargo fmt --manifest-path crates/neovm-core/fuzz/Cargo.toml --check`；
  CI 对 5 个前端 crate 跑 `clippy --no-deps -- -D warnings`（neovm-* 不在此列，lint 状态各自独立）。
  仓库自带推前门 `.config/git/hooks/pre-push`（fmt + `cargo check --workspace --all-targets`），
  本地用 `git config core.hooksPath .config/git/hooks` 激活（本机克隆尚未激活）；
  未激活时，push 前手动跑同样两条命令。

## 6. Oracle 与兼容性纪律

- 行为问题先问："GNU 在这里怎么做？"——找到对应实现（`src/*.c`）再决定怎么移植；
  结论里带 C 文件:行号锚点。只凭直觉"这样更合理"而改行为，一律先对齐 oracle。
- 新增/扩展 Elisp 能力按 `docs/neovm-subsystem-porting.md` 的清单走：
  模块归入 `crates/neovm-core/src/emacs_core/<子系统>/` 并在那里接线 → 内建用 `SubrSpec` /
  `define_subrs!` 注册（声明放同级 `subrs.rs`，legacy manifest 只减不增）→ 在
  `crates/neovm-oracle-tests/src/divergence/` 按现有风格加 oracle 对比用例 →
  跑相关 gate → 在 `docs/ongoing-tasks.md` 记一条切片。
  （该文档里的 `rust/neovm-core/src/elisp/`、`test/neovm/vm-compat`、`check-neovm`
  等路径与命令都已过时，目录在迁移中删除了；现以 `crates/neovm-core/src/emacs_core/`、
  `crates/neovm-oracle-tests` 与 CI workflow 为准。）
- oracle 用例的模式开关：`NEOVM_ORACLE_MODE`（`snapshot` 默认 / `verify` / `refresh`）
  决定期望值是比对快照还是现场跑 GNU；现场比对的 GNU 默认取 `PATH` 上的 `emacs`，
  也可用 `NEOVM_FORCE_ORACLE_PATH` 点名（点名后解析不到会硬失败，不会静默跳过）；
  被测 Neomacs 二进制默认 `target/release/neomacs`，可用 `NEOVM_BINARY_PATH` 改写。
- **不许用弱化期望的方式让测试变绿**：不加 `#[ignore]`、不删断言、不绕过公共路径。
  本项目刻意保留"失败着提交"的 divergence 测试；`DIVERGENCES.md` 条目是意图，不是待清理对象。
- `crates/neomacs-melpa-tests/DIVERGENCES.md` 与 `crates/neovm-oracle-tests/*_DIVERGENCES.md`
  只写已在两个编辑器里真实复现的条目；新增记录前先按文件里的重现命令跑一遍。
- 显示/GUI 问题优先用可文本化的观测回路：`(neomacs--frame-snapshot FRAME FORMAT)` +
  `emacsclient --eval`（见 `docs/gui-agent-testing.md`）；"GPU 真的画了没有"用
  `NEOMACS_DEBUG_SURFACE_READBACK=1` + `NEOMACS_DEBUG_SURFACE_READBACK_PNG=<path>` 的
  readback PNG 证明，而不是靠人眼看截图。

## 7. 代码归属与架构约束

- 运行时核心 `neovm-core` 自包含：不感知 buffer/window/frame；编辑子系统通过 Runtime API
  注册类型/根/原语；模块之间不伸手进对方内部——这是 `docs/ARCHITECTURE.md` 的硬边界。
- `crates/neovm-core/src/emacs_core/` 是"一个子系统一个目录"：**新生产 Rust 文件必须落在
  拥有它的子系统目录下**，根/域级散落文件会被架构测试拒绝；根 `mod.rs` 只是稳定 facade。
  完整规则见 `crates/neovm-core/src/emacs_core/README.md`。
- Rust 侧 Elisp 函数用 `SubrSpec` 声明（名字 / 函数形状 / Lisp 可见 arity / 分派 / 交互契约 /
  启动策略一体），唯一安装路径是 `Context::register_subr`；实现放子系统 `mod.rs`、
  声明放同级 `subrs.rs`，由 `define_subrs!` 生成批次。那份 legacy manifest 只减不增。
- 工作区根路径不要靠猜：用 `.cargo/config.toml` 定义的 `CARGO_WORKSPACE_DIR`；
  crate 内 fixture 才用 `CARGO_MANIFEST_DIR`。
- lints：5 个前端 crate opt-in 工作区 lint（含 `unsafe_op_in_unsafe_fn = "warn"`）；
  需要 `#[allow]` 时按站点写并给理由，不要在 crate 级大面积压掉。

## 8. 依赖与 fork 纪律

- **绝不 vendor**：要改第三方 crate，就在 GitHub 上 fork，改动放 fork 的 `patch` 分支；
  `[patch.crates-io]` 引用精确 `rev`，并在旁边用注释写清分支与原因。
- 不无理由新增/升级依赖；`Cargo.lock` 在 CI 多处配 `--locked`，不要让它被顺手改写。
  依赖政策受 `deny.toml`（license allowlist + advisory 例外）约束，CI 跑 cargo-deny。
- Cranelift 家族必须同 minor 版本联动（xtask 的 coherence 检查会验）；
  本地预检：`cargo run --locked -p xtask -- check-dependency-coherence`。

## 9. Git 纪律

- **不主动 commit / push / 建分支**，除非用户点名要求；绝不执行破坏性操作
  （`reset --hard`、force push、丢弃工作区改动）——确需时先复述将丢失什么。
- **远端布局**：`origin` = 我的 fork `dok4everak47/neomacs`（推送目标），
  `upstream` = 上游 `eval-exec/neomacs`（**只读参照，绝不 push**）。本地 `main`
  跟踪 `origin/main`；要看/拉上游用 `git fetch upstream && git log upstream/main`。
  需要往上游送东西时只走 PR（从 fork 发起），不要在本地直接推上游。
- 提交风格对齐上游历史：英文、祈使句 subject（一行）；正文解释"GNU 是什么行为
  （引 `src/*.c` 行号）、这个移植做了什么、为什么这样切"；相关 issue（`#NNN`）/ ledger
  编号写进正文。文档改动与代码改动分开提交。
- 开工前先 `git status -sb` 认清基线（`upstream/main` 与 `origin/main` 可能与本地有差距）；
  工作区里存在**有意为之的本地改动**（见下），不要把它们当成待清理的脏文件。
- **`flake.nix` / `flake.lock` 是本机个人精简工具链**（rust-overlay 的
  `stable.latest` + `clippy`/`rustfmt`/`rust-analyzer`/`rust-src`，带 ClashBar 代理 shellHook），
  不是上游那份 flake-parts/crane 结构。它只覆盖 Rust 工具链，不复制 `nix/` 与
  `Cargo.toml` 里 CI/production 的能力面（无 WPE、无 GStreamer、无 production-capabilities
  映射；`nix flake check`、cachix 发布等 CI 合同不覆盖本机 devShell）。
  所以：**不要因为与上游不同就"修回去"或还原**，也不要把它当成 Cargo 构建是否成功的判据。
  看上游 flake 用 `git show upstream/main:flake.nix`。

## 10. 文档与记录

- README / docs 与代码不一致时，以实际代码为准，并在同一任务里把文档改真；
  `docs/ongoing-tasks.md` 的切片日志随完成追加。
- 大特性先写/更新带日期的设计稿（沿用 `docs/plans/`、`docs/design/`、
  `docs/superpowers/{plans,specs}/` 的既有惯例），再动手。
- **不手改生成物**：`*loaddefs.el`、`lisp/leim/quail/*.el` 等 leim 生成项、`subdirs.el` 在 `.gitignore`
  里，由 xtask 从源数据生成——要改就改源。
- GNU 派生素材（`lisp/`、`test/`、`etc/`）尽量少动；必须是行为移植/修复才动，
  并在提交信息里说清对应的 GNU 语义。

## 11. 命令速查

| 目的 | 命令 |
| --- | --- |
| 进入 devShell（本机精简版） | `nix develop` |
| 进入上游完整 devShell | `nix develop --accept-flake-config`（用 `upstream/main` 那份 flake 时） |
| 完整构建 | `cargo xtask fresh-build --release` |
| 低内存构建 | `cargo xtask fresh-build --release --low-memory` |
| 运行 | `./target/release/neomacs` |
| 快速编译检查 | `cargo check -p <crate>` / `cargo check -p neomacs --no-default-features` |
| 格式门禁 | `cargo fmt --all --check` |
| Clippy 硬门（CI 同款） | `cargo clippy -p neomacs-display-protocol -p neomacs-layout-engine -p neomacs-renderer-wgpu -p neomacs-display-runtime -p neomacs --no-deps -- -D warnings` |
| 核心套件 | `cargo nextest run -p neovm-core --no-fail-fast` |
| oracle 套件 | `cargo nextest run -p neovm-oracle-tests --no-fail-fast` |
| TUI 对比 | `cargo nextest run -p neomacs-tui-tests --release --no-fail-fast` |
| MELPA parity | `TMPDIR=$PWD/tmp NEOMACS_BIN=$PWD/target/release/neomacs cargo nextest run -p neomacs-melpa-tests --no-fail-fast` |
| GC 根探测 | `cargo xtask gc-stress` |
| 性能对比 | `cargo xtask perf list / run / compare / profile` |
| 依赖一致性 | `cargo run --locked -p xtask -- check-dependency-coherence` |
| 重定 GNU 基线（罕见） | `cargo xtask pin-reference --emacs PATH --reason "..."` |

## 12. 最后

- 目标是让每个结论都带证据（命令输出、C 行号、oracle 对比）；慢一点没关系。
- 任何本文件没覆盖的情况，按"先对齐 GNU、再最小改动、且可验证"三原则处理，
  并提醒用户可以把这个新规则补进本文件。
