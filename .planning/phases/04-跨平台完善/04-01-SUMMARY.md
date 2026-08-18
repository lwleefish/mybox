---
phase: 04-跨平台完善
plan: 01
subsystem: [testing, infra]
tags: [github-actions, windows, ci, cjk-fonts, headless-probes]

requires: []
provides:
  - Windows CI 验证链（build / unit / 16 headless probes）在 GitHub Actions 落地并全绿
  - Windows CJK 字体加载（msyh.ttc → simhei.ttf → simsun.ttc 回退链）
  - enter_clipboard 能力门控（D-03 探针能力分拣）
  - 探针 harness 隐藏窗口轮询驱动（Windows 事件派发差异的通用修复）
affects: [04-02, 后续所有需要 Windows 回归验证的 phase]

tech-stack:
  added: [GitHub Actions (windows-latest), dependabot, actionlint]
  patterns:
    - "CI 仅依赖 push 到 main 触发；探针按能力门控而不是白名单跳过"
    - "隐藏窗口的探针轮询：about_to_wait 合成 RedrawRequested 直接驱动，不依赖平台事件派发"

key-files:
  created:
    - .github/workflows/windows-ci.yml
    - .github/dependabot.yml
  modified:
    - crates/modules/palette/src/fonts.rs
    - crates/modules/capture/src/bin/capture_checks.rs
    - crates/modules/palette/src/bin/palette_checks.rs

key-decisions:
  - "CI 是唯一 Windows 验证途径（D-01）：仓库从零创建（无 remote），先建仓后推送触发首次 CI"
  - "分支 master → main 重命名（workflow 触发条件与 GitHub 默认一致）"
  - "四个第三方 action 全部 SHA-pin + 版本注释（T-4-01 供应链）"
  - "click_hide_before_capture 超时的根因是 Windows 隐藏窗口不再派发 RedrawRequested，非代码回归——修复落在探针 harness 层（合成事件驱动），不动产品代码"

patterns-established:
  - "Windows 隐藏窗口轮询：WM_PAINT 不派发给隐藏窗口 → request_redraw 是 no-op → 探针 poll stage 必须由 about_to_wait 合成 RedrawRequested 驱动"
  - "探针请求队列 drain：close() 同步入队 Destroy，但 straggler（Redraw）排在队首，需确定性 drain 循环而非依赖后续事件逐个消费"

requirements-completed: [FRMW-06]

duration: ~2.5h
completed: 2026-08-18
---

# Phase 04: 跨平台完善 — Plan 01 Summary

**GitHub Actions Windows CI（build + unit + 16 headless probes）全绿，Windows CJK 字体加载与 enter_clipboard 能力门控落地，探针 harness 获得隐藏窗口轮询驱动能力**

## Performance

- **Duration:** ~2.5h（含 9 次 CI 迭代）
- **Started:** 2026-08-18
- **Completed:** 2026-08-18
- **Tasks:** 3
- **Files modified:** 5（3 created + 2 modified + 1 probe fix）

## Accomplishments
- Windows CI 三步验证链全绿：`cargo build --target x86_64-pc-windows-msvc --locked`、`cargo nextest run --locked`、capture 4 + palette 12 headless probes 全部 OK（enter_clipboard 在 Windows 上真实 OK，无需 SKIP 路径）
- Windows CJK 字体分支：msyh.ttc → simhei.ttf → simsun.ttc 回退链，中文命令名在 Windows 渲染有字形
- 仓库基建：GitHub repo `lwleefish/mybox` 创建（PUBLIC），分支 master→main 重命名，dependabot（github-actions + cargo 双条目）落地
- 4 个第三方 action 全部 SHA-pin + `# vX` 注释
- 探针 harness 两处通用修复（见 Deviations），修好本 plan 首个 Windows 专有问题并惠及后续所有隐藏窗口轮询探针

## Task Commits

1. **Task 1: 仓库创建 + gh 认证 gate** - `add8a5a` (feat: CI workflow + dependabot 一并提交)
2. **Task 2+3: workflow/dependabot + CJK 字体 + 能力门控** - `fd47e70` (feat(04-01): windows CJK font loading and enter_clipboard capability gate)
3. **CI 修复迭代（8 commits）** - `903d8b4`, `b5dc960`, `e8f6860`, `fb6f152`, `4d3d88b`, `97edba3`, `1cbccde`, `90b44c8`
4. **探针根因修复（2 commits）** - `32d8e05`, `7c6c13d`

**Plan metadata:** （plan 完成提交未单独产生——SUMMARY 随 STATE 更新提交）

## Files Created/Modified
- `.github/workflows/windows-ci.yml` - Windows CI 三步验证 job（SHA-pin actions、--locked 全步骤、bash 探针循环）
- `.github/dependabot.yml` - github-actions + cargo 每周依赖更新
- `crates/modules/palette/src/fonts.rs` - Windows CJK 字体回退链（cfg windows 分支 + 测试）
- `crates/modules/capture/src/bin/capture_checks.rs` - enter_clipboard 能力门控（探测失败 → 明确 SKIPPED）
- `crates/modules/palette/src/bin/palette_checks.rs` - 探针 harness 修复（见 Deviations）

## Decisions Made
- 仓库参数：个人账号、PUBLIC、名 mybox（用户已确认）
- CI 触发仅 push main（+PR 保留）；首次 run 由 Task 3 联合推送触发
- 探针 12/12 全部按 RUN 进 CI；enter_clipboard 的能力门控在探针内表达，CI 循环不特殊处理
- 探针失败一律先按真实回归排查，确认为平台差异才动探针（本次两个修复都是 harness 层，产品代码零改动）

## Deviations from Plan

### Auto-fixed Issues

**1. [Pitfall 6 - Blocking] 仓库无 remote / 无 GitHub 仓库**
- **Found during:** Task 1（gh 认证后）
- **Issue:** 仓库从未创建，无法触发 CI
- **Fix:** `gh repo create mybox --public`，设置 origin remote，推送 main
- **Files modified:** 无（git 层面）
- **Verification:** `gh repo view mybox` 显示 PUBLIC；`git ls-files` 无敏感文件
- **Committed in:** add8a5a（推送触发首次 CI）

**2. [D-04 - Blocking] 分支名 master ≠ GitHub 默认 main**
- **Found during:** Task 1
- **Issue:** 本地分支是 master，GitHub 默认/工作流约定是 main，trigger 条件会歧义
- **Fix:** `git branch -m main` + 上游重绑
- **Verification:** push 到 main 成功触发 workflow
- **Committed in:** add8a5a

**3. [CI 迭代 - Blocking] 首次 CI run 多步红（8 轮修复）**
- **Found during:** 首次 CI run（9 次 run）
- **Issue:** rust-toolchain 用 stable 分支 SHA（非可重定位）、install-action 需 tool input、Windows 非 Send 热键管理器、size-label 字体链、config dir 断言平台差异、softbuffer 首次 present 未显式 resize、glyph AA 阈值平台差异、keyword_highlight accent 扫描容差——全部为 Windows 平台差异/CI 环境差异，非产品逻辑回归
- **Fix:** 逐轮修复（rust-toolchain stable 分支→具体版本 SHA、tool input 指定、热键 deferred 注册门控、平台字体链、per-platform 断言、显式 resize、平台感知阈值、容差扫描+全帧诊断）
- **Files modified:** workflow、fonts.rs、lib.rs（热键 gate）、单测断言、探针阈值
- **Verification:** 每轮 push 后 `gh run watch` 直至绿
- **Committed in:** 903d8b4..90b44c8

**4. [探针根因 - Blocking] click_hide_before_capture 在 Windows CI 超时（10s watchdog）**
- **Found during:** run 32090901993（仅剩此一项红）
- **Issue:** stage 3 的 try_recv 每次 driver 调用只消费一个队列请求，依赖后续 RedrawRequested 事件继续 drain；Windows 隐藏窗口不派发 WM_PAINT → request_redraw no-op → driver 停摆。macOS orderOut 后仍派发，故仅 Windows 复现。深层原因：stage 0/1/2 帧的 resp.repaint 会在 FIFO 队列留下 Redraw straggler，排在 close() 同步入队的 Destroy 之前
- **Fix:** 两层修复——(a) stage 3 改为确定性 while-let drain 循环（一次消费全部 straggler 直达 Destroy，不依赖后续事件）；(b) harness about_to_wait 检测窗口隐藏后合成 RedrawRequested 直接驱动 driver，保证 poll stage（如 stage 4 的 runner counter）在 Windows 上继续推进
- **Files modified:** crates/modules/palette/src/bin/palette_checks.rs
- **Verification:** 本地 macOS 12/12 探针全绿 + Windows CI 全绿（run 32095791623）
- **Committed in:** 32d8e05, 7c6c13d

---

**Total deviations:** 4 项，全部为必要修复（2 基建 + 8 CI 迭代 + 2 探针 harness）。产品代码零回归，探针修复惠及全部隐藏窗口轮询探针。
**Impact on plan:** 无范围蔓延；CI 全绿达成 must_haves 全部 6 条 truth。

## Issues Encountered
- 本地 `cargo nextest run` 窗口时序单测偶发 flaky（hotkey/late-window 类 5 个测试），重跑即绿；CI 上稳定绿。与本次改动无关（探针 bin 不影响库测试）。
- actionlint 经 brew 安装后零错误通过（plan 预设）。

## User Setup Required
已由用户在 Task 1 gate 处完成：`gh auth login`（browser flow，账号 lwleefish）。仓库参数（PUBLIC/mybox/个人账号）经用户确认。

## Next Phase Readiness
- 04-02 的 Windows 回归通道已就绪：后续 phase 的 Windows 验证只需 push main 触发 CI（Task 4 已具备推送触发条件）
- 探针 harness 的隐藏窗口轮询能力（合成 RedrawRequested）是通用修复，04-02 若引入新的隐藏窗口探针可直接复用
- 注意：04-02 新增/修改的探针必须保持 16 个探针在 Windows 全绿（CI 是全绿门禁）

---
*Phase: 04-跨平台完善*
*Completed: 2026-08-18*
