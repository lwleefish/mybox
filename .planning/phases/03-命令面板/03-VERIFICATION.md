---
phase: 03-命令面板
verified: 2026-08-15T05:41:03Z
status: human_needed
score: 11/11 must-haves verified
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: 17/17
  gaps_closed:
    - "GAP-1 热键重复唤出失败（第二次唤出闪退）——HotKeyState::Pressed 守卫 + WindowSpec.on_created 建销配对归属"
    - "GAP-2 文字渲染为灰色块——UV 判别纹理分派 + partial 图集补丁原位写入"
  gaps_remaining: []
  regressions: []
deferred:
  - truth: "Windows 上热键 Send 化（HotkeyManager 延迟注册模式）、Windows 字体发现、explorer 打开文件行为"
    addressed_in: "Phase 4"
    evidence: "Phase 4 成功标准 3：'命令面板在 Windows 上可唤出并执行命令'；goal '在 Windows 上完成适配'"
  - truth: "sync_window_geometry 负坐标 clamp（左侧/上方副屏重居中跳主屏）——上轮 REVIEW WR-01"
    addressed_in: "Phase 4"
    evidence: "Phase 4 goal 明确包含 '多显示器'"
  - truth: "runner panic 无 catch_unwind 保护（面板卡死 Executing）——上轮 REVIEW WR-03"
    addressed_in: "Phase 4"
    evidence: "Phase 4 goal 明确包含 '错误处理打磨' + plan 04-02 'DPI 缩放修复 + 错误处理打磨'"
  - truth: "窗口创建失败会永久卡住 pending_close（本轮 REVIEW WR-03，GAP-1 新配对设计的健壮性缺口）"
    addressed_in: "Phase 4"
    evidence: "Phase 4 goal '错误处理打磨' + plan 04-02；缺陷仅经创建失败路径可达（softbuffer surface 失败），生产正常路径不受影响"
human_verification:
  - test: "UAT 测试 1 重跑（GAP-1 关闭确认）：cargo run -p mybox-app 后按 Cmd+Shift+Space 唤出→再按热键关闭→再唤出，重复 ≥3 轮；随后执行「开始截图」后再唤出面板"
    expected: "每次唤出面板均保持显示（不闪退），截图 overlay 未被误销毁"
    why_human: "探针经 bus 级 summon 直驱，不经过 OS 物理热键注册→回调链路（探针覆盖声明已明示）；物理按键行为只能真人确认"
  - test: "UAT 测试 5 重跑（GAP-2 关闭确认）：面板内中文命令名（开始截图）、描述、占位符「输入命令…」均为可识别字形"
    expected: "无灰色方块/豆腐块；CJK 字形清晰"
    why_human: "glyph_shape 探针已断言真实帧缓冲字形结构（aa_spread 242 vs 旧 bug 40），但最终视觉验收需人眼"
  - test: "UAT 测试 2：按 manual_checklist.md 第 3/4/6 步走查——「截图」/「jt」过滤命中高亮 #FF6000、↑/↓ 环绕导航、Enter 执行、ESC 关闭不执行"
    expected: "与手册一致"
    why_human: "视觉高亮颜色、键盘手感、输入法交互属用户体验层面"
  - test: "UAT 测试 3：执行「开始截图」——面板先消失、截图选区出现，确认截图中不含面板"
    expected: "面板绝不出现在截图里（SPEC 硬约束）"
    why_human: "真实时序依赖窗口服务器销毁节奏；E2E 只断言入队序，真实截图内容需真人确认"
  - test: "UAT 测试 4：执行「退出应用/重启应用/打开配置目录/打开日志文件」四个内置命令"
    expected: "各自 OS 副作用正确发生"
    why_human: "真实 OS 副作用（进程生命周期、文件管理器）无法安全地在验证进程内执行"
---

# Phase 3: 命令面板 Verification Report（Re-verification — gap closure 03-03/03-04）

**Phase Goal:** 实现命令面板作为所有模块的统一交互入口。全局快捷键唤出，展示已注册命令，模糊搜索，键盘导航执行。
**Mode:** mvp
**Verified:** 2026-08-15T05:41:03Z
**Status:** human_needed
**Re-verification:** Yes — after gap closure (previous: gaps_found, 17/17 code truths but 2 BLOCKER human-UAT failures)

> **MVP 模式格式守卫（Escalation 项，上轮已提出、本轮仍未解决）：** `gsd-sdk query user-story.validate` 对 ROADMAP Phase 3 goal 返回 `false`（本轮 verifier 重新执行确认）。两张主 PLAN 的 Phase Goal 均为规范用户故事（03-01：唤出面板并看到全部命令；03-02：输入过滤/键盘选择执行），本报告据此构建用户流程覆盖。**建议用户运行 `/gsd mvp-phase 03` 将 ROADMAP goal 重写为用户故事格式**——不阻塞本验证（5 条成功标准无歧义、gap 计划 truth 集明确定义），但 UAT 脚本生成质量依赖它。

## 验证结论（先行）

**上轮两个 BLOCKER gap 均已关闭，代码层 + 探针层证据闭环：**

- **GAP-1（热键重复唤出闪退）关闭** — 两处根因均修复：`App::on_hotkey` Pressed 守卫过滤 Released 双报；建销配对从广播 `core/window-created` 改为每窗口 `WindowSpec.on_created` 主线程同步回调。新 E2E 探针 `consecutive_summon_close`（真实窗口 3 轮建销 + 最终唤出观察 ≥3 帧无 Destroy）由本 verifier 在桌面会话实跑通过。
- **GAP-2（文字灰色块）关闭** — 两处根因均修复：`raster::paint` 按 epaint WHITE_UV 契约分派 textured 路径（字形三角形进入字体图集采样）；`session.apply_textures` 对 `ImageDelta::partial` 原位行拷贝补丁。新 E2E 探针 `glyph_shape`（3 帧 + Ime 注入强制增量字形）由本 verifier 实跑通过：**aa_spread=242 vs 旧 bug 基线 40（6x 分离）**，帧间 diff=52830 证明 partial 图集补丁路径被真实行使。

## User Flow Coverage (MVP Mode)

User story（合并 03-01 + 03-02 的 plan-level 用户故事）：
«As a mybox 用户, I want to 按全局快捷键唤出屏幕中央的命令面板浮窗、看到全部已注册命令、输入关键词即时过滤、用方向键与回车选择并执行命令, so that 所有工具通过一个统一入口触手可及。»

| Step | Expected | Evidence | Status |
|------|----------|----------|--------|
| 按 Cmd+Shift+Space | 活动显示器中央出现无边框置顶浮窗 | app.rs:325-327 Pressed 守卫（每次按压仅一次 toggle）；lib.rs:93-104 hotkey.triggered → toggle_palette；build_window_spec on_created 配对；E2E summon_render + consecutive_summon_close 实跑通过 | ✓（物理按键路径留待 UAT 1 重跑） |
| 再按热键/ESC 关闭后再唤出 | 面板保持显示，不闪退 | Pressed 守卫 + on_created 配对 + consecutive_summon_close 探针（3 轮循环每轮观察 ≥2 帧无 Destroy + 无 pending_close 残留）本 verifier 实跑通过 | ✓ |
| 看到命令列表（文字可读） | 中文命令名/描述/占位符为可识别字形，非灰色块 | raster.rs:106-130 UV 判别；session.rs:323-346 原位补丁；glyph_shape 探针 aa_spread=242 实跑通过 | ✓ |
| 输入「截图」/「jt」 | 「开始截图」命中且排首位，命中字符 #FF6000 高亮 | filter.rs 三层梯队（上轮已验证，本轮无改动）；E2E fuzzy_navigation_execute 实跑通过 | ✓ |
| ↑/↓/Enter | 环绕导航；回车执行；执行中面板保持 +「正在执行：…」 | session.rs 状态机（上轮已验证）；E2E fuzzy_navigation_execute + capture_hides_first 实跑通过 | ✓ |
| ESC | 面板关闭且不执行任何命令 | E2E five_summon_esc_no_residue 实跑通过 | ✓ |
| Outcome | 统一入口可用——快捷键唤出、键盘全程操作、文字清晰可读 | 6/6 E2E 探针 + 215 单测 + cargo check 全绿（本 verifier 实跑） | ✓ |

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| G1-T1 | 每一次物理热键按压只产生一次 toggle（释放事件不再触发关闭——GAP-1 根因消除） | ✓ VERIFIED | app.rs:314-327 `on_hotkey` 入口 `if e.state != HotKeyState::Pressed { return; }`；回归单测 `on_hotkey_released_event_is_ignored`（Released 注入断言零 bus 事件，本 verifier 实跑通过）。物理按键链路留待 UAT 1 |
| G1-T2 | 连续多次唤出/关闭循环后再次唤出，面板保持显示不闪退（无 pending_close 残留误销毁） | ✓ VERIFIED | E2E `consecutive_summon_close` 本 verifier 实跑 PASS：3 轮（summon→on_created 配对→观察 ≥2 帧无 Destroy 且 window_id/state 正确→ESC 配对 Destroy 无残留）+ 最终唤出观察 ≥3 帧 |
| G1-T3 | 面板建销配对只作用于面板自己的窗口——capture overlay 的 window-created 不触碰 palette session | ✓ VERIFIED | lib.rs:106-111 init 无 `core/window-created` 订阅（源码断言确认）；session.rs:136-144 `on_window_created` 仅经 spec.on_created 调用；capture/src/lib.rs:225 仍订阅广播、app.rs:422-426 仍发广播（capture 链路未破坏） |
| G2-T1 | 面板内所有文字渲染为可识别的字形，而非纯色块（GAP-2 症状消除） | ✓ VERIFIED | raster.rs:106-130 UV 判别（WHITE_UV 契约）替换颜色相等判别；RED 测试 `textured_dispatch_uses_uv_not_color`（三顶点同色白 + 棋盘纹理断言 ≥2 种输出值）；glyph_shape 探针实测 bbox=1200x288、kinds=53、aa_spread=242（旧 bug 基线 40） |
| G2-T2 | 多帧渲染（增量字形进入字体图集后）文字仍保持正确——partial 补丁不再破坏已光栅化字形 | ✓ VERIFIED | session.rs:323-346 `apply_textures` 按 `change.pos` 分支；patch_texture_image（session.rs:468-539）原位行拷贝 + 越界 warn+clip + 变体不匹配退化；3 个单测；glyph_shape 帧间 diff=52830 证明 partial delta 路径真实行使且输出正确 |
| G2-T3 | 命令列表可辨识地列出全部已注册命令（PAL-02 的视觉呈现成立） | ✓ VERIFIED | glyph_shape 探针用 CJK 命令名（开始截图/退出应用）驱动，bbox 1200x288 覆盖完整列表区、53 种 RGBA 值证明多字形渲染 |
| SC-1 | 用户按全局快捷键唤出命令面板浮窗（PAL-01） | ✓ VERIFIED（回归） | 上轮链路无改动；E2E summon_render 实跑通过；G1-T1/T2 加固 |
| SC-2 | 面板列出截图模块注册的命令（PAL-02） | ✓ VERIFIED（回归） | 上轮链路无改动；G2-T3 视觉呈现加固 |
| SC-3 | 输入关键词可模糊过滤命令列表（PAL-03） | ✓ VERIFIED（回归） | E2E fuzzy_navigation_execute 实跑通过 |
| SC-4 | 方向键选择命令，回车执行对应功能（PAL-04） | ✓ VERIFIED（回归） | E2E fuzzy_navigation_execute + capture_hides_first 实跑通过 |
| SC-5 | 按 ESC 关闭命令面板（PAL-05） | ✓ VERIFIED（回归） | E2E five_summon_esc_no_residue + consecutive_summon_close 实跑通过 |

**Score:** 11/11 truths verified（6 gap-closure truths + 5 roadmap SCs；上轮 17 条 plan 级 truth 的回归证据见探针与单测实跑）

### Deferred Items

| # | Item | Addressed In | Evidence |
|---|------|-------------|----------|
| 1 | Windows 热键 Send 化、字体发现、explorer 打开文件行为 | Phase 4 | Phase 4 SC-3 + goal（Windows 适配） |
| 2 | sync_window_geometry 负坐标 clamp（上轮 REVIEW WR-01） | Phase 4 | Phase 4 goal「多显示器」 |
| 3 | runner panic 无 catch_unwind（上轮 REVIEW WR-03） | Phase 4 | Phase 4 goal「错误处理打磨」+ plan 04-02 |
| 4 | 窗口创建失败永久卡死 pending_close（本轮 REVIEW WR-03） | Phase 4 | Phase 4 goal「错误处理打磨」+ plan 04-02；仅创建失败路径可达，不影响正常使用 |

### Required Artifacts（gap closure 增量）

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| crates/mybox-core/src/app.rs | on_hotkey Pressed 守卫 + create_window 调用 on_created | ✓ VERIFIED | 325-327 守卫 + 注释；406-421 take + register 之后/bus emit 之前同步调用；733-757 回归单测 |
| crates/mybox-core/src/window.rs | WindowSpec.on_created 字段 + Default | ✓ VERIFIED | 63 字段声明（doc 完整）；81 Default；469 default 断言 |
| crates/modules/palette/src/session.rs | on_window_created 配对入口 + apply_textures partial 补丁 | ✓ VERIFIED | 136-144 配对；323-346 补丁分派；468-539 防御完整；844-930 三个纹理测试 |
| crates/modules/palette/src/lib.rs | init 无广播订阅 + build_window_spec 配对闭包 + 帧间清屏 | ✓ VERIFIED | 106-111 无订阅（注释明示原因）；342-347 on_created: Some(...)；291 帧缓冲 fill #202020 |
| crates/modules/palette/src/raster.rs | UV 判别分派 + RED 测试 + 强化字形测试 | ✓ VERIFIED | 106-130 uses_texture；469-529 RED 测试；384+ 强化 bbox/覆盖率/值种类断言 |
| crates/modules/palette/src/bin/palette_checks.rs | consecutive_summon_close + glyph_shape 探针 + realize_window 生产配对 | ✓ VERIFIED | 724-830 + 935-1075；87-123 realize_window 走 spec.on_created（无 set_window_id 直调） |
| crates/modules/palette/tests/integration.rs | 2 个新 #[ignore] 测试 | ✓ VERIFIED | 83-97 两测试均接线（IN-04：doc 注释阈值描述陈旧，info 级） |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|----|---------|---------|
| global_hotkey Released 事件 | bus hotkey.triggered | App::on_hotkey Pressed 守卫早退 | ✓ WIRED | app.rs:325-327；单测锁定 Released 零事件 |
| App::create_window（主线程） | palette session 配对 | spec.on_created.take() → 同步回调 → session.on_window_created → pending_close 时 Destroy(id) | ✓ WIRED | app.rs:406-421 + session.rs:136-144 + lib.rs:342-347；同一 drain pass 内完成 |
| ESC/toggle 关闭 | 窗口销毁 | close() 返回 window_id 入队 Destroy；未配对置 pending_close 由 on_created 补销毁 | ✓ WIRED | session.rs:163-176；consecutive_summon_close 探针断言无残留 |
| egui tessellate ClippedPrimitive | raster::paint 路径分派 | uses_texture = texture_id != default ∥ 任一顶点 uv != WHITE_UV | ✓ WIRED | raster.rs:106-130；RED 测试 + glyph_shape 探针双层锁定 |
| full_output.textures_delta | session 纹理表 | apply_textures 按 ImageDelta.pos：None 整图 / Some 原位补丁 | ✓ WIRED | session.rs:323-346；lib.rs:275 帧循环调用点 |
| session 纹理表快照 | paint_textured_triangle 采样 | textures.get(&texture_id) + −0.5 纹素中心对齐 | ✓ WIRED | raster.rs 采样路径；glyph_shape 探针 aa_spread=242 |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|-------|--------------------|--------|
| raster::paint textured 路径 | mesh.vertices uv / texture_id | egui tessellate 字形三角形（真实 UI 渲染） | Yes — glyph_shape 探针帧缓冲实测 24222 非背景像素、53 种值 | ✓ FLOWING |
| apply_textures → textures 表 | ImageDelta set/free | epaint TextureAtlas::take_delta 真实增量光栅化 | Yes — Ime 注入产生 diff=52830（partial delta 路径被行使） | ✓ FLOWING |
| paint_textured_triangle 采样 | texture_buffers | session.textures() 快照（真实图集） | Yes — 字形笔画 aa_spread=242（旧 bug 40） | ✓ FLOWING |
| on_created 配对闭包 | created_session/created_windows Arc 克隆 | build_window_spec 捕获（生产装配） | Yes — 探针 3 轮循环 window_id 配对正确、Destroy 配对正确 | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| 全 workspace 单测（复跑 2） | cargo nextest run --workspace | 215 passed / 14 skipped | ✓ PASS |
| palette 包单测 | cargo nextest run -p mybox-palette | 54 passed / 6 skipped | ✓ PASS |
| 编译健康 | cargo check --workspace | exit 0，无 warning | ✓ PASS |
| E2E 集成测试（桌面会话） | cargo test -p mybox-palette --test integration -- --ignored | 6/6 PASS | ✓ PASS |

**观察项（非回归）：** 本轮首次 `cargo nextest run --workspace` 出现 5 个 palette lib.rs 时序测试失败（bus 工作线程在冷启动并行负载下等待超时，"first toggle must summon"）；随即单独运行 palette 包 54/54、完整 workspace 复跑 215/215 全绿。与上轮记录的"E2E 首跑抖动"同型（固定 sleep + 轮询脚手架的冷启动竞争），gap 修复本身稳定通过。若需根治建议后续将 sleep 改为事件驱动的 wait_until 条件轮询。

### Probe Execution

| Probe | Command | Result | Status |
|-------|---------|--------|--------|
| summon_render | bash palette_checks（经 integration.rs） | exit 0 | ✓ PASS |
| fuzzy_navigation_execute | 同上 | exit 0 | ✓ PASS |
| capture_hides_first | 同上 | exit 0 | ✓ PASS |
| five_summon_esc_no_residue | 同上 | exit 0 | ✓ PASS |
| consecutive_summon_close | 同上 | exit 0 | ✓ PASS（GAP-1 回归，本 verifier 实跑） |
| glyph_shape | 同上 | exit 0，实测 bbox=1200x288 non_bg=24222 diff=52830 kinds=53 aa_spread=242 | ✓ PASS（GAP-2 回归，本 verifier 实跑；测量值与 03-04 SUMMARY 声明完全一致） |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|---------|
| PAL-01 | 03-01 / 03-03 | 用户按全局快捷键唤出命令面板浮窗 | ✓ SATISFIED | REQUIREMENTS.md:39 `[x]` + traceability Complete；Pressed 守卫 + on_created 配对 + consecutive_summon_close 实跑 |
| PAL-02 | 03-01 / 03-04 | 命令面板列出所有模块注册的命令 | ✓ SATISFIED | REQUIREMENTS.md:40 `[x]` + traceability Complete；UV 分派修复 + glyph_shape 实跑（CJK 命令名字形结构断言） |
| PAL-03 | 03-02 | 模糊过滤命令列表 | ✓ SATISFIED | `[x]` Complete；fuzzy_navigation_execute 实跑通过 |
| PAL-04 | 03-02 | 方向键导航，回车执行 | ✓ SATISFIED | `[x]` Complete；fuzzy_navigation_execute + capture_hides_first 实跑通过 |
| PAL-05 | 03-02 | ESC 关闭命令面板 | ✓ SATISFIED | `[x]` Complete；five_summon_esc_no_residue 实跑通过 |

**追踪表状态：** REQUIREMENTS.md 中 PAL-01..PAL-05 全部 `[x]` 且 traceability 表全部 Complete（上轮指出的 PAL-01/PAL-02 陈旧问题已被 03-03/03-04 修复）。ROADMAP Phase 3 标 completed。无 orphaned requirements（Phase 3 声明 5 个 ID 全部被 plan 认领）。

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|---------|
| crates/modules/palette/src/lib.rs | 306 | 高度同步依赖帧内 prev/next 快照比较（Executing/Error 转变不触发） | ⚠️ Warning | REVIEW WR-01：Executing/Error 态窗口不增高，列表底部裁切——03-02 既有代码，不影响 gap 关闭判定 |
| crates/modules/palette/src/lib.rs | 179 | 帧缓冲仅 summon 时分配一次，窗口增高后新区域无绘制 | ⚠️ Warning | REVIEW WR-02：WR-01 修复前为潜伏态（高度不变），不阻断 |
| crates/modules/palette/src/session.rs + app.rs:519-521 | 151-176 | 创建失败路径 pending_close 无清除，热键切换永久失效 | ⚠️ Warning | REVIEW WR-03：GAP-1 新配对设计的健壮性缺口——deferred Phase 4（错误处理打磨），生产正常路径不可达 |
| crates/modules/palette/src/raster.rs | 31-48, 299-304 | ImageData::Color 纹理双倍 premultiply | ⚠️ Warning | REVIEW WR-04：潜伏缺陷（当前仅 Font 图集纹理被行使，r=g=b=a 时 premultiply==straight），首个 image/icon 纹理模块接入前需修 |
| crates/modules/palette/tests/integration.rs | 87-93 | glyph_shape doc 注释阈值描述与实现不符 | ℹ️ Info | REVIEW IN-04：文档陈旧（提覆盖 <0.7/全覆盖像素，实际为 bbox/kinds/aa_spread） |

无 TBD/FIXME/XXX 债务标记 → 无 🛑 Blocker。空 match 臂均为枚举穷举合法分支，无 stub。

### Human Verification Required

1. **UAT 1 重跑（GAP-1 关闭确认）** — 按 Cmd+Shift+Space 唤出→再按关闭→再唤出，循环 ≥3 轮；执行「开始截图」后再唤出面板。**Expected:** 每次均保持显示；**Why human:** 探针走 bus 级 summon，OS 物理热键链路只能真人验证。
2. **UAT 5 重跑（GAP-2 关闭确认）** — 面板内中文命令名/描述/占位符均为可识别字形。**Expected:** 无灰色方块；**Why human:** 最终视觉验收需人眼（代码/探针证据已强：aa_spread 242 vs 旧 bug 40）。
3. **UAT 2 补跑** — 过滤/导航走查（高亮 #FF6000、环绕、Enter/ESC）。**Why human:** 视觉高亮与键盘手感。
4. **UAT 3 补跑** — 截图时序硬约束（面板不出现于截图）。**Why human:** 真实时序需真人确认截图内容。
5. **UAT 4 补跑** — 四个内置命令 OS 副作用。**Why human:** 进程/文件管理器副作用无法安全在验证进程内执行。

### Gaps Summary

**本轮无 gap。** 上轮两个 BLOCKER（GAP-1 热键重复唤出闪退、GAP-2 文字灰色块）均已在代码层关闭：

- GAP-1：Pressed 守卫消除 macOS/Windows 热键双报（单测锁定），`WindowSpec.on_created` 主线程同步配对消除跨模块污染与异步竞态（探针 3 轮建销循环实跑通过）。capture 的广播订阅与 core 的广播发射均保留，跨模块契约未破坏。
- GAP-2：UV（WHITE_UV）契约分派使字形三角形进入纹理采样（RED 测试锁定），partial 图集补丁原位写入保护已光栅化字形（单测 + Ime 注入探针锁定，diff=52830 证明路径真实行使）。执行期额外修复半纹素采样偏移与帧间清屏，均为渲染正确性所需。

质量账本：215 单测（含 7 个新增 gap 回归测试）、6/6 E2E 探针、cargo check 零 warning、REQUIREMENTS.md 全 Complete。4 项 REVIEW warning 均为边界/潜伏场景，其中 1 项（WR-03 创建失败路径）有明确 Phase 4 归属，3 项为不影响本次验收的既有/潜伏缺陷（详情见 Anti-Patterns 表）。

**剩余动作全部属于人类验收：** 2 项 gap 关闭确认（UAT 1/5 重跑）+ 3 项上轮未完成的 UAT 补跑。建议用户重跑后更新 03-HUMAN-UAT.md，全部通过后本阶段可判定 passed 并推进 Phase 4。

---

_Verified: 2026-08-15T05:41:03Z_
_Verifier: the agent (gsd-verifier)_
