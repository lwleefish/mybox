---
phase: 03-命令面板
plan: 04
subsystem: core
tags: [rust, egui, tiny-skia, glyph-rendering, texture-atlas, gap-closure]
requires:
  - phase: 03-命令面板
    provides: 03-03 GAP-1 热键/建销修复（本计划在其成果之上叠加：palette_checks realize_window 的 on_created 配对、session.on_window_created 均未回退）
provides:
  - raster::paint 按 UV（WHITE_UV 契约）分派 solid/textured 三角形——字形进入纹理采样路径（GAP-2 主因消除）
  - paint_textured_triangle 按 GL 纹素中心语义采样（uv*size − 0.5，消除半纹素边缘模糊）
  - session.apply_textures 原位 partial 图集补丁（ImageDelta.pos 分支 + 越界/变体不匹配防御）
  - 帧间帧缓冲清屏（#202020 不透明全幅契约——几何变化后无陈旧像素）
  - glyph_shape E2E 探针（真实窗口 + Ime 注入强制增量字形 + aa_spread 字形结构断言）
affects: [03-命令面板, verify-phase HUMAN-UAT 测试 5 重跑]
tech-stack:
  added: []
  patterns:
    - "epaint 契约式分派：solid mesh = uv == WHITE_UV(0,0) + TextureId::default()；颜色相等不可作为无纹理判别（字形三角形颜色统一）"
    - "GL 纹素中心采样对齐：sample_bilinear 以整数为纹素中心，UV→纹素坐标换算需 −0.5 才与 GL_LINEAR/egui shader 语义一致"
    - "partial 图集补丁原位行拷贝（Font f32 步长 1 / Color RGBA8 步长 4），越界 warn+clip/skip，变体不匹配 warn+整图退化"
key-files:
  created: []
  modified:
    - crates/modules/palette/src/raster.rs
    - crates/modules/palette/src/session.rs
    - crates/modules/palette/src/lib.rs
    - crates/modules/palette/src/bin/palette_checks.rs
    - crates/modules/palette/tests/integration.rs
    - .planning/REQUIREMENTS.md

key-decisions:
  - "纹理分派改 UV 判别（WHITE_UV 契约）：字形三角形三顶点同色（文本色），颜色相等判别永远走 solid 路径——epaint 保证 solid mesh 的 uv 恒为 WHITE_UV + 默认 TextureId（lib.rs:84-88）"
  - "paint_textured_triangle 采样坐标加 −0.5 对齐 GL/egui 纹素中心语义：修正前半纹素偏移使 1 纹素宽字形笔画被模糊至 204/255（egui 自身渲染器在纹素中心精确采样）"
  - "单元测试字形断言阈值校准：14px Hiragino CJK 笔画 ~1 纹素宽 + 亚像素摆放，双线性采样几乎不产生 alpha==255——'近满覆盖'改为 alpha≥240、(0.005, 0.7)；纯色块 ≈0.85 仍必红"
  - "E2E 探针断言改 aa_spread：合成后帧缓冲完全不透明（文字叠加在不透明 #202020 卡片上），alpha 指标无法区分字形与色块——改为输入区中灰阶 AA 像素计数（实测真实字形 242 vs 旧色块 bug 40，阈值 120，6x 分离）"
  - "帧循环在 raster::paint 前清屏为 #202020：几何变化（Filtering→Empty 收窄窗口）后屏幕矩形外的陈旧字形行不再残留（不透明全幅窗口契约）"

requirements-completed: [PAL-02]

duration: 26min
completed: 2026-08-15
---

# Phase 3 Plan 04: GAP-2 字形渲染修复 Summary

**UV（WHITE_UV）契约分派 + 半纹素采样对齐 + partial 图集补丁原位写入，面板文字从纯色块变为可识别字形；glyph_shape 探针桌面 6/6 全绿（aa_spread 242 vs 旧 bug 40）**

## Performance

- **Duration:** 26 min
- **Started:** 2026-08-15T03:51:30Z
- **Completed:** 2026-08-15T04:17:17Z
- **Tasks:** 3
- **Files modified:** 6
- **Commits:** 4

## Accomplishments

- **GAP-2 主因消除**：`raster::paint` 以 `mesh.texture_id != default || 任一顶点 uv != WHITE_UV` 判别 textured 路径——字形三角形进入纹理采样，字体图集真正被采样。新增 RED 测试 `textured_dispatch_uses_uv_not_color`（三顶点同色白 + 非 WHITE_UV + 棋盘纹理 → ≥2 种输出值；修复前走 solid 输出单一白）。
- **GAP-2 次因消除**：`session.apply_textures` 按 `ImageDelta.pos` 分支——`None` 整图替换、`Some([x,y])` 原位行拷贝补丁（Font f32 步长 1 / Color RGBA8 步长 4），越界 warn+clip/skip、变体不匹配 warn+整图退化（T-03-05 防御）。epaint `TextureAtlas::take_delta` 增量字形光栅化时发的 partial 补丁不再破坏已光栅化字形。
- **采样对齐修复（执行期发现的隐藏缺陷）**：`paint_textured_triangle` 采样坐标 −0.5，对齐 GL/egui 纹素中心语义——修复前 1 纹素宽字形笔画被半纹素边缘模糊（实测 max alpha 204/255）；修复后与 egui 自身渲染器等价。
- **帧缓冲清屏**：帧循环在 `raster::paint` 前清为 #202020——几何变化后旧布局的陈旧字形行不再残留（不透明全幅窗口契约）。
- **E2E 回归探针**：`check_glyph_shape` 真实窗口驱动 3 帧 + 帧间 `Ime::Commit`（"测试"/"zz"）注入强制新字形增量光栅化（partial delta 路径被真实行使：帧间 diff=52830），帧 3 断言字形结构（bbox ≥8x8、文本区 ≥16 种 RGBA、输入区 AA 像素 aa_spread ≥120）。桌面会话 6/6 `--ignored` 全绿。
- **文档同步**：REQUIREMENTS.md PAL-02 → Complete（checkbox、traceability、时间戳）。

## Task Commits

Each task was committed atomically:

1. **Task 1: raster 三角形路径判别改 UV** - `a80f30d` (fix) — UV 分派 + 半纹素采样对齐 + RED 测试 + 强化字形结构测试
2. **Task 2: apply_textures partial 补丁** - `98c2ec3` (fix) — 原位补丁 + 防御分支 + 3 个单测
3. **Task 3: glyph_shape 探针 + 接线 + PAL-02 同步** - `45f0b2e` (fix, 帧缓冲清屏) + `23fce42` (test, 探针 + integration + REQUIREMENTS)

**Plan metadata:** committed with this SUMMARY (docs)

## Files Created/Modified

- `crates/modules/palette/src/raster.rs` - `paint` UV（WHITE_UV）分派替换颜色相等判别；`paint_textured_triangle` 采样 −0.5（GL 纹素中心对齐）；新 RED 测试 `textured_dispatch_uses_uv_not_color`；强化 `paint_renders_chinese_label`（bbox ≥4x4 + 近满覆盖 (0.005,0.7) + ≥16 种值）
- `crates/modules/palette/src/session.rs` - `apply_textures` partial 原位补丁分支；`patch_texture_image` 越界/变体防御；3 个新单测（in-place patch / full replace / OOB clip-skip）
- `crates/modules/palette/src/lib.rs` - 帧循环 raster::paint 前清屏 #202020（几何变化后无陈旧像素）
- `crates/modules/palette/src/bin/palette_checks.rs` - `glyph_structure` 统计助手 + `check_glyph_shape` 探针（3 帧 + Ime 注入 + diff/bbox/kinds/aa_spread 断言）；main 分发与 usage 更新
- `crates/modules/palette/tests/integration.rs` - `palette_glyph_shape`（`#[ignore]`，PAL-02/GAP-2 回归）
- `.planning/REQUIREMENTS.md` - PAL-02 → Complete

## Verification Evidence

- `cargo nextest run -p mybox-palette raster` → 5/5 PASS（含 2 个新/强化测试）
- `cargo nextest run -p mybox-palette session` → 18/18 PASS（含 3 个新纹理补丁测试）
- `cargo nextest run -p mybox-palette` → 54/54 PASS
- `cargo nextest run --workspace` → **215/215 PASS**（03-03 时为 211，+4 新测试，无回归）
- `cargo check --workspace` → exit 0，无 warning
- 桌面会话 `cargo test -p mybox-palette --test integration -- --ignored` → **6/6 PASS**，其中 `glyph_shape`：帧间 diff=52830（Ime 提交真实改变渲染文本，partial 图集补丁路径被行使）、bbox=1200x288、kinds=53、aa_spread=242

## Decisions Made

- UV 分派遵循 epaint 契约而非颜色启发式——这是 GAP-2 主因的唯一正确判据（字形三角形颜色统一，颜色相等≠无纹理）。
- 采样 −0.5 是对"匹配 egui shader 语义"（paint_textured_triangle 既有文档契约）的修复而非新功能——强化测试暴露 max alpha 204 后才定位到半纹素偏移。
- E2E 探针放弃 alpha 指标改 aa_spread：合成后帧缓冲全不透明，alpha==255 占比在字形与色块两种情况下都 ≈1.0（计划前提不成立），中灰阶 AA 计数实测 6x 分离（242 vs 40）。
- 阈值校准记录：单元测试"近满覆盖"alpha ≥240 ∈ (0.005, 0.7)；探针 aa_spread ≥120——校准依据（含旧 bug 基线测量，通过临时环境变量切换分派后运行探针实测）详见 Deviations。

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] paint_textured_triangle 半纹素采样偏移**
- **Found during:** Task 1（强化 `paint_renders_chinese_label` 后 max alpha 仅 204/255）
- **Issue:** 采样器以整数坐标为纹素中心，而 GL/egui 的纹素中心在 i+0.5——UV×尺寸 直接采样落在纹素边缘，1 纹素宽字形笔画被模糊一半；违背函数既有"匹配 egui shader 语义"的文档契约。
- **Fix:** UV→纹素坐标换算 −0.5（`uv*size − 0.5`），与 GL_LINEAR 语义严格对齐（含边缘 clamp 行为一致）。
- **Files modified:** crates/modules/palette/src/raster.rs
- **Verification:** 修复后 max alpha 250→255 达标；`solid_fast_path_matches_barycentric` 等既有测试不受影响（1x1 纹理采样退化）。
- **Committed in:** a80f30d

**2. [Rule 1 - Bug] 帧缓冲帧间不清屏**
- **Found during:** Task 3（glyph_shape 探针首测 bbox 覆盖整窗——几何变化后残留帧 1 命令行字形像素）
- **Issue:** 帧循环直接叠加 paint，卡片只覆盖当前 egui screen rect——窗口收窄（Filtering→Empty）后屏幕矩形外的行保留上一布局的字形（测量污染 + 潜在视觉残留）。
- **Fix:** `raster::paint` 前 `framebuffer.fill(#202020 不透明)`（不透明全幅窗口契约，03-PATTERNS）。
- **Files modified:** crates/modules/palette/src/lib.rs
- **Verification:** 探针测量区恢复干净；桌面 6/6 全绿。
- **Committed in:** 45f0b2e

**3. [阈值校准] 单元测试 alpha==255 覆盖率前提不成立**
- **Found during:** Task 1（强化测试首测 coverage=0.000——无任何 alpha==255 像素）
- **Issue:** 14px Hiragino CJK 笔画 ~1 纹素宽 + 亚像素摆放，双线性采样几乎不产生精确 255（实测 max ≈250，egui 自身 GL 渲染同此性质）；计划的 (0.02, 0.7) 下界不可达。
- **Fix:** "近满覆盖"改 alpha ≥240，区间 (0.005, 0.7)（实测字形 ≈0.013，纯色块 ≈0.85 必红）。
- **Committed in:** a80f30d（测试内注释记录校准理由）

**4. [阈值校准] E2E 探针 alpha 指标改为 aa_spread**
- **Found during:** Task 3（首测 near_full=0.992——bbox 内几乎全为 alpha≥240）
- **Issue:** 合成后帧缓冲完全不透明（文字 over-blend 在不透明 #202020 卡片上，out_a 恒 255），alpha==255 占比/存在性指标在字形与纯色块间无分离；纯色块基线 kinds=34 亦无法用 ≥16 阈值区分（tiny-skia solid 路径 AA 边缘产生数十灰阶）。
- **Fix:** 探针改为输入区（白字 #2E2E2E 输入框，scale-aware 边界）中灰阶 (60..245) AA 像素计数 `aa_spread`：实测真实字形 242 vs 旧色块 bug 40（临时环境变量切换分派实测基线），阈值 120（6x/2x 双侧余量）；保留 bbox ≥8x8 与 kinds ≥16 作健全性断言。
- **Files modified:** crates/modules/palette/src/bin/palette_checks.rs
- **Verification:** 修复代码 242 ≥120 PASS；旧分派基线 40 <120 必红（已实测）。
- **Committed in:** 23fce42

---

**Total deviations:** 4（2 个 Rule 1 bug 修复 + 2 个阈值校准）
**Impact on plan:** 2 个 bug 修复均为字形渲染正确性所必需（采样对齐 + 帧间清屏），2 个阈值校准使断言与合成后帧缓冲的现实一致、判别力反而更强（6x 分离）。无范围蔓延。

## Issues Encountered

- 计划假设"真实字形产生 alpha==255 像素"在 14px 亚像素 CJK + 不透明合成下均不成立——通过实测（含临时旧分派基线）完成阈值校准，而非削弱断言目标。

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- GAP-2（BLOCKER）两个根因均已消除并有单测（UV 分派 RED + 补丁原位）+ E2E（glyph_shape，Ime 注入行使 partial 路径）双层回归锁定。
- 03-03/03-04 两个 gap closure 计划均完成：GAP-1（热键双报/建销配对）、GAP-2（字形渲染）修复落地，PAL-01/PAL-02 均标 Complete。
- 真实桌面的最终人工复验（HUMAN-UAT 测试 5 重跑：面板内中文命令名、描述、占位符均为可识别字形）留给 verify-phase。

---

*Phase: 03-命令面板*
*Completed: 2026-08-15*

## Self-Check: PASSED

- All key-files exist on disk (raster.rs, session.rs, lib.rs, palette_checks.rs, integration.rs, REQUIREMENTS.md)
- All 4 plan commits present in git history (a80f30d, 98c2ec3, 45f0b2e, 23fce42)
- Plan-level verification re-run: `cargo nextest run --workspace` 215/215 PASS; `cargo check --workspace` clean; desktop `--ignored` integration 6/6 PASS (含 glyph_shape aa_spread=242)
