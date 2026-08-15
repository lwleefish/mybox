---
phase: 03-命令面板
verified: 2026-08-15T09:05:45Z
status: gaps_found
score: 18/19 must-haves verified
overrides_applied: 0
re_verification:
  previous_status: human_needed
  previous_score: 11/11
  gaps_closed:
    - "GAP-3 面板位置漂移 — sync_window_geometry 去重居中（只 request_inner_size 绝不 set_outer_position）+ geometry_revision 修订计数触发器（WR-01 一并关闭）+ resize_framebuffer 帧缓冲伸缩（WR-02 一并关闭）"
    - "GAP-4 hover 高亮错位 — 行交互/绘制统一到 ScrollArea 内容 ui 坐标系 + item_spacing.y 归零（幻影滚动归零）+ 行内布局落回 48px 行高"
    - "GAP-5 点击无反应 — 行交互 Sense::click + make_persistent_id 稳定 id + clicked→execute::execute（与 Enter 同语义同守卫）"
    - "GAP-6 Ctrl+P/N 缺失 — ModifiersChanged 事件流跟踪修饰键 + on_palette_key Ctrl+P/N 守卫臂（等价 ↑/↓ 环绕）+ summon 重置防跨窗口残留"
    - "GAP-7 前半（前缀发现） — 四个内置命令拼音 keywords（tuichu/peizhi/chongqi/rizhi），fuzzy-matcher 关键词梯队天然支持"
    - "GAP-7 后半（IME 输入）首次唤出路径 — ensure_winit_state 首次事件显式 window.set_ime_allowed(true)，ime_allowed 标志锁定"
  gaps_remaining:
    - "WR-01（本轮新发现，BLOCKER）：IME 显式开启是 per-session 一次性——第二次及以后的唤出窗口不再收到 set_ime_allowed(true)，egui-winit 0.30 allow_ime 去抖无翻转不重开，重唤出面板无法输入中文"
  regressions: []
deferred:
  - truth: "Windows 上热键 Send 化（HotkeyManager 延迟注册模式）、Windows 字体发现、explorer 打开文件行为"
    addressed_in: "Phase 4"
    evidence: "Phase 4 成功标准 3：'命令面板在 Windows 上可唤出并执行命令'；goal '在 Windows 上完成适配'"
  - truth: "sync_window_geometry 负坐标 clamp（左侧/上方副屏重居中跳主屏）——REVIEW WR-01"
    addressed_in: "Phase 4"
    evidence: "Phase 4 goal 明确包含 '多显示器'"
  - truth: "runner panic 无 catch_unwind 保护（REVIEW WR-03）+ run_command 线程 spawn panic（本轮 IN-01）"
    addressed_in: "Phase 4"
    evidence: "Phase 4 goal 明确包含 '错误处理打磨' + plan 04-02 'DPI 缩放修复 + 错误处理打磨'"
  - truth: "窗口创建失败会永久卡住 pending_close（本轮 REVIEW WR-03 重申）"
    addressed_in: "Phase 4"
    evidence: "Phase 4 goal '错误处理打磨' + plan 04-02；缺陷仅经创建失败路径可达（softbuffer surface 失败），生产正常路径不受影响"
gaps:
  - truth: "面板【每一次】窗口创建后即显式请求 IME 允许（GAP-7 输入子问题在重唤出场景同样消除）"
    status: failed
    reason: "IME 显式开启是 per-session 一次性：ime_allowed 标志置位后永不复位、egui-winit State 创建一次永不重建（summon/close 均无复位）。第二次及以后的唤出窗口不再收到 set_ime_allowed(true)。且 egui-winit 0.30 的去抖（vendored source lib.rs:848-852 已验证：仅 allow_ime 翻转时调用 window.set_ime_allowed）在复用 State 时 allow_ime 保持 true → 无翻转 → 永不重开；winit macOS 后端每窗口默认禁用 IME。结论：ESC/热键关闭后重唤出（GAP-1 强制打牢的核心循环，UAT 测试 1 的同一流程），中文输入法再次不可用。现有 ime_commit_updates_input 探针只覆盖首次唤出，10/10 集成测试未能捕获此洞。"
    artifacts:
      - path: "crates/modules/palette/src/session.rs"
        issue: "summon() (130-149) 只复位 modifiers/input/selection 等，不复位 ime_allowed=false 与 winit_state=None；ensure_winit_state (478-503) 的 ime_allowed 守卫一次生效后永不复位，winit_state 只创建一次"
      - path: "crates/modules/palette/src/bin/palette_checks.rs"
        issue: "check_ime_commit_updates_input (1753+) 仅单次唤出（stage 0 断言首次事件 ime_allowed 标志），无 ESC 关闭→再唤出的重唤出阶段，覆盖声明未承诺第二窗口 IME 断言"
    missing:
      - "summon()（或 close() 等价路径）复位 inner.ime_allowed = false 与 inner.winit_state = None，使每次窗口创建都重新执行显式 window.set_ime_allowed(true) 并针对新窗口重建 egui-winit State（REVIEW WR-01 的修复建议）"
      - "E2E 探针 ime_commit_updates_input 增加重唤出阶段：summon → ESC 配对 Destroy → 再 summon → 断言第二窗口的 ime_allowed 复位→重新置位路径被行使（可加 Ime::Commit 中文断言于第二窗口）"
      - "建议同一 gap-closure 计划顺带修复同函数两个 warning：WR-04（sync_window_geometry 对 Hidden 态加早退，避免 capture.start 点击执行路径的 1px resize）与 WR-02（summon_palette 初始高度 all.len() 改 all.len().max(1)，与帧循环同步规则一致）"
---

# Phase 3: 命令面板 Verification Report（Re-verification — gap closure 03-05..03-08）

**Phase Goal:** 实现命令面板作为所有模块的统一交互入口。全局快捷键唤出，展示已注册命令，模糊搜索，键盘导航执行。
**Mode:** mvp
**Verified:** 2026-08-15T09:05:45Z
**Status:** gaps_found
**Re-verification:** Yes — after gap closure（previous: human_needed 11/11；本轮 18/19，1 个新 BLOCKER）

> **MVP 模式格式守卫（Escalation 项，上上轮提出、上轮重申、本轮复检仍不满足）：** `gsd-sdk query user-story.validate` 对 ROADMAP Phase 3 goal 返回 `false`（本 verifier 本轮重新执行确认）。plan-level 用户故事（03-01：唤出面板并看到全部命令；03-02：输入过滤/键盘选择执行）可规范，本报告据此构建用户流程覆盖。**建议用户运行 `/gsd mvp-phase 03` 将 ROADMAP goal 重写为用户故事格式**——不阻塞本验证（5 条成功标准无歧义），但 UAT 脚本生成质量依赖它。

## 验证结论（先行）

**03-05..03-08 四个 gap-closure 计划本 verifier 全部实跑复验（非仅信 SUMMARY）：**

- **GAP-3 关闭确认** — `sync_window_geometry` 函数体内确无 `set_outer_position`（lib.rs:486-505，只 request_inner_size + resize_framebuffer）；geometry_revision 在 summon/set_input/set_executing-成功/finalize-Err 四类转变递增（session.rs:133/282/539/579）。E2E `position_stable_on_filter` 本 verifier 桌面会话实跑 PASS：过滤收缩 320→128、恢复 128→320、Executing 增高 320→352 三阶段 outer_position 与召唤原点**精确相等**、帧缓冲全程覆盖窗口物理尺寸。
- **GAP-4/5 关闭确认** — `draw_command_row` 使用 `ui.interact(row_rect, make_persistent_id(("palette-row", cmd.id)), Sense::click())` + `resp.clicked()` → `execute::execute`（ui.rs:397-419）；行绘制走 ScrollArea 内容 ui 的 `ui.painter()`（ui.rs:332）；`item_spacing.y = 0.0`（ui.rs:137/329）；name/desc 布局落回 48px 行高。E2E `hover_click_alignment` 本 verifier 实跑 PASS：**hover_px_in_band=94049、hover_px_above_band=0、text_px_in_band=16549**（@2x），点击经真实事件链进入 Executing、gated runner 恰好执行一次。
- **GAP-6 关闭确认** — `on_event_win` 闭包 `WindowEvent::ModifiersChanged(m) => session.set_modifiers(m.state())`（lib.rs:237-239，不早退）；`on_palette_key` 含 modifiers 参数与两个 `Key::Character` Ctrl 守卫臂（lib.rs:438-447，winit 0.30 NamedKey 无字母变体属实——0.31 概念）；summon 重置修饰键（session.rs:137）。E2E `ctrl_pn_navigation` 本 verifier 实跑 PASS：真实 ModifiersChanged 注入 → 环绕断言（Some(2)/Some(0)）→ 清空断言 → 无修饰键 ESC 回归。
- **GAP-7 部分关闭（见 BLOCKER）** — 拼音 keywords 四个字面量落位（command.rs:111/134/150/179）且单测锁定；`ensure_winit_state` 锁外 `window.set_ime_allowed(true)`（session.rs:500）。E2E `ime_commit_updates_input` 本 verifier 实跑 PASS：ime_allowed 标志、中文 Commit → session.input=="截图" → filtered [0]、`tuichu` → filtered [1] 全部断言通过。**但首次唤出之外的窗口路径存在 REVIEW WR-01 缺陷（见下）——GAP-7 只对首次唤出关闭。**

## User Flow Coverage (MVP Mode)

User story（合并 03-01 + 03-02 的 plan-level 用户故事）：
«As a mybox 用户, I want to 按全局快捷键唤出屏幕中央的命令面板浮窗、看到全部已注册命令、输入关键词即时过滤、用方向键/鼠标/Ctrl+P/N 选择并执行命令, so that 所有工具通过一个统一入口触手可及。»

| Step | Expected | Evidence | Status |
|------|----------|----------|--------|
| 按 Cmd+Shift+Space | 活动显示器中央出现无边框置顶浮窗 | app.rs Pressed 守卫 + lib.rs hotkey→toggle + on_created 配对（上轮已验，无改动）；consecutive_summon_close 探针本 verifier 实跑 PASS | ✓（物理按键留待 UAT 1） |
| 再按热键/ESC 关闭后再唤出 | 面板保持显示不闪退；位置不漂移 | consecutive_summon_close + position_stable_on_filter（三阶段 outer_position 精确相等）本 verifier 实跑 PASS | ✓ |
| 看到命令列表（文字可读） | 中文命令名/描述/占位符为可识别字形 | glyph_shape 本 verifier 实跑：bbox=1200x288 non_bg=24248 kinds=53 **aa_spread=242** | ✓ |
| 输入「截图」/「jt」/「tuichu」 | 命中命令、命中字符高亮、面板位置不动 | fuzzy_navigation_execute + position_stable_on_filter + ime_commit_updates_input（拼音命中断言）本 verifier 实跑 PASS | ✓ |
| 鼠标 hover / 点击行 | 高亮与文字同矩形；点击执行 | hover_click_alignment 本 verifier 实跑：带内 94049 高亮像素、行上带 0、文字与高亮同带、点击→执行 | ✓ |
| ↑/↓/Ctrl+P/Ctrl+N/Enter | 环绕导航、回车执行、执行中面板保持 | fuzzy_navigation_execute + ctrl_pn_navigation 本 verifier 实跑 PASS | ✓ |
| 中文输入（IME） | 输入框可输入中文并过滤 | **首次唤出**：ime_commit_updates_input 实跑 PASS；**重唤出（ESC 关闭后再唤出）**：代码可证 IME 不会重开（REVIEW WR-01，见 BLOCKER gap） | ✗ 首次 ✓ / 重唤出 ✗ |
| Outcome | 统一入口可用——快捷键唤出、键盘/鼠标全程操作、文字清晰 | 228 单测 + 10/10 E2E + cargo check 全绿（本 verifier 实跑） | ✓（含 1 个重唤出 IME 洞） |

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| SC-1 | 用户按全局快捷键唤出命令面板浮窗（PAL-01） | ✓ VERIFIED（回归） | 上轮链路无改动；consecutive_summon_close 实跑 PASS |
| SC-2 | 面板列出截图模块注册的命令（PAL-02） | ✓ VERIFIED（回归） | glyph_shape 实跑 PASS（CJK 命令名字形结构断言） |
| SC-3 | 输入关键词可模糊过滤命令列表（PAL-03） | ✓ VERIFIED（回归+强化） | fuzzy_navigation_execute + ime_commit_updates_input（tuichu 拼音命中）实跑 PASS |
| SC-4 | 方向键选择命令，回车执行对应功能（PAL-04） | ✓ VERIFIED（回归+强化） | fuzzy_navigation_execute + hover_click_alignment + ctrl_pn_navigation 实跑 PASS |
| SC-5 | 按 ESC 关闭命令面板（PAL-05） | ✓ VERIFIED（回归） | five_summon_esc_no_residue 实跑 PASS |
| 03-05-T1 | 过滤改变命令项数量时窗口高度收缩/恢复但位置保持不动（GAP-3） | ✓ VERIFIED | sync_window_geometry 无 set_outer_position（lib.rs:486-505）；position_stable_on_filter 实跑：三阶段 outer_position == summon 原点精确相等 |
| 03-05-T2 | 状态转变（Executing/Error）同样触发高度同步且不移动窗口（WR-01） | ✓ VERIFIED | geometry_revision 四类转变递增（session.rs:133/282/539/579）+ 帧循环修订计数比对（lib.rs:332-337）；探针 stage 3 Executing 增高 320→352 位置零漂移 |
| 03-05-T3 | 窗口增高后新区域正常绘制——帧缓冲始终覆盖窗口物理尺寸（WR-02） | ✓ VERIFIED | resize_framebuffer（session.rs:391）+ sync 调用点（lib.rs:504）；探针三阶段帧缓冲 1200×{256,640,704} 全程覆盖断言 |
| 03-06-T1 | hover 高亮块与该行文字区域精确重叠（GAP-4） | ✓ VERIFIED | 内容 ui painter（ui.rs:332）+ item_spacing.y=0（ui.rs:137/329）；探针 hover_px_above_band=0、text_px_in_band=16549 |
| 03-06-T2 | 行内 name/description 完整落在 48px 行高内 | ✓ VERIFIED | name_pos=row.top+SP_SM、desc_pos=name 底+SP_XS（ui.rs:422-423）；单测 row_geometry_fits_48px（228/228 全绿） |
| 03-06-T3 | 点击命令项执行对应命令（GAP-5） | ✓ VERIFIED | Sense::click + clicked→execute::execute（ui.rs:397-419）；探针点击→Executing→gated runner 恰好一次；headless 单测 row_interact_hovers_and_clicks_execute |
| 03-06-T4 | 列表无幻影滚动（n 行内容高度 == 视口高度） | ✓ VERIFIED | item_spacing.y 归零 + allocate_rect 精确预留；探针 hover 填充精确落 68..116 行带（无偏移证据） |
| 03-07-T1 | Ctrl+P/N 与 ↑/↓ 等价（环绕），Idle/Filtering 生效（GAP-6） | ✓ VERIFIED | 守卫臂 lib.rs:438-447；ctrl_pn_navigation 实跑：Idle 无选中 Ctrl+P→Some(2)（环绕末位）、Ctrl+N→Some(0) |
| 03-07-T2 | 普通 P/N 输入不被路由消费（无 Ctrl 透传 TextEdit） | ✓ VERIFIED | 守卫不满足落 `_ => false`（lib.rs:473）；单测 plain_p_without_ctrl_is_not_consumed 实跑通过 |
| 03-07-T3 | Error 态 Ctrl+P/N 仍走「任意键关闭」 | ✓ VERIFIED | Error 首臂优先（lib.rs:415-418）；单测 ctrl_pn_in_error_state_closes_panel 实跑通过 |
| 03-07-T4 | 修饰键经真实 ModifiersChanged 跟踪，新窗口唤出重置 | ✓ VERIFIED | lib.rs:237-239 接线 + session.rs:137 summon 重置；探针真实事件注入 + 单测 summon_resets_modifiers |
| 03-08-T1 | **每一次**面板窗口创建后即显式请求 IME 允许（GAP-7 输入子问题） | ✗ FAILED | **首次唤出** ✓：ensure_winit_state 显式 set_ime_allowed(true)（session.rs:500）+ 探针 ime_allowed 标志断言。**重唤出** ✗：ime_allowed 一次置位永不复位、winit_state 永不重建（summon/close 均无复位）；egui-winit 0.30 去抖仅在 allow_ime 翻转时调用 set_ime_allowed（vendored lib.rs:848-852 已验证）→ 复用 State 无翻转 → 新窗口永不重开；winit macOS 每窗口默认禁用 IME → 中文输入死。探针只覆盖首次唤出（BLOCKER gap，见 frontmatter） |
| 03-08-T2 | 注入 Ime::Commit 中文经完整链路进入输入并触发过滤 | ✓ VERIFIED | ime_commit_updates_input 实跑：input=="截图"、Filtering、filtered==[0] |
| 03-08-T3 | 四个内置命令可用拼音关键词命中（前缀发现路径） | ✓ VERIFIED | command.rs:111/134/150/179 + 单测 builtin_keywords_include_pinyin_aliases；探针 tuichu→filtered [1] |

**Score:** 18/19 truths verified（5 roadmap SC + 14 gap-closure truths；03-08-T1 首次唤出分支成立、重唤出分支失败）

### Deferred Items

| # | Item | Addressed In | Evidence |
|---|------|-------------|----------|
| 1 | Windows 热键 Send 化、字体发现、explorer 打开文件行为 | Phase 4 | Phase 4 SC-3 + goal（Windows 适配） |
| 2 | sync_window_geometry 负坐标 clamp（多显示器） | Phase 4 | Phase 4 goal「多显示器」 |
| 3 | runner panic 无 catch_unwind（含本轮 IN-01 线程 spawn panic） | Phase 4 | Phase 4 goal「错误处理打磨」+ plan 04-02 |
| 4 | 窗口创建失败永久卡死 pending_close（本轮 REVIEW WR-03 重申） | Phase 4 | Phase 4 goal「错误处理打磨」+ plan 04-02；仅创建失败路径可达，不影响正常使用 |

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| crates/modules/palette/src/session.rs | geometry_revision 修订计数 + resize_framebuffer + modifiers 跟踪 + ime_allowed 标志 | ✓ VERIFIED | 66/112 字段与构造；133/282/539/579 四类递增；391 伸缩（同尺寸零分配）；232/247 修饰键访问器；478-503 IME 显式开启（锁外 winit 调用） |
| crates/modules/palette/src/lib.rs | sync_window_geometry 去重居中 + 修订计数触发 + ModifiersChanged 接线 + Ctrl+P/N 守卫臂 + ui::draw 接线 | ✓ VERIFIED | 486-505（无 set_outer_position）；332-337（revision 比对）；237-239（modifiers 事件流）；438-447（Ctrl 臂）；292（ui::draw 4 参） |
| crates/modules/palette/src/ui.rs | draw 签名扩展 + draw_command_row 重写（content-ui painter / Sense::click / clicked→execute / 48px 布局）+ item_spacing 归零 | ✓ VERIFIED | 91-96 签名；137/329 spacing；332 行 painter；397-419 交互与点击执行；422-423 布局；2 个新单测 |
| crates/mybox-core/src/command.rs | 四个内置命令拼音 keywords | ✓ VERIFIED | 111/134/150/179 + 单测 440-468 |
| crates/modules/palette/src/bin/palette_checks.rs | 4 个新探针 + window_outer_position/window_inner_size/press_key_mods 辅助 + main 分发 | ✓ VERIFIED | 1148/1372/1597/1753 四探针；149/158 辅助；280 press_key_mods；1890-1893 main 分发 |
| crates/modules/palette/tests/integration.rs | 4 个新 #[ignore] 测试接线（总计 10） | ✓ VERIFIED | 100-153 十测试全部接线，文档注释含 PAL/GAP 映射 |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|----|---------|---------|
| 过滤输入 / 状态转变（set_input/set_executing/finalize-Err） | sync_window_geometry | geometry_revision 修订计数比对（帧内快照无法察觉帧外转变——WR-01 根因） | ✓ WIRED | session.rs 四类递增 + lib.rs:332-337 比对；探针三阶段高度实测 |
| 窗口高度变化 | 窗口位置 | 只 request_inner_size 绝不 set_outer_position（GAP-3 根因：重居中使顶边下移） | ✓ WIRED | lib.rs:486-505 函数体内无 set_outer_position（源码断言确认）；探针 outer_position 精确相等 |
| sync_window_geometry | session 帧缓冲 | resize_framebuffer(new_size)（WR-02：增高后新区域可绘制） | ✓ WIRED | lib.rs:504；探针帧缓冲覆盖断言 |
| ScrollArea 内容 ui 行矩形 | 行高亮/文字绘制 | 同一内容 ui 的 ui.painter()（两坐标系仅在偏移 0 时重合的根因类消除） | ✓ WIRED | ui.rs:332 row_painter；探针 hover_px_above_band=0 |
| 鼠标点击（egui-winit 转换） | execute::execute | ui.interact(Sense::click) → resp.clicked()（原 hover-only 永无 click） | ✓ WIRED | ui.rs:397-419；探针点击→Executing 断言 |
| winit ModifiersChanged | session.modifiers | session.set_modifiers(m.state())（winit 0.30 KeyEvent 无 modifiers 字段） | ✓ WIRED | lib.rs:237-239；探针真实事件注入断言 |
| KeyboardInput + session.modifiers | on_palette_key 路由 | Ctrl+P/N 守卫臂（control_key() + 字符 p/n）等价 move_selection(∓1) | ✓ WIRED | lib.rs:438-447；单测 5 个 + 探针环绕断言 |
| 无 Ctrl 的普通 P/N | egui TextEdit | 守卫不满足 → 路由返回 false → 事件透传 | ✓ WIRED | lib.rs:473 `_ => false`；单测 plain_p_without_ctrl_is_not_consumed |
| 窗口首事件（ensure_winit_state） | OS IME 输入通道 | window.set_ime_allowed(true)（首次窗口）+ egui-winit 去抖 | ⚠️ PARTIAL | session.rs:500 仅首次窗口；重唤出窗口无任何 set_ime_allowed 调用路径（BLOCKER gap） |
| winit Ime::Preedit/Commit | session.set_input | egui-winit State::on_window_event → egui Event::Ime → TextEdit → changed() | ✓ WIRED | 探针实跑 input=="截图"、Filtering |
| 拼音查询（tuichu 等） | 内置命令 | fuzzy-matcher 关键词梯队（keywords 纯数据，与 jietu 同机制） | ✓ WIRED | command.rs 数据 + 探针 filtered [1] 断言 |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|-------|--------------------|--------|
| sync_window_geometry | physical_h / last_height | ui::window_height 几何表 × 修订计数触发 | Yes — 探针实测 256/640/704 物理高度达成、位置零漂移 | ✓ FLOWING |
| resize_framebuffer | framebuffer Pixmap | 窗口物理尺寸驱动重分配 | Yes — 探针断言 Pixmap 尺寸 == 窗口物理尺寸全程覆盖 | ✓ FLOWING |
| draw_command_row 交互 | resp (egui Response) | egui hit-test（prev_pass.widgets） | Yes — 探针帧缓冲 94049 高亮像素、点击→Executing→runner 一次 | ✓ FLOWING |
| on_palette_key Ctrl 判定 | modifiers | winit ModifiersChanged 事件流 → session | Yes — 探针真实事件注入 → control_key() 断言 | ✓ FLOWING |
| ensure_winit_state | ime_allowed 标志 | 首次窗口事件一次性置位 | Yes（首次）— 探针断言置位；**重唤出不流动（无复位、无重开）** | ⚠️ PARTIAL |
| 拼音 keywords | command.keywords | 编译期静态数据 → fuzzy-matcher | Yes — 探针 tuichu→filtered [1] | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| 全 workspace 单测 | cargo nextest run --workspace | 228 passed / 18 skipped | ✓ PASS |
| palette 包单测 | cargo nextest run -p mybox-palette | 66 passed / 9 skipped（含 5 个 Ctrl 路由 + 2 个修饰键 + 2 个行交互新测试） | ✓ PASS |
| core command 单测 | cargo nextest run -p mybox-core command | 11/11（含 builtin_keywords_include_pinyin_aliases） | ✓ PASS |
| 编译健康 | cargo check --workspace | exit 0，无 warning | ✓ PASS |
| E2E 集成测试（桌面会话，本 verifier 实跑） | cargo test -p mybox-palette --test integration -- --ignored | **10/10 PASS**（3.52s） | ✓ PASS |

### Probe Execution

| Probe | Command | Result | Status |
|-------|---------|--------|---------|
| summon_render | bash palette_checks（经 integration.rs） | exit 0 | ✓ PASS |
| fuzzy_navigation_execute | 同上 | exit 0 | ✓ PASS |
| capture_hides_first | 同上 | exit 0 | ✓ PASS |
| five_summon_esc_no_residue | 同上 | exit 0 | ✓ PASS |
| consecutive_summon_close | 同上 | exit 0 | ✓ PASS（GAP-1 回归） |
| glyph_shape | 同上 | exit 0，实测 bbox=1200x288 non_bg=24248 diff=45942 kinds=53 **aa_spread=242** | ✓ PASS（GAP-2 回归） |
| position_stable_on_filter | 同上 | exit 0（三阶段 outer_position == 原点精确相等） | ✓ PASS（GAP-3 回归，本 verifier 实跑） |
| hover_click_alignment | 同上 | exit 0，实测 **hover_px_in_band=94049 / hover_px_above_band=0 / text_px_in_band=16549** @2x | ✓ PASS（GAP-4/5 回归，本 verifier 实跑） |
| ctrl_pn_navigation | 同上 | exit 0（真实 ModifiersChanged 注入 + 环绕断言） | ✓ PASS（GAP-6 回归，本 verifier 实跑） |
| ime_commit_updates_input | 同上 | exit 0（ime_allowed 标志 + 中文 Commit + tuichu 拼音过滤） | ✓ PASS（GAP-7 首次唤出路径，本 verifier 实跑） |

**探针覆盖声明核实：** 各探针的 doc 注释覆盖声明与实现一致（position/hover/ctrl/ime 均明示合成事件注入边界、OS 物理输入留待人工）。**核实发现的覆盖洞：** ime_commit_updates_input 的覆盖声明未承诺重唤出窗口——而 WR-01 恰在重唤出窗口上使 GAP-7 失效（探针通过但缺陷真实存在）。

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|---------|
| PAL-01 | 03-01 / 03-03 | 用户按全局快捷键唤出命令面板浮窗 | ✓ SATISFIED | REQUIREMENTS.md:39 `[x]` Complete；consecutive_summon_close 实跑 |
| PAL-02 | 03-01 / 03-04 | 命令面板列出所有模块注册的命令 | ✓ SATISFIED | `[x]` Complete；glyph_shape aa_spread=242 实跑 |
| PAL-03 | 03-02 / 03-05 / 03-08 | 输入关键词模糊过滤命令列表 | ✓ SATISFIED | `[x]` Complete；fuzzy_navigation_execute + ime_commit_updates_input（拼音路径）实跑 |
| PAL-04 | 03-02 / 03-05 / 03-06 / 03-07 | 方向键导航选择，回车执行 | ✓ SATISFIED | `[x]` Complete；hover_click_alignment + ctrl_pn_navigation 实跑（点击/Ctrl+P/N 强化） |
| PAL-05 | 03-02 | ESC 关闭命令面板 | ✓ SATISFIED | `[x]` Complete；five_summon_esc_no_residue 实跑 |

**追踪表状态：** REQUIREMENTS.md 中 PAL-01..PAL-05 全部 `[x]` 且 traceability 表全部 Complete。8 张 PLAN 的 `requirements:` 前端字段合计认领 PAL-01×2 / PAL-02×2 / PAL-03×3 / PAL-04×4 / PAL-05×1 —— **无 orphaned requirements**（Phase 3 声明的 5 个 ID 全部被 plan 认领）。ROADMAP Phase 3 标 completed、8 plans 全部 `[x]`。

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|---------|
| crates/modules/palette/src/session.rs | 130-149, 478-503 | IME 显式开启 per-session 一次性：ime_allowed 不复位、winit_state 不重建 → 重唤出窗口永不 set_ime_allowed(true) | 🛑 Blocker | REVIEW WR-01：ESC/热键关闭后重唤出（核心循环）中文输入法死亡。egui-winit 0.30 去抖源码已验证（仅 allow_ime 翻转调用）；探针只覆盖首次唤出 |
| crates/modules/palette/src/lib.rs | 486-504 | Hidden 态窗口高度 0 → max(1) 1px resize + 1px 帧缓冲重分配 | ⚠️ Warning | REVIEW WR-04：capture.start 点击执行路径可达（同帧 revision 触发），瞬态（Destroy 随后排出）、下次 summon 重装帧缓冲，无持久损坏 |
| crates/modules/palette/src/lib.rs | 176 vs 491 | 初始高度 all.len() 与帧循环 len().max(1) 不一致（零命令 80 vs 128） | ⚠️ Warning | REVIEW WR-02：潜伏缺陷（生产恒有 4 内置命令，不可达）；修复 = 两处统一 max(1) |
| crates/mybox-core/src/app.rs | 518-521 + session.rs | 窗口创建失败仅日志，session 永久 pending_close 卡死 | ⚠️ Warning | REVIEW WR-03（重申）：deferred Phase 4，生产正常路径不可达 |
| crates/mybox-core/src/command.rs | 239-245 | run_command 线程 spawn expect panic（主线程） | ℹ️ Info | IN-01：资源耗尽场景；deferred Phase 4 错误处理打磨 |
| crates/modules/palette/src/session.rs | 481-490 vs lib.rs:288-292 | 锁顺序相反（state→egui_ctx vs egui_ctx→state） | ℹ️ Info | IN-02：今日无嵌套无死锁；修复 WR-01 时建议顺手加不变式文档 |
| crates/modules/palette/src/bin/palette_checks.rs | 82-124 | realize_window 注释不准确（旧窗口由 harness WM 持有不销毁） | ℹ️ Info | IN-03：测试脚手架注释级，不影响断言 |
| crates/mybox-core/src/app.rs | 145-146 | config_dir().unwrap_or_default() 静默降级空路径 | ℹ️ Info | IN-04：open_config/open_log 在配置目录失败时报错体验差 |
| crates/modules/palette/src/ui.rs | 175-207 | 64 字符截断在帧重建时可见回弹（无提示） | ℹ️ Info | IN-05：SPEC 约束合规但 UX 粗糙；建议 char_limit |

无 TBD/FIXME/XXX 债务标记（blocker 扫描零命中）；PLACEHOLDER 命中均为 UI 颜色 token（输入占位文本色），非 stub。空 match 臂均为枚举穷举合法分支。

### Human Verification Required

以下项目无法在验证进程内程序化完成（OS 物理输入链路 / 真实副作用），待 gap-closure（03-09）后随最终 UAT 执行：

1. **UAT 1 重跑（物理热键循环）** — 按 Cmd+Shift+Space 唤出→再按关闭→再唤出 ≥3 轮；执行「开始截图」后再唤出。**Expected:** 每次保持显示不闪退。**Why human:** 探针走 bus 级 summon，OS 热键注册→回调链路只能真人验证。
2. **UAT 3 重跑（截图时序硬约束）** — 执行「开始截图」，面板先消失、截图画面中绝不含面板。**Expected:** 截图中无面板。**Why human:** 探针只断言入队序，真实截图内容需真人确认。
3. **UAT 4 重跑（内置命令 OS 副作用）** — 退出/重启/打开配置目录/打开日志。**Expected:** 各自 OS 副作用正确。**Why human:** 进程生命周期/文件管理器无法在验证进程内执行。
4. **UAT 10 重跑（真实输入法）** — **首次唤出**输入中文确认候选窗出现；**ESC 关闭后重唤出再输入中文**。**Expected:** 两次都能输入中文（重唤出场景当前代码可证失败——03-09 修复后此步转为关闭确认）。**Why human:** OS 候选窗出现/交互无法合成。
5. **（可选）视觉/手感走查** — UAT 2/6/7/8/9 已由探针在真实窗口像素级断言（高亮颜色与位置、环绕、点击执行），剩余纯手感项（#FF6000 高亮观感、物理鼠标/键盘手感）可选复验。

### Gaps Summary

**本轮 1 个 BLOCKER gap（REVIEW WR-01），其余 6 个 GAP 全部关闭：**

- **GAP-3/4/5/6 关闭且双层锁定**（代码 + 探针，本 verifier 实跑复验）：位置漂移（去重居中 + 修订计数 + 帧缓冲伸缩）、hover 错位与点击无效（content-ui 坐标系 + Sense::click 接线）、Ctrl+P/N 缺失（ModifiersChanged 事件流 + 守卫臂）。REVIEW WR-01/WR-02（03-05 计划标称的几何 warning）已随 03-05 一并关闭。
- **GAP-7 部分关闭**：拼音前缀发现路径（数据层，全场景生效）与首次唤出 IME（代码层 + 探针）关闭；**重唤出 IME 未关闭**——03-08 的显式开启是 per-session 一次性，egui-winit 去抖在复用 State 时永不重开，winit macOS 每窗口默认禁用 IME，ESC 关闭后重唤出无法输入中文。该缺陷代码可证（本 verifier 独立验证 vendored egui-winit 0.30 源码 lib.rs:848-852 与 session.rs 无复位路径），且现有探针集 10/10 全绿仍无法捕获（覆盖洞：探针只测首次唤出）。
- **修复建议（小而确定）**：summon() 复位 `ime_allowed = false` 与 `winit_state = None`（2 行），使每次窗口创建重新执行显式开启并重建 State；探针增加重唤出阶段锁定；顺带同函数修复 WR-04（Hidden 早退）与 WR-02（max(1) 统一）。

质量账本：228 单测（03-05..03-08 新增 13 个）、10/10 E2E 探针（新增 4）、cargo check 零 warning、REQUIREMENTS.md PAL-01..05 全 Complete、无 TBD/FIXME/XXX。4 项 warning（WR-03/WR-04/WR-02/IN-01）中 WR-03 已 deferred Phase 4，WR-04/WR-02 建议并入 gap-closure 计划一并修复。

**推进建议：** 以 frontmatter `gaps` 结构化条目驱动 `/gsd-plan-phase --gaps` 生成 03-09 计划（IME 重唤出复位 + 探针重唤出阶段 + WR-04/WR-02 顺带修复）；03-09 关闭后，剩余 4 项人工验收（UAT 1/3/4/10）通过即本阶段 passed。

---

_Verified: 2026-08-15T09:05:45Z_
_Verifier: the agent (gsd-verifier)_
