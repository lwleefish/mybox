# Phase 4: 跨平台完善 (Cross-platform Completion) - Research

**Researched:** 2026-08-17
**Domain:** Windows CI verification (GitHub Actions), headless probe classification, DPI conversion verification, Phase 3 error-handling debt fixes
**Confidence:** HIGH (stack/CI) / HIGH (probe classification) / MEDIUM (DPI-on-CI limits)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-01:** Windows 验证通过 GitHub Actions CI runner（windows-latest），**暂不用真机测试**。开发机是 macOS（Darwin 24.6.0），无物理 Windows 机器、无远程仓库，CI 是唯一可验证途径。
- **D-02:** CI 验证程度 = 编译 + 单测 + headless 探针。托盘/截图实际捕获/窗口交互行为留待后续真机（成功标准 1-3 在 CI 上标记为「编译/单测/headless 通过 + 真机待验」）。
- **D-03:** headless 探针按依赖能力分拣：依赖真实屏幕捕获（xcap capture_image）、真实交互输入（鼠标坐标/点击）、真实热键的探针在 Windows CI 下 skip；窗口创建、注册、事件路由、纯逻辑探针在 CI 运行。判定机制用 `#[cfg(target_os)]` 组合探针自身能力探测（如捕获失败则 skip）。不使用手动白名单文件，不做失败即跳过。
- **D-04:** macOS 本地**不做** Windows 交叉编译验证，Windows 编译仅在 CI 进行（`cargo build --target x86_64-pc-windows-msvc`，target 已安装）。本地迭代不设 xwin/llvm-mingw。
- **D-05:** Phase 3 遗留「HotkeyManager 非 Send 延迟注册」（STATE.md:80）**记录 + CI 覆盖**，不在本 phase 修复核心框架（Rule 4 级改动）。CI 单测覆盖可注入场景或文档明确限制。
- **D-06:** DPI 工作 = **验证导向**，不是补 awareness。已证实 winit 0.30.13 在 Windows `EventLoop::new()` 时自动调用 `become_dpi_aware()`（`winit-0.30.13/src/platform_impl/windows/event_loop.rs:199` → `SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)`，dpi.rs:20-40），进程级 awareness 无需自补。范围为验证现有 scale_factor 换算在 Windows 高 DPI（如 150%）下选区与实际捕获一致，修正发现的偏差。
- **D-07:** 验证覆盖 `crates/modules/capture/src/capture.rs:29-42`（xcap 点坐标 × scale_factor 转物理像素，RESEARCH Pattern 3 标注的"唯一换算点"）与 `crates/modules/palette/src/position.rs` `compute_geometry`（已含 scale）在 Windows 下的一致性。混合 DPI 多屏、macOS Retina 不作为本 phase 强制范围。
- **D-08:** Phase 3 遗留 9 项**全部修复**（03-REVIEW.md）：WR-01 `App::create_window` 失败永久卡死 palette session（app.rs:518-521, session.rs:198-223）— 补 `on_create_failed` 回调；WR-02 `on_event` / `on_event_win` 模块回调无 `catch_unwind`，panic 杀死事件循环（app.rs:466-476）— 两处都包；WR-03 零命令 fallback 144px 被 128px 窗口截断（lib.rs:181, ui.rs:216-227）；IN-01 `run_command` 线程 spawn 失败在主线程 panic（command.rs:245）；IN-02 `SessionInner`/`egui_ctx` 锁序不一致但未文档化（session.rs:493-506, lib.rs:293-322）；IN-03 `PaletteHarness::realize_window` 注释不实（palette_checks.rs:81-124）；IN-04 `config_dir().unwrap_or_default()` 降级为空路径（app.rs:145-146）；IN-05 64 字符查询上限粘贴无反馈（ui.rs:175-207, session.rs:290-301）；IN-06 E2E 探针带几何/颜色魔法数字、位置盲（palette_checks.rs:2048-2066）。
- **D-09:** 9 项错误债**归入 04-02**（「DPI 缩放修复 + 错误处理打磨」），与 DPI 验证同计划，不新增 04-03，符合 ROADMAP 已定计划结构。

### the agent's Discretion
- CI workflow 的具体配置（setup-rust action、触发时机、缓存、`--target` 安装步骤）
- headless 探针能力分拣的具体实现（cfg 组合 vs 运行时能力探测的粒度）
- DPI 验证的具体探针/测试方法（如何在 CI 无真实捕获下验证换算逻辑——纯函数单测为主）
- WR/IN 各单项的具体修复实现（catch_unwind 放置、IN-04 bail vs 降级、IN-05 char_limit vs 放宽等）
- Windows 视觉等效（覆盖窗口盖过任务栏的层级方案、命令面板圆角是否复刻）— 未在讨论中选择，留给 researcher/planner 按平台惯例决定，不作为用户锁定决策
- CI 编译的 Windows target 是否需要额外步骤（MSVC linker 等由 setup-rust/actions 处理）

### Deferred Ideas (OUT OF SCOPE)
- Windows 真机交互验证（托盘图标实际显示、截图实际捕获画面、窗口交互）— 后续真机环境就绪后验收（CI 只做「编译/单测/headless 通过」标记）
- HotkeyManager 非 Send 延迟注册的核心框架修复（Rule 4 级改动）— 本 phase 只记录 + CI 覆盖，修复留待后续

</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| FRMW-06 | macOS 上应用以 Accessory 模式运行（不显示在 Dock），覆盖窗口可获取键盘焦点 | Phase 4 的 FRMW-06 关注点转向 **Windows 验证**：CI 编译 + 单测 + headless 探针（D-01/D-02）。Accessory 模式本身 Phase 1 已完成（app.rs:218 macOS ActivationPolicy 块）。Research：16 个探针中 15 个可在 Windows CI 运行（见 Pattern 1 分类表） |
| INFRA-04 | 应用配置文件存储在用户配置目录（macOS: ~/Library/Application Support/mybox/） | Phase 4 关联项为 **IN-04**（`config_dir().unwrap_or_default()` 降级为空路径，app.rs:145-146）。`directories` crate 跨平台路径已就位；修复 = 保留 Option 语义 + 显式错误处理，Windows 路径（`%APPDATA%\mybox\`）由 directories 自动处理 |

</phase_requirements>

## Summary

Phase 4 的核心事实是：**Windows 验证完全依赖 GitHub Actions CI**（D-01，且当前仓库无 remote，CI 只有在推送 GitHub 后才能运行）。research 证实 windows-latest runner 具有**交互式桌面会话**（虚拟显示器 1024x768 @ 100% DPI，非 Linux 式 headless）——winit 窗口创建在 CI 上可用，这与 D-03 的探针分拣假设兼容。对全部 16 个探针逐一核验后得到一个重要修正：**所有 palette 探针都是窗口创建 + 合成事件 + 内存 framebuffer 读取**（无 enigo、无 xcap 真实捕获、无 OS 热键注册），因此 **12/12 在 Windows CI 可运行**；capture 探针中仅 `enter_clipboard` 依赖真实系统剪贴板（能力探测后 skip），`overlay_capture` 用的是 fake capture（可运行），`drag_selection`/`esc_destroy` 是纯状态机（可运行）。D-03 的「真实捕获/真实输入/真实热键 skip」类别实际只命中 1 个探针。

第二个关键事实：**CI 显示器是 100% DPI，真实 150% 高 DPI 无法在 CI 验证**（D-06/D-07 的验证导向因此必然落地为纯函数单测）。`capture.rs:29-42` 的换算目前内联在 `capture_all_monitors` 中不可单测——需要先抽取纯函数再补 scale 1.0/1.25/1.5/2.0 单测；`position.rs::compute_geometry` 已是纯函数可直接补测。

第三个重要发现：**Windows CJK 字体缺口**——`fonts.rs` 在非 macOS 上是 no-op（fonts.rs:45-50，注释明确写「Windows font discovery is deferred to Phase 4」）。egui 内置字体无 CJK 字形，Windows 上所有中文命令名（开始截图、退出应用…）会渲染为豆腐块。成功标准 3（命令面板在 Windows 可唤出并执行）的功能不受阻，但 UX 完全不可用，且 `glyph_shape`/`keyword_highlight` 探针的像素断言依赖正确字形。建议 04-01 纳入 Windows 中文字体加载（Microsoft YaHei `msyh.ttc` + 回退链），属 discretion 范畴但强烈推荐。

**Primary recommendation:** 04-01 建单 job 的 windows-latest workflow（`dtolnay/rust-toolchain@stable` + `Swatinem/rust-cache@v2` + `taiki-e/install-action@nextest` + `cargo build --target x86_64-pc-windows-msvc` + `cargo nextest run` + 带能力探测的 headless 探针循环），并把 Windows CJK 字体加载纳入范围；04-02 按 D-08 修 9 项错误债（参考 03-REVIEW.md 的修复方向），DPI 验证以抽取换算纯函数 + 多 scale 单测实现。

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Windows 编译验证 | CI/CD（GitHub Actions windows-latest） | — | D-01/D-04 锁定：本地不交叉编译，Windows 编译只在 CI |
| headless 探针执行 | CI/CD | 模块层（probe bin） | 探针已在 capture_checks/palette_checks 中；CI 只负责按能力分拣后执行 |
| DPI 换算验证 | 模块层（capture/palette 单测） | CI/CD（跑单测） | 换算逻辑在模块内；CI 100% DPI 无法真实验证高 DPI，纯函数单测是唯一途径 |
| 错误处理债修复 | 核心层（mybox-core app.rs/command.rs） | 模块层（palette session/lib/ui） | WR-01/02/IN-01/04 在核心层，WR-03/IN-02/05/06 在 palette 模块 |
| Windows CJK 字体 | 模块层（palette fonts.rs） | — | 字体加载属于 palette 渲染链，cfg(target_os="windows") 分支 |
| 托盘/真机交互验证 | 真机（deferred） | — | D-02：CI 无 Explorer/taskbar，托盘无法在 CI 验证 |

## Standard Stack

### Core (CI tooling — no new Rust crates needed)

| Component | Version | Purpose | Why Standard |
|-----------|---------|---------|--------------|
| GitHub Actions `windows-latest` | current image | Windows 编译 + 测试 runner | D-01 锁定；有交互式桌面会话（1024x768 @ 100%）[CITED: actions/runner-images#2935] |
| `dtolnay/rust-toolchain@stable` | v2.x | Rust 工具链安装 | 社区标准 setup-rust action；`actions/checkout` 前置；支持 `targets:` 输入 [VERIFIED: dtolnay/rust-toolchain README] |
| `Swatinem/rust-cache@v2` | v2.9+ | cargo 依赖缓存 | 事实标准缓存 action；自动 key 于 rustc 版本 + Cargo.lock；workspace 自身产物不缓存 [VERIFIED: Swatinem/rust-cache README] |
| `taiki-e/install-action@nextest` | v2.x | cargo-nextest 安装 | nextest 官方文档指定安装方式；Windows 预编译二进制签名（SignPath）[VERIFIED: nexte.st/docs] |
| `actions/checkout@v4` | v4 | 检出代码 | GitHub 官方 |
| cargo-nextest | 0.9.x（本地 0.9.143） | 测试 runner | 03-VALIDATION 已采用；Windows 支持良好（0.9.140 为 CI 预编译版本）[VERIFIED: install-action manifest] |

### Supporting (Rust crates — all already in workspace, NO new dependencies)

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| winit | 0.30.13 | 窗口/事件循环（探针真实窗口） | 探针已依赖；Windows 自动 DPI awareness 已核实（D-06）[VERIFIED: vendored source] |
| softbuffer | 0.4+ | framebuffer 合成（探针渲染） | 已依赖 |
| xcap | 0.4+ | 屏幕捕获 | CI 上不执行真实捕获（D-02/D-03） |
| arboard | 3.4+ | 剪贴板（enter_clipboard 探针） | 仅该探针使用；Windows CI 需能力探测 |
| tiny-skia | 0.11+ | 2D 渲染 | 已依赖，无变更 |

**Installation:** 无新 crate。CI 依赖通过 GitHub Actions 引入（见 Code Examples）。

**Version verification:** 全部 CI action 版本已通过官方 README/文档核实（dtolnay/rust-toolchain、Swatinem/rust-cache、taiki-e/install-action、actions/checkout）。cargo-nextest Windows 预编译版本 0.9.140 [VERIFIED: install-action manifest]。

## Package Legitimacy Audit

> 本 phase **不安装任何新 Rust crate**——所有依赖已在 workspace（macOS 专属依赖全部 `[target.'cfg(target_os = "macos")']` gated，Windows 可编译已核实）。下表审计的是 CI 引入的第三方 GitHub Actions。

| Package | Registry | Age | Downloads | Source Repo | slopcheck | Disposition |
|---------|----------|-----|-----------|-------------|-----------|-------------|
| dtolnay/rust-toolchain | GitHub Actions | 5+ yrs | 广泛使用 | github.com/dtolnay/rust-toolchain | n/a（非 crates） | Approved — [VERIFIED: official README] |
| Swatinem/rust-cache | GitHub Actions | 5+ yrs | 1.8k stars | github.com/Swatinem/rust-cache | n/a | Approved — [VERIFIED: official README] |
| taiki-e/install-action | GitHub Actions | 4+ yrs | 广泛使用 | github.com/taiki-e/install-action | n/a | Approved — [VERIFIED: official README + nextest docs] |
| actions/checkout | GitHub Actions | 8+ yrs | GitHub 官方 | github.com/actions/checkout | n/a | Approved — [VERIFIED: GitHub official] |

**Packages removed due to slopcheck [SLOP] verdict:** none（本 phase 无新 crate 安装）
**Packages flagged as suspicious [SUS]:** none

*说明：slopcheck 面向语言生态包（npm/PyPI/crates），GitHub Actions 不属于其范畴。上述 action 均经官方文档核实且为社区事实标准，等效 [OK]。建议 planner 在生产 workflow 中对第三方 action 使用 commit SHA pin（Rust Project Primer 最佳实践），见 Security Domain。*

## Architecture Patterns

### System Architecture Diagram

```
                    ┌─────────────────────────────────────────────────┐
                    │         GitHub Actions workflow (04-01)          │
                    │                                                 │
  push to GitHub ──►│  windows-latest job                             │
 (requires remote)  │  ┌───────────────────────────────────────────┐  │
                    │  │ 1. actions/checkout@v4                    │  │
                    │  │ 2. dtolnay/rust-toolchain@stable          │  │
                    │  │ 3. Swatinem/rust-cache@v2                 │  │
                    │  │ 4. taiki-e/install-action@nextest         │  │
                    │  │ 5. cargo build --target                   │  │
                    │  │    x86_64-pc-windows-msvc  ◄── D-04       │  │
                    │  │ 6. cargo nextest run        ◄── 单测      │  │
                    │  │ 7. headless probe loop      ◄── D-02/D-03 │  │
                    │  │    capture_checks:         ┌───────────┐  │  │
                    │  │      overlay_capture  RUN  │ capability│  │  │
                    │  │      drag_selection   RUN  │  probe    │  │  │
                    │  │      esc_destroy      RUN  │ (clipboard│  │  │
                    │  │      enter_clipboard  ────►│  fail →   │  │  │
                    │  │      (SKIP on failure)     │  skip)    │  │  │
                    │  │    palette_checks:     └───────────┘  │  │  │
                    │  │      12 probes ALL RUN                │  │  │
                    │  └───────────────────────────────────────────┘  │
                    └─────────────────────────────────────────────────┘
                                        │
                                        ▼
               ┌──────────────────────────────────────────────────────┐
               │  Pure-function DPI unit tests (04-02, runs locally   │
               │  AND in CI — CI display is 100% DPI so real 150%     │
               │  cannot be tested there)                              │
               │  ┌──────────────────┐   ┌──────────────────────────┐  │
               │  │ capture.rs       │   │ position.rs              │  │
               │  │ extract pure fn: │   │ compute_geometry (pure)  │  │
               │  │ x'=round(x*scale)│   │ + unit tests @1.0/1.5/2.0│  │
               │  └──────────────────┘   └──────────────────────────┘  │
               └──────────────────────────────────────────────────────┘
```

### Recommended Project Structure

```
.github/
└── workflows/
    └── windows-ci.yml      # 新建 — Phase 4 唯一的新增文件
crates/mybox-core/src/
├── app.rs                  # WR-01 on_create_failed / WR-02 catch_unwind / IN-04 config_dir
├── command.rs              # IN-01 spawn 失败处理
crates/modules/capture/src/
├── capture.rs              # D-07: 抽取换算纯函数 + 单测（新）
crates/modules/palette/src/
├── fonts.rs                # Windows CJK 字体加载（新 cfg 分支，推荐纳入）
├── lib.rs                  # WR-03 zero-command fallback
├── session.rs              # WR-01 回调 / IN-02 锁序文档 / IN-05 char limit
├── ui.rs                   # WR-03 / IN-05
├── position.rs             # D-07: compute_geometry 补 scale 单测
└── bin/palette_checks.rs   # IN-03 注释 / IN-06 魔法数字
```

### Pattern 1: Probe capability classification (D-03)

**What:** 每个探针按「依赖的能力」分拣：依赖真实屏幕捕获 / 真实输入 / 真实热键 → Windows CI skip；窗口创建、注册、事件路由、纯逻辑 → 运行。判定机制 = `#[cfg(target_os)]` + 探针自身能力探测（如剪贴板打开失败则 skip），**不用手动白名单，不做失败即跳过**。

**When to use:** 所有 headless 探针的执行入口。

**Verified probe inventory (from source, 2026-08-17):**

| 探针 | 真实依赖 | Windows CI | 依据（源码核实） |
|------|---------|-----------|-----------------|
| palette summon_render | 窗口创建 + 合成键事件 | **RUN** | PaletteHarness::realize_window → el.create_window；press_key → on_palette_key（合成） |
| palette fuzzy_navigation_execute | 窗口 + 合成事件 | **RUN** | 同 harness 模式 |
| palette capture_hides_first | 无（纯逻辑 + fake runner） | **RUN** | check_capture_hides_palette_first:588 — 无 event loop，gated_runner 队列断言 |
| palette five_summon_esc_no_residue | 窗口 + 合成 ESC | **RUN** | summon_palette + harness |
| palette consecutive_summon_close | 窗口 | **RUN** | harness + realize_window |
| palette glyph_shape | 窗口 + **内存 framebuffer** 读取 | **RUN** | with_framebuffer（非 xcap）—— 无真实捕获 |
| palette position_stable_on_filter | 窗口 + framebuffer | **RUN** | assert_framebuffer_covers:1116 |
| palette hover_click_alignment | 窗口 + **合成** MouseInput 事件 | **RUN** | 注入 winit WindowEvent::MouseInput（非 enigo/OS 鼠标）；注释 1370-1371 明确「OS 级物理鼠标不覆盖」 |
| palette ctrl_pn_navigation | 窗口 + 合成键 | **RUN** | press_key |
| palette ime_commit_updates_input | 窗口 + 合成 IME 事件 | **RUN**（若 Windows 上 flaky 则能力门控） | 合成 WindowEvent::Ime；Windows IME 路径与 macOS 不同，标 MEDIUM 风险 |
| palette keyword_highlight | 窗口 + framebuffer | **RUN** | with_framebuffer:2049 |
| palette click_hide_before_capture | 窗口 + 合成鼠标 | **RUN** | 合成 MouseInput:2397 |
| capture overlay_capture | 窗口 + **fake capture**（非 xcap） | **RUN** | capture_checks.rs:87 "Fake capture: opaque fill + mask"；仅窗口+softbuffer 合成 |
| capture drag_selection | **纯状态机**（无窗口无输入） | **RUN** | capture_checks.rs:141 — 直接调 session.on_mouse_down/move/up |
| capture esc_destroy | **纯状态机** | **RUN** | capture_checks.rs:203 — session.cancel() 断言 |
| capture enter_clipboard | **真实系统剪贴板**（arboard） | **SKIP**（能力探测：arboard::Clipboard::new() 失败 → skip，非 fail） | capture_checks.rs:163 — 真实复制+读回 |

**关键修正：** CONTEXT 假设「依赖真实捕获/输入/热键的探针较多」，实际核验后仅 `enter_clipboard` 命中 skip 类别（且用能力探测即可）。所有 palette 探针读内存 framebuffer 而非屏幕，`overlay_capture` 用 fake capture。**无任何探针注册 OS 热键**（palette_checks.rs:752-754 注释证实 summon 走 bus 层非物理热键路径）。

**Example (capability-gated skip):**
```rust
// capture_checks.rs — enter_clipboard 的 Windows CI 能力探测模式（D-03 机制）
fn check_enter_clipboard() -> Result<(), String> {
    // 能力探测：Windows CI 会话可能无可用剪贴板 — 打开失败即 skip（非 fail）
    if std::env::consts::OS == "windows" && arboard::Clipboard::new().is_err() {
        println!("capture_checks 'enter_clipboard': SKIPPED (no clipboard in CI session)");
        return Ok(());
    }
    // ... 原有断言逻辑不变
}
```
*Source: 基于 [CITED: nexte.st/docs] + D-03 机制的推荐实现模式（discretion 范畴）*

### Pattern 2: DPI conversion pure-function extraction (D-06/D-07)

**What:** `capture.rs:29-42` 的换算逻辑当前内联在 `capture_all_monitors` 中，无法脱离 xcap 单测。抽取纯函数后可在任意 scale 下单测，且不改变生产行为。

**When to use:** DPI 验证的唯一 CI 可行途径（CI 显示器 100% DPI）。

**Example:**
```rust
// capture.rs — 抽取后（新函数，保持原 .round() 语义，Source: capture.rs:38-39 现有代码）
/// 点坐标 × scale_factor → 物理像素（RESEARCH Pattern 3 唯一换算点，D-07 验证对象）
pub fn point_to_physical(x: f64, y: f64, scale: f64) -> (i32, i32) {
    ((x * scale).round() as i32, (y * scale).round() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scale_1_0_identity() { assert_eq!(point_to_physical(100.0, 200.0, 1.0), (100, 200)); }
    #[test]
    fn scale_1_5_rounds() { assert_eq!(point_to_physical(10.0, 10.0, 1.5), (15, 15)); }
    #[test]
    fn scale_2_0_doubles() { assert_eq!(point_to_physical(-5.0, 3.0, 2.0), (-10, 6)); }
    #[test]
    fn scale_1_25_fractional() { assert_eq!(point_to_physical(3.0, 3.0, 1.25), (4, 4)); }
}
```
*Source: 基于 [VERIFIED: capture.rs:29-42] 的抽取建议（discretion 范畴）*

### Pattern 3: Windows CJK font loading (recommended 04-01 addition)

**What:** `fonts.rs` 目前非 macOS 为 no-op。Windows 需要加载系统中文字体（Microsoft YaHei）到 egui Proportional family 头部，复用现有 TTC face-index 模式。

**When to use:** 04-01 中实现，否则 Windows 中文命令名渲染为豆腐块（成功标准 3 的 UX 缺口）。

**Example (recommended shape, following fonts.rs macOS pattern):**
```rust
// fonts.rs — 新 #[cfg(target_os = "windows")] 分支（Source: fonts.rs:16-43 macOS 模式）
#[cfg(target_os = "windows")]
pub fn install_cjk_fonts(ctx: &egui::Context) -> anyhow::Result<()> {
    // 回退链：msyh.ttc (YaHei) → simhei.ttf → simsun.ttc；均为 Windows 标准字体目录
    for path in [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\simsun.ttc",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            let mut defs = egui::FontDefinitions::default();
            defs.font_data
                .insert("cjk".to_string(), egui::FontData::from_owned(bytes).into());
            if let Some(family) = defs.families.get_mut(&egui::FontFamily::Proportional) {
                family.insert(0, "cjk".to_string());
            }
            ctx.set_fonts(defs);
            return Ok(());
        }
    }
    anyhow::bail!("no CJK font found in C:\\Windows\\Fonts")
}
```
*Source: 模式基于 [VERIFIED: fonts.rs:16-43]（macOS 分支现有代码）；文件路径 [ASSUMED]（微软标准字体名，需真机/CI 验证存在性）*

### Anti-Patterns to Avoid
- **失败即跳过（fail-to-skip）：** D-03 明令禁止——探针断言失败必须 FAIL，只有**能力探测失败**（如剪贴板打不开、窗口创建因环境缺失）才 skip。区分方式：能力探测在断言逻辑**之前**独立执行。
- **手动白名单文件：** D-03 禁止维护「哪些探针在哪些平台跑」的清单。用 `#[cfg(target_os)]` + 运行时能力探测表达。
- **在 CI 尝试真实托盘验证：** GitHub Actions Windows runner 无 Explorer/taskbar 外壳，托盘图标无法显示。不做、不探。
- **为 CI 写专属测试路径：** 探针必须在本地 macOS 与 CI Windows 用同一入口跑，避免「CI 专用测试」分叉。

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Rust 工具链安装（CI） | 手写 rustup install 脚本 | `dtolnay/rust-toolchain@stable` | 社区标准；处理缓存 key、targets 输入、MSVC linker 环境 |
| cargo 依赖缓存 | 手写 actions/cache 配置 | `Swatinem/rust-cache@v2` | 自动 key 于 rustc + Cargo.lock；清理 workspace 自身产物（手写会缓存膨胀）；处理 macOS cache corruption workaround |
| cargo-nextest 安装 | cargo install 源码编译（CI 上 3-5 分钟） | `taiki-e/install-action@nextest` | 预编译二进制 + SHA256 checksum + 签名；nextest 官方文档指定方式 |
| 窗口创建失败恢复 | 让事件循环挂死 | `on_create_failed` 回调（WR-01） | 回调模式已在 app.rs 设计意图中，只是缺实现 |
| 模块回调 panic 防护 | 每个模块自己 try/catch | `std::panic::catch_unwind` 包在 on_event/on_event_win 外层（WR-02） | 事件循环是单点故障，必须在框架层统一防护 |

**Key insight:** 本 phase 的「标准栈」是 CI 惯例而非新库。所有 crate 依赖已就位且 Windows-clean，**不要引入任何新 Rust 依赖**——错误债修复全部用 std（catch_unwind）+ 现有结构完成。

## Runtime State Inventory

> 本 phase 为错误修复 + CI 搭建 + 验证导向的 DPI 工作，无重命名/迁移。以下类别经核实均无遗留状态。

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | None — 无数据库/持久化存储涉及（配置 TOML 由 ConfigCenter 管理，无字符串重命名） | 无 |
| Live service config | None — 无外部服务配置引用本 phase 改动内容 | 无 |
| OS-registered state | None — 无 OS 级注册涉及（不注册服务/任务/plist） | 无 |
| Secrets/env vars | None — 本 phase 无 secret 或 env 变更（CI 无 secrets 需求；workflow 用公开 checkout） | 无 |
| Build artifacts | 注意：CI 新增 `--target x86_64-pc-windows-msvc` 构建产物写入本地 `target/` 子目录；`.gitignore` 已忽略 target/（需确认） | 无需操作（CI 每次全新 runner） |

## Common Pitfalls

### Pitfall 1: CI 显示器 100% DPI，误以为可验证真实高 DPI
**What goes wrong:** 在 CI 上跑「高 DPI 选区一致性」探针，实际 runner 是 1024x768 @ 100%，scale_factor 恒为 1.0，探针永远通过但什么都没验证。
**Why it happens:** GitHub Actions windows runner 虚拟显示器固定分辨率、默认 100% 缩放 [CITED: actions/runner-images#2935]。
**How to avoid:** DPI 验证 = 纯函数单测（`point_to_physical` @ 1.0/1.25/1.5/2.0 + `compute_geometry` @ 1.5）。真实 150% 行为留真机（D-06/D-07 已定义）。
**Warning signs:** 探针中读取 `Monitor::scale_factor()` 并断言 >1.0——CI 上必然失败或恒等。

### Pitfall 2: 把 enter_clipboard 当窗口探针跑（或当普通失败处理）
**What goes wrong:** arboard 在 CI 服务会话可能打不开剪贴板——直接跑会 FAIL 且难以复现；但若把失败当 skip 又会掩盖真实回归。
**Why it happens:** 剪贴板是真实系统资源，CI runner 会话的剪贴板可用性不受控。
**How to avoid:** 能力探测与断言分离：`arboard::Clipboard::new()` 失败 → SKIP（输出明确标记）；成功 → 走完整断言，断言失败 = FAIL。
**Warning signs:** CI 上 enter_clipboard 偶发失败且本地 macOS 复现不了。

### Pitfall 3: 探针依赖隐式真实输入（enigo/物理鼠标）而不自知
**What goes wrong:** 认为 hover_click_alignment 需要真实鼠标，实际它注入合成 winit 事件（注释 1370-1371 明确 OS 物理鼠标不覆盖）——错误地 skip 会丢失 CI 覆盖。
**Why it happens:** 探针命名（"click"、"drag"）暗示物理交互。
**How to avoid:** 按源码核实（本 research 已完成分类表）；planner 以分类表为准，不凭命名判断。
**Warning signs:** 探针名字含 click/drag/key 但函数体只用 winit 合成事件。

### Pitfall 4: Windows 中文豆腐块（CJK 字体缺口）
**What goes wrong:** Windows 上 palette 所有中文命令名渲染为 □，glyph_shape/keyword_highlight 像素断言失败或失真。
**Why it happens:** fonts.rs 非 macOS 是 no-op（fonts.rs:45-50），egui 内置字体无 CJK。
**How to avoid:** 04-01 实现 Windows 字体加载（Pattern 3）；若决定推迟，必须在 SUCCESS 标准里明确「Windows 中文显示 = 真机待验」并降低 glyph 探针断言。
**Warning signs:** CI 上 palette 探针的像素断言与 macOS 阈值不一致。

### Pitfall 5: workflow 用可变 tag 引用第三方 action
**What goes wrong:** `@stable`/`@v2` 是可变引用，action 维护者可推新代码到同一 tag——CI 行为无警告变化（供应链风险 + 复现性问题）。
**Why it happens:** README 示例都用 tag，方便但非生产最佳实践。
**How to avoid:** 生产 workflow 用 commit SHA pin（`actions/checkout@b4ffde6… # v4.1.1`），Dependabot/Renovate 自动更新。
**Warning signs:** 所有 action 引用都是裸 tag。

### Pitfall 6: 忘记仓库无 remote，CI 无法触发
**What goes wrong:** 计划「推送验证 CI」，但 `git remote -v` 为空——必须先建 GitHub 仓库并 push。
**Why it happens:** D-01 已知无远程仓库。
**How to avoid:** 04-01 计划中显式包含「创建 GitHub 仓库 + push」前置步骤（用户确认后）；在 push 前，CI workflow 用本地工具验证 YAML 语法（`actionlint`）。
**Warning signs:** 执行时 `git push` 报 no remote。

## Code Examples

Verified patterns from official sources:

### GitHub Actions Windows CI workflow (04-01 core deliverable)
```yaml
# .github/workflows/windows-ci.yml
# Source: 组合 dtolnay/rust-toolchain README + nexte.st/docs + rustprojectprimer.com/ci/github
name: windows-ci

on:
  push:
    branches: [main]
  pull_request:

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

env:
  CARGO_TERM_COLOR: always

jobs:
  windows:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4

      # D-04: 只用 CI 编译 Windows target（宿主即 x86_64-pc-windows-msvc，无需额外 target 安装）
      - uses: dtolnay/rust-toolchain@stable

      - uses: Swatinem/rust-cache@v2

      - name: Install nextest
        uses: taiki-e/install-action@nextest

      # D-04 + D-02: 编译验证（显式 --target 明确验证 Windows 目标）
      - name: Build (Windows target)
        run: cargo build --target x86_64-pc-windows-msvc --locked

      # D-02: 单测
      - name: Unit tests
        run: cargo nextest run --locked

      # D-02/D-03: headless 探针（窗口创建 + 合成事件 + 能力探测 skip）
      - name: Capture headless probes
        shell: bash
        run: |
          for check in overlay_capture drag_selection esc_destroy enter_clipboard; do
            echo "== capture_checks $check =="
            cargo run -p mybox-capture --bin capture_checks -- "$check" || exit 1
          done

      - name: Palette headless probes
        shell: bash
        run: |
          for check in summon_render fuzzy_navigation_execute capture_hides_first \
                       five_summon_esc_no_residue consecutive_summon_close glyph_shape \
                       position_stable_on_filter hover_click_alignment ctrl_pn_navigation \
                       ime_commit_updates_input keyword_highlight click_hide_before_capture; do
            echo "== palette_checks $check =="
            cargo run -p mybox-palette --bin palette_checks -- "$check" || exit 1
          done
```

### catch_unwind wrapper for module callbacks (WR-02)
```rust
// app.rs — on_event/on_event_win 两处统一包装（Source: D-08 + std::panic docs）
let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    module.on_event_win(&event)
}));
if let Err(panic) = result {
    // 记录 panic 载荷 + 模块 id，事件循环继续存活
    log::error!("module {:?} panicked in on_event_win: {:?}", module.id(), panic);
}
```
*Source: [CITED: doc.rust-lang.org/std/panic/fn.catch_unwind] — 模式建议（discretion）*

### Error-downgrade → explicit error (IN-04)
```rust
// app.rs — config_dir 保留 Option 语义，调用侧显式处理（D-08 IN-04 方向）
pub fn config_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "mybox")
        .map(|d| d.config_dir().to_path_buf())
}

// 调用侧：不存在 → 显式 warn + 使用默认配置（而非静默空路径）
match config::config_dir() {
    Some(dir) => { /* 正常路径 */ }
    None => { log::warn!("no config dir — using defaults"); /* 纯默认配置 */ }
}
```
*Source: 基于 [VERIFIED: app.rs:145-146 现状] 的修复方向（discretion）*

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Windows 验证靠真机/虚拟机 | GitHub Actions windows-latest CI（交互式桌面会话） | 2019+（runner 常驻）；本 phase 2026-08 采用 | CI 可跑窗口创建类探针；托盘/真实捕获仍须真机 |
| dtolnay/rust-toolchain 仅装 toolchain | 同一 action 支持 `targets:` 多目标 | 持续演进 | 无需额外 rustup target add 步骤（宿主 target 已内建） |
| cargo test 跑全部 | cargo-nextest（快、并行、Windows 支持） | 2022+ 主流 | 03 已采用，本 phase CI 沿用 |
| 手写 actions/cache 缓存 target/ | Swatinem/rust-cache 自动 key + 清理 | 2020+ 事实标准 | 缓存不膨胀、Cargo.lock 变更精准失效 |
| 探针真实输入（enigo/物理鼠标） | 合成 winit 事件 + 内存 framebuffer 断言 | Phase 3 已采用 | 探针可在 CI 运行（D-03 分拣几乎全绿） |

**Deprecated/outdated:**
- **actions-rs/toolchain（旧 setup-rust action）**: 已弃用多年，维护停止——用 `dtolnay/rust-toolchain` 替代 [CITED: dtolnay/rust-toolchain README]。
- **cargo install cargo-nextest（CI 安装方式）**: 被 `taiki-e/install-action@nextest` 预编译二进制替代（编译 3-5 分钟 → 秒级）[VERIFIED: nexte.st/docs]。

## Assumptions Log

> 所有 [ASSUMED] 标注的声明集中于此。planner/discuss-phase 需用户确认后才能锁定。

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | GitHub Actions windows-latest 具有交互式桌面会话，winit 窗口可创建（1024x768 @ 100% DPI） | Summary / Patterns | runner 若为纯服务会话则窗口探针全挂 → 需退化为纯逻辑探针 + 真机验证（备选方案见 Open Q3） |
| A2 | 托盘图标无法在 GitHub Actions Windows runner 验证（无 Explorer/taskbar） | Pitfalls | 若 runner 实际有 shell，可加托盘注册探针（但 D-02 已将其留真机，影响小） |
| A3 | Windows CI 会话中 arboard 剪贴板可能不可用 → enter_clipboard 需能力探测 | Patterns | 若剪贴板恒可用，能力探测不触发，探针照常跑——无风险 |
| A4 | Microsoft YaHei 位于 `C:\Windows\Fonts\msyh.ttc`（simhei.ttf / simsun.ttc 作回退） | Patterns (CJK) | 若路径/字体名不符 → 回退链兜底；最坏情况 CJK 加载失败 → warn-and-continue（现有 macOS 同语义） |
| A5 | CI 上 ime_commit_updates_input 探针在 Windows 的行为与 macOS 一致（合成 IME 事件路径相同） | Patterns 分类表 | Windows winit IME 合成事件处理可能不同 → 探针 FAIL → 需能力门控或降级为 macOS-only（标记 MEDIUM 风险） |
| A6 | 仓库创建 + push 后 CI 才可运行；在此之前 workflow 只能本地语法校验 | Environment | push 前无任何 Windows 验证反馈——执行顺序必须先建仓库（需用户提供 GitHub 账号/仓库名） |
| A7 | `--locked` 在 CI 使用 Cargo.lock 无冲突（lockfile 已在仓库） | Code Examples | 若 lockfile 缺失/过期 → CI FAIL → 先 `cargo generate-lockfile` 提交 |

## Open Questions (RESOLVED)

> 四项开放问题均已在计划阶段解决（2026-08-17）。**RESOLVED** 标注与对应计划落点如下，供执行器/后续 phase 追溯。

1. **Windows CJK 字体加载是否纳入 04-01 范围？**
   - What we know: fonts.rs 非 macOS no-op；egui 无 CJK；Windows 中文命令名会豆腐块；成功标准 3 功能不受阻但 UX 不可用；glyph 探针像素断言依赖字形
   - What's unclear: CONTEXT 未将此项列为决策（discretion 范畴）；是否影响「成功标准」判定
   - Recommendation: **纳入 04-01**（Pattern 3 实现，~1 个任务）；若用户选择推迟，需在 04-UAT 明确「Windows 中文显示 = 真机待验」并放宽 glyph 探针
   - **RESOLVED: 纳入 04-01。** 04-01 Task 3 锁定 fonts.rs `#[cfg(target_os = "windows")]` 分支（msyh.ttc → simhei.ttf → simsun.ttc 回退链）+ `#[cfg(all(test, target_os = "windows"))]` 测试（has_glyphs("截图") false → true，CI Windows 跑）。

2. **GitHub 仓库创建流程**
   - What we know: 无 remote（D-01）；gh CLI 2.87.3 已安装可用
   - What's unclear: 仓库名、可见性、归属（个人/org）、是否 push 现有历史
   - Recommendation: 04-01 首任务 = 用户确认仓库参数 + `gh repo create` + `git push`；push 前用 actionlint 校验 workflow 语法
   - **RESOLVED: 04-01 Task 1 落实。** 默认参数已锁定（个人账号、公开、名 mybox，依据 PROJECT.md「面向个人使用，开源」）；冲突/归属不明时创建 checkpoint:decision 由用户裁决；push 前 `git ls-files` 审查敏感文件。

3. **ime_commit_updates_input 在 Windows CI 的稳定性**
   - What we know: 合成 IME 事件；Windows winit IME 管线与 macOS 不同
   - What's unclear: 合成 `WindowEvent::Ime` 在 Windows 上是否等效触发 egui-winit 的 IME 路径
   - Recommendation: 先按 RUN 进 CI；若首次运行 FAIL 且确认为环境差异 → 能力门控（Windows 上探测 IME 可用性后 skip），保留 macOS 全覆盖
   - **RESOLVED: 按推荐执行（A5 处置路径已写入计划）。** 04-01 Task 2 将 ime_commit_updates_input 先按 RUN 进 CI；首次 CI run 失败且确认为 Windows 环境差异（非代码回归）→ 探针内加能力门控（保留 macOS 全覆盖）并记录 SUMMARY。

4. **是否添加 macOS job 做回归对照？**
   - What we know: D-01 只要求 Windows CI；macOS 验证本地已覆盖
   - What's unclear: 双 job 的 CI 成本 vs 回归价值
   - Recommendation: 本期只建 windows job（贴合 D-01）；workflow 结构留好 matrix 扩展位
   - **RESOLVED: 本期只建 windows job。** 04-01 Task 2 单 job 结构（D-01 贴合）；workflow 保留 matrix 扩展位（不建 macOS job，本地 macOS 回归已由全仓 nextest 覆盖）。

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| cargo / rustc (stable) | 本地单测 + CI | ✓ | cargo 1.97.1 | — |
| x86_64-pc-windows-msvc target | CI 编译（D-04） | ✓（本地已装，CI 宿主自带） | — | — |
| cargo-nextest | 单测 | ✓ | 0.9.143（本地）/ CI 用 install-action | `cargo test` |
| gh CLI | 仓库创建 + push（Open Q2） | ✓ | 2.87.3 | 浏览器手动创建 |
| git remote | CI 触发 | ✗（无 remote） | — | 必须先 `gh repo create` + push（阻塞项） |
| GitHub Actions runner | Windows 验证 | ✗（远程服务，push 后才可用） | — | 本地无法模拟 Windows 编译（D-04 明确不交叉编译） |
| actionlint | workflow 语法校验（push 前） | ✗ 未安装 | — | `brew install actionlint` 或 YAML 解析器兜底 |
| Windows 真机 | 托盘/真实捕获/交互验证 | ✗（deferred） | — | D-02 标记「真机待验」 |

**Missing dependencies with no fallback:**
- **git remote / GitHub 仓库** — CI 触发的硬前置，04-01 首任务必须解决（用户确认仓库参数后 `gh repo create`）
- **Windows 真机** — 成功标准 1-3 的最终验收（本 phase 明确 deferred，CI 只做「编译/单测/headless 通过」标记）

**Missing dependencies with fallback:**
- actionlint — `brew install actionlint`（一次性，可作 04-01 任务）

## Validation Architecture

> `.planning/config.json` 中 `workflow.nyquist_validation: true` → 本节必须。采用与 03-VALIDATION 一致的契约结构。

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo-nextest（本地 0.9.143 / CI 0.9.140+ via `taiki-e/install-action@nextest`）+ `cargo test -- --ignored`（显示/OS 级） |
| Config file | 无 `.config/nextest.toml`（03 未建；本期不新增，沿用默认 profile） |
| Quick run command | `cargo nextest run -p mybox-core -p mybox-palette -p mybox-capture` |
| Full suite command | `cargo nextest run && cargo test -- --ignored`（本地 macOS）；CI 为 workflow 内步骤组合 |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| FRMW-06（Windows 编译） | workspace 在 x86_64-pc-windows-msvc 可编译 | CI 编译 | `cargo build --target x86_64-pc-windows-msvc --locked`（CI job） | ❌ Wave 0（workflow 文件） |
| FRMW-06（单测） | 全仓单测在 Windows 绿 | CI 单测 | `cargo nextest run --locked`（CI job） | ❌ Wave 0（workflow 文件） |
| FRMW-06（headless 探针） | 16 探针按 D-03 分类执行 | CI 探针循环 | workflow 内 `cargo run -p mybox-* --bin *_checks -- <check>` | ✅（probe bins 已存在） |
| D-07（capture 换算） | `point_to_physical` 在 scale 1.0/1.25/1.5/2.0 正确 | unit | `cargo nextest run -p mybox-capture point_to_physical` | ❌ Wave 0（新抽取函数 + 测试） |
| D-07（palette geometry） | `compute_geometry` @ scale 1.5 选区一致 | unit | `cargo nextest run -p mybox-palette position::tests` | ✅（position.rs 已有测试，需补 scale 用例） |
| WR-01 | create_window 失败 → on_create_failed 回调 → session 不卡死 | unit | `cargo nextest run -p mybox-core app::tests::create_failed` | ❌ Wave 0 |
| WR-02 | 模块回调 panic 被 catch_unwind 捕获，循环存活 | unit | `cargo nextest run -p mybox-core app::tests::panic_isolated` | ❌ Wave 0 |
| WR-03 | 零命令 fallback 高度 ≤ 窗口高度（不截断） | unit | `cargo nextest run -p mybox-palette ui::tests::effective_height_zero_commands_uses_empty` | ❌ Wave 0 |
| IN-01 | spawn 失败返回 Err 而非主线程 panic | unit | `cargo nextest run -p mybox-core command::tests::spawn_failure` | ❌ Wave 0 |
| IN-02 | 锁序文档化（assert 或注释） | unit/文档 | `cargo nextest run -p mybox-palette session::tests`（现网）+ 文档审查 | ✅（现有测试覆盖行为；文档为手动项） |
| IN-03 | realize_window 注释与实际一致 | 文档（手动） | 代码审查 | ✅（注释编辑） |
| IN-04 | config_dir 缺失 → 显式 warn + 默认配置（无空路径） | unit | `cargo nextest run -p mybox-core command::tests::no_config_dir_builtins_bail` | ❌ Wave 0 |
| IN-05 | 超 64 字符粘贴 → 截断 + 反馈 | unit | `cargo nextest run -p mybox-palette session::tests::input_limit` | ❌ Wave 0 |
| IN-06 | E2E 探针魔法数字抽取为具名常量 | unit/探针重构 | `cargo nextest run -p mybox-palette` + 探针重跑 | ✅（重构，行为不变） |
| CJK（若纳入 04-01） | Windows 字体加载后 has_glyphs("截图") 为真 | unit（cfg windows） | `cargo nextest run -p mybox-palette fonts::tests`（CI Windows 跑） | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo nextest run -p mybox-core -p mybox-palette -p mybox-capture`
- **Per wave merge:** `cargo nextest run && cargo test -- --ignored`（本地）+ workflow 全步骤（若已 push）
- **Phase gate:** 全绿后 `/gsd-verify-work`；CI 绿（若仓库已就绪）为 FRMW-06 验收前提

### Wave 0 Gaps
- [ ] `.github/workflows/windows-ci.yml` — 覆盖 FRMW-06 编译/单测/探针三步骤
- [ ] `crates/modules/capture/src/capture.rs` — 抽取 `point_to_physical` 纯函数 + 4 scale 用例
- [ ] `crates/modules/palette/src/position.rs` — 补 `compute_geometry` @ scale 1.5 用例
- [ ] `crates/mybox-core/src/app.rs` — WR-01/WR-02/IN-04 测试（create_failed / panic_isolated / no_config_dir）
- [ ] `crates/mybox-core/src/command.rs` — IN-01 spawn_failure 测试
- [ ] `crates/modules/palette/src/{lib,ui,session}.rs` — WR-03/IN-05 测试
- [ ] `crates/modules/palette/src/bin/palette_checks.rs` — IN-06 重构（魔法数字 → 常量）
- [ ] `crates/modules/palette/src/fonts.rs` — Windows CJK 分支 + 测试（若纳入）
- [ ] 无框架安装缺口（nextest 已用；无需新测试框架）

## Security Domain

> `security_enforcement` 未在 config.json 显式 false → 默认启用，本节必须。本 phase 为 CI 搭建 + 错误处理修复，威胁面集中在 CI 供应链与 panic 韧性。

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | —（桌面工具无认证） |
| V3 Session Management | no | — |
| V4 Access Control | no | —（本地单用户工具） |
| V5 Input Validation | yes | IN-05（64 字符上限 + 截断反馈）；egui TextEdit 已有上限 |
| V6 Cryptography | no | —（无加密需求） |
| V8 Error Handling (延伸) | yes | WR-01/02 + IN-01：失败显式、panic 隔离、不静默降级 |

### Known Threat Patterns for {GitHub Actions CI + Rust event loop}

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| 第三方 action 供应链篡改（可变 tag 指向新代码） | Tampering | commit SHA pin + Dependabot/Renovate 自动更新（Rust Project Primer 最佳实践）[CITED: rustprojectprimer.com/ci/github] |
| 模块回调 panic 杀死事件循环（单点故障） | DoS | `catch_unwind` 包 on_event/on_event_win（WR-02）—— panic 记录后循环存活 |
| create_window 失败永久卡死 palette | DoS | `on_create_failed` 回调（WR-01）—— 失败显式通知而非挂死 |
| 剪贴板越权读取（enter_clipboard 探针） | Information Disclosure | 探针只读回自己刚写入的 2x2 图像并断言尺寸；不读取/记录剪贴板内容 |
| CI 日志泄露敏感信息 | Information Disclosure | 本 phase workflow 无 secrets；探针输出仅含几何/像素统计，不含路径/内容 |
| 依赖解析投毒（crates.io） | Tampering | `--locked` 固定 Cargo.lock；`Swatinem/rust-cache` 缓存校验 |

## Sources

### Primary (HIGH confidence)
- [VERIFIED: winit 0.30.13 vendored source] `~/.cargo/registry/src/index.crates.io-*/winit-0.30.13/src/platform_impl/windows/event_loop.rs:198-199` + `dpi.rs:20-40` — D-06 自动 DPI awareness 证实
- [VERIFIED: project source] `capture.rs:29-42`（唯一换算点）、`position.rs::compute_geometry`、`fonts.rs:16-50`（macOS 分支 + Windows no-op）、`palette_checks.rs`（12 探针逐一分类）、`capture_checks.rs:87/141/163/203`（fake capture / 纯状态机 / 真实剪贴板）、`app.rs:145/466-476/519-521`、`command.rs:245`、`ui.rs:175-227`、`palette_checks.rs:2048-2066`、`03-REVIEW.md`（WR/IN 详述）、`04-CONTEXT.md`（D-01..09）
- [VERIFIED: nexte.st/docs/installation/pre-built-binaries] — taiki-e/install-action@nextest 为官方指定 CI 安装方式；Windows 二进制签名
- [VERIFIED: dtolnay/rust-toolchain README] — 标准 setup action 用法
- [VERIFIED: Swatinem/rust-cache README] — 缓存机制、key 构成、清理规则
- [CITED: github.com/actions/runner-images#2935] — Windows runner 默认显示分辨率 1024x768
- [CITED: dtolnay/rust-toolchain README] — actions-rs/toolchain 弃用

### Secondary (MEDIUM confidence)
- [CITED: rustprojectprimer.com/ci/github] — SHA pin 最佳实践、matrix 模式、--locked
- [CITED: gitlab-runner work_items/37955] — Windows runner session 限制参考（与 GitHub Actions 会话差异对比）
- [CITED: github.com/actions/runner-images#2935 + community discussion #49112] — 分辨率/缩放限制多源印证

### Tertiary (LOW confidence)
- [ASSUMED: A1] GitHub Actions windows-latest 交互式桌面会话（社区实践共识：Playwright/GUI 测试可跑于 Windows runner；runner-images issue 佐证有显示器）
- [ASSUMED: A4] Microsoft YaHei 字体路径 `C:\Windows\Fonts\msyh.ttc`（微软标准字体，未在本次会话验证）

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — CI action 全部经官方文档核实；无新 Rust 依赖
- Architecture: HIGH — 16 探针逐一源码核验分类；CI workflow 结构为标准模式
- Pitfalls: MEDIUM — A1（runner 桌面会话）与 A5（IME 行为）为推断，首次 CI 运行后确认

**Research date:** 2026-08-17
**Valid until:** 2026-09-17（CI action 版本演进较快，建议按需复查 dtolnay/rust-toolchain 与 install-action 版本）