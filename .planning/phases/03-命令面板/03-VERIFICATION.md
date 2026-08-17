---
phase: 03-命令面板
verified: 2026-08-17T15:30:00Z
status: human_needed
score: 25/25 must-haves verified
overrides_applied: 0
re_verification:
  previous_status: human_needed
  previous_score: 21/21
  gaps_closed:
    - "UAT gap 1 (minor, test 5): keyword 梯队命中无 #FF6000 高亮 — filter.rs Match.keyword_hit 数据通道 + ui.rs keyword tag 渲染（「 · {keyword}」行尾 + 命中字符 ACCENT）双修复落地"
    - "UAT gap 2 (major, test 11): 鼠标点击执行「开始截图」面板被拍进截图 — lib.rs RedrawRequested 帧循环 Hidden 守卫（egui_ctx.run 之后、apply_textures 之前）window.set_visible(false) 同步隐藏 + 早退"
    - "CR-01 (Critical, 03-REVIEW): keyword_tag_job 分隔符字节偏移错误 — KEYWORD_TAG_SEP const + sep_len() 推导前缀 + 单测字节偏移修正（j→4、t→7）+ CJK 无 panic 新单测（commit 32561c8）"
  gaps_remaining: []
  regressions: []
requirements:
  - id: PAL-01
    status: satisfied
    evidence: "REQUIREMENTS.md:39 [x] Complete；consecutive_summon_close 探针实跑 PASS（回归）；重唤出 IME 可达性 03-09 关闭"
  - id: PAL-02
    status: satisfied
    evidence: "REQUIREMENTS.md:40 [x] Complete；glyph_shape 探针实跑 aa_spread=242（回归）"
  - id: PAL-03
    status: satisfied
    evidence: "REQUIREMENTS.md:41 [x] Complete；fuzzy_navigation_execute + keyword_highlight 探针实跑 PASS（03-10 强化 keyword 梯队高亮全路径）"
  - id: PAL-04
    status: satisfied
    evidence: "REQUIREMENTS.md:42 [x] Complete；hover_click_alignment + ctrl_pn_navigation + click_hide_before_capture 探针实跑 PASS（03-10 强化点击路径时序与 Enter 一致）"
  - id: PAL-05
    status: satisfied
    evidence: "REQUIREMENTS.md:43 [x] Complete；five_summon_esc_no_residue 实跑 PASS；click_hide_before_capture stage 3 Destroy 配对断言"
human_verification:
  - test: "UAT 5 重跑（keyword 高亮——03-10 关闭项）——输入 jt / jietu / tuichu / peizhi / chongqi / rizhi，观察命令行 description 行尾的「 · {keyword}」标签"
    expected: "命中命令排前，且标签中命中的拼音字符（如 j/t）以 #FF6000 橙色高亮（E2E 探针已断言帧缓冲 ACCENT 像素 16/51 px；肉眼可见正确性需人工确认）"
    why_human: "OS 渲染链路的最终视觉效果（命中字符正确着色、非错位/非 e 字符）只能真人观察"
  - test: "UAT 11 重跑（鼠标点击截图时序——03-10 关闭项）——在面板中用鼠标点击「开始截图」"
    expected: "面板在截图覆盖层出现前已关闭，截图画面中绝不含面板本身（探针已断言 is_visible()==Some(false) 且 gated 读屏计数器==0；OS 合成器级真实截图需人工确认）"
    why_human: "真实截图的屏幕内容只能人工查看；探针经合成指针事件 + gated 模拟读屏锁定时序，物理鼠标链路留待人工"
  - test: "UAT 1 重跑（物理热键循环）——按 Cmd+Shift+Space 唤出→再按关闭→再唤出 ≥3 轮；执行「开始截图」后再唤出"
    expected: "每次保持显示不闪退"
    why_human: "探针走 bus 级 summon，OS 热键注册→回调链路只能真人验证"
  - test: "UAT 4 重跑（内置命令 OS 副作用）——执行退出/重启/打开配置目录/打开日志"
    expected: "各自 OS 副作用正确（进程生命周期/文件管理器打开正确位置）"
    why_human: "进程生命周期/文件管理器无法在验证进程内执行"
  - test: "UAT 10 重跑（真实输入法——首次唤出 AND ESC 关闭后重唤出）——两次都输入中文"
    expected: "候选窗两次都出现、可正常组合输入（03-09 GAP-8 已代码层关闭；OS 候选窗出现/交互仍只能人工确认）"
    why_human: "OS 候选窗出现/交互无法合成；探针仅注入合成 Ime 事件，不经 OS 输入法组合链路"
---

# Phase {3}: 命令面板 Verification Report（Re-verification — after 03-10 gap closure）

**Phase Goal:** 实现命令面板作为所有模块的统一交互入口。全局快捷键唤出，展示已注册命令，模糊搜索，键盘导航执行。
**Mode:** mvp
**Verified:** 2026-08-17T15:30:00Z
**Status:** human_needed
**Re-verification:** Yes — after 03-10 gap closure（previous: human_needed 21/21；本轮 25/25，UAT 2 个留存 gap + 03-REVIEW CR-01 全部代码层关闭，5 项人工 UAT 待执行）

> **MVP 模式格式守卫（Escalation 项，延续前轮）：** Phase 3 ROADMAP goal 非「As a…, I want to…, so that…」用户故事格式。`gsd-sdk query user-story.validate` 返回 `false`（本 verifier 复核确认）。不阻塞验证（5 条成功标准无歧义），沿用前轮决定继续验证。建议用户运行 `/gsd mvp-phase 03` 重写 goal 为用户故事格式。

## 验证结论（先行）

**03-10 gap-closure 计划（最后 1 个 plan，10/10）本 verifier 全部实跑复验（非仅信 SUMMARY）——UAT 2 个留存 gap 代码层关闭确认 + CR-01 Critical 修复落地确认：**

- **Gap 1 关闭（keyword 梯队高亮，UAT test 5）** — filter.rs `KeywordHit`（L52-56，自带 `#[derive(Clone, Debug, PartialEq, Eq)]`）+ `Match.keyword_hit: Option<KeywordHit>`（L44）；keyword 梯队分支改用 `fuzzy_indices` 逐 keyword 取最高分（L122-140，无 `fuzzy_match` 残留调用——仅 doc 注释提及）；空查询/name/description 分支均显式 `keyword_hit: None`（L89/111/114）。渲染层 ui.rs `keyword_tag_job`（L487-512）+ `KEYWORD_TAG_SEP` const（L485）+ `draw_command_row` 追加 keyword_hit 参数（L386）并在 desc 行尾渲染 tag（L434-443），`draw_command_list` 传 `hl.and_then(|m| m.keyword_hit.as_ref())`（L351）。单测：`query_jietu_hits_capture_via_pinyin_keyword` 断言 `KeywordHit { keyword: "jietu", indices: vec![0, 3] }`（L228-233）；`pinyin_keywords_all_carry_keyword_hit` 覆盖 tuichu/peizhi/chongqi/rizhi 全梯队（L238-269）；`name_tier_match_has_no_keyword_hit` + `empty_query_has_no_keyword_hit`（L271-293）。E2E 探针 `check_keyword_highlight` 本 verifier 桌面会话实跑：jt 阶段 16 ACCENT px、tuichu 阶段 51 ACCENT px（行 1 带内精确 #FF6000）。
- **Gap 2 关闭（点击路径截图时序，UAT test 11）** — lib.rs RedrawRequested 帧循环在 `egui_ctx.run`（L297）之后、`apply_textures`（L314）之前插入 Hidden 守卫（L309-312）：`if session.state() == PaletteState::Hidden { window.set_visible(false); return; }`——点击帧内 execute→close→Hidden 后同步隐藏窗口（macOS orderOut 即时）并跳过本帧 paint/present/request_redraw。E2E 探针 `check_click_hide_before_capture` 本 verifier 桌面会话实跑 PASS：stage 0 基线 `is_visible()==Some(true)`（防空洞断言）→ stage 3 断言 `state==Hidden` + `is_visible()==Some(false)` + gated 读屏计数器==0 + Destroy 已入队 → stage 4 释放后 counter==1 且无二次 Destroy（finalize 守卫 no-op）。execute.rs 未被改动（`git diff 2897afd..HEAD -- crates/modules/palette/src/execute.rs` 空）。
- **CR-01 修复确认（03-REVIEW Critical，commit 32561c8）** — `KEYWORD_TAG_SEP: &str = " · "` const（ui.rs L485）+ `sep_len() = KEYWORD_TAG_SEP.len()` 推导前缀（L489，杜绝硬编码 3）；单测字节偏移修正为 4/7（L683/L685）；新增 `keyword_tag_job_cjk_keyword_does_not_panic_and_marks_correct_chars`（L701-722，断言「截图」命中段 ACCENT 4..10 无 panic）。git show 确认 diff 仅触及 ui.rs（+KEYWORD_TAG_SEP/sep_len/偏移修正/CJK 测试），与 SUMMARY 声称一致。

**质量账本（本 verifier 实跑）：** `cargo nextest run --workspace` **233 passed / 0 failed**（20 skipped）；`cargo check --workspace` exit 0 **无 warning**；`cargo build -p mybox-palette --bin palette_checks` exit 0；`cargo test -p mybox-palette --test integration -- --ignored` **12/12 PASS** in 3.67s（含 keyword_highlight 16/51 ACCENT px + click_hide_before_capture 全 stage 断言）；`cargo nextest run -p mybox-palette filter::` 10/10；`cargo nextest run -p mybox-palette ui::` 8/8。

## User Flow Coverage (MVP Mode)

User story（合并 03-01 + 03-02 plan-level 用户故事，ROADMAP goal 非用户故事格式——见上方守卫）：
«As a mybox 用户, I want to 按全局快捷键唤出屏幕中央的命令面板浮窗、看到全部已注册命令、输入关键词即时过滤、用方向键/鼠标/Ctrl+P/N 选择并执行命令, so that 所有工具通过一个统一入口触手可及。»

| Step | Expected | Evidence | Status |
|------|----------|----------|--------|
| 按 Cmd+Shift+Space | 活动显示器中央出现无边框置顶浮窗 | app.rs:325 Pressed 守卫 + on_created 配对（回归）；consecutive_summon_close 探针实跑 PASS | ✓（物理按键留待 UAT 1） |
| 再按热键/ESC 关闭后再唤出 | 面板保持显示不闪退；位置不漂移；重唤出可输入中文 | consecutive_summon_close + position_stable_on_filter + ime_commit_updates_input 重唤出 stages 实跑 PASS | ✓ |
| 看到命令列表（文字可读） | 中文命令名/描述/占位符为可识别字形 | glyph_shape 实跑 aa_spread=242 | ✓ |
| 输入「截图」/「jt」/「tuichu」 | 命中命令排前、命中字符高亮、位置不动 | fuzzy_navigation_execute + position_stable_on_filter 实跑 PASS；**keyword_highlight 实跑 PASS（jt→capture.start 16 ACCENT px、tuichu→builtin.quit 51 ACCENT px——Gap 1 关闭）** | ✓（橙色高亮肉眼可见留待 UAT 5） |
| 鼠标 hover / 点击行 | 高亮与文字同矩形；点击执行 | hover_click_alignment 实跑 PASS；**click_hide_before_capture 实跑 PASS（点击→Hidden→is_visible()==Some(false)→读屏前隐藏——Gap 2 关闭）** | ✓（真实截图内容留待 UAT 11） |
| ↑/↓/Ctrl+P/Ctrl+N/Enter | 环绕导航、回车执行、执行中面板保持 | fuzzy_navigation_execute + ctrl_pn_navigation 实跑 PASS | ✓ |
| 中文输入（IME）——首次唤出 + 重唤出 | 输入框可输入中文并过滤 | ime_commit_updates_input 全 stage（含 03-09 重唤出扩展）实跑 PASS | ✓（OS 候选窗留待 UAT 10） |
| Outcome | 统一入口可用——快捷键唤出、键盘/鼠标全程操作、文字清晰、拼音关键词命中高亮、点击截图不含面板 | 233 单测 + 12/12 E2E + cargo check/build 全绿（本 verifier 实跑） | ✓ |

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| SC-1 | 用户按全局快捷键唤出命令面板浮窗（PAL-01） | ✓ VERIFIED（回归） | consecutive_summon_close 探针实跑 PASS |
| SC-2 | 面板列出截图模块注册的命令（PAL-02） | ✓ VERIFIED（回归） | glyph_shape 探针实跑 aa_spread=242 |
| SC-3 | 输入关键词可模糊过滤命令列表（PAL-03） | ✓ VERIFIED（回归+强化） | fuzzy_navigation_execute + keyword_highlight 实跑 PASS |
| SC-4 | 方向键选择命令，回车执行对应功能（PAL-04） | ✓ VERIFIED（回归+强化） | hover_click_alignment + ctrl_pn_navigation + click_hide_before_capture 实跑 PASS |
| SC-5 | 按 ESC 关闭命令面板（PAL-05） | ✓ VERIFIED（回归） | five_summon_esc_no_residue 实跑 PASS |
| 03-10-T1 | 输入「jt」/「jietu」时「开始截图」命中排前，命中的拼音 keyword 字符以 #FF6000 高亮 | ✓ VERIFIED（**03-10 关闭**） | filter.rs keyword_hit（jietu + indices [0,3] 单测断言）+ ui.rs keyword tag 渲染（KEYWORD_TAG_SEP + sep_len 推导——CR-01 修复）+ E2E jt 阶段 16 ACCENT px |
| 03-10-T2 | 全部拼音 keyword（jietu/tuichu/peizhi/chongqi/rizhi）命中路径同机制显示 keyword 文本 + #FF6000 高亮 | ✓ VERIFIED（**03-10 关闭**） | filter.rs `pinyin_keywords_all_carry_keyword_hit`（4 keyword 逐一断言 keyword+indices 非空）+ E2E tuichu 阶段 51 ACCENT px（行 1 带内精确 #FF6000） |
| 03-10-T3 | 鼠标点击执行「开始截图」时窗口在点击帧内同步隐藏（window.is_visible() == Some(false)），Destroy 排出前已从屏幕消失 | ✓ VERIFIED（**03-10 关闭**） | lib.rs L309-312 Hidden 守卫（egui_ctx.run 后、apply_textures 前）+ 探针 stage 3 断言 is_visible()==Some(false) + gated 读屏 counter==0 |
| 03-10-T4 | Enter 与鼠标点击两条路径时序一致：面板先关闭、不再出现 | ✓ VERIFIED（**03-10 关闭**） | 探针 stage 3 Hidden + Destroy 已入队 + stage 4 释放后 counter==1 且无二次 Destroy（finalize generation 守卫 no-op） |
| 03-10-T5 | CR-01 修复：keyword tag 字节偏移正确（KEYWORD_TAG_SEP + sep_len 推导），CJK keyword 不 panic | ✓ VERIFIED（**03-10 修复**） | commit 32561c8；ui.rs L485 const + L489 sep_len；单测偏移 4/7（L683/685）+ CJK 无 panic 测试（L701-722）实跑 PASS |

**Score:** 25/25 truths verified（5 roadmap SC + 14 前轮 gap-closure truths + 03-09 2 truths 回归 + 本轮 03-10 新增 4 truths + CR-01 修复 1 truth；Gap 1/Gap 2/CR-01 全分支成立）

### Deferred Items

| # | Item | Addressed In | Evidence |
|---|------|-------------|----------|
| 1 | Windows 热键 Send 化、字体发现、explorer 打开文件行为 | Phase 4 | Phase 4 SC-3 + goal（Windows 适配） |
| 2 | sync_window_geometry 负坐标 clamp（多显示器） | Phase 4 | Phase 4 goal「多显示器」 |
| 3 | runner panic 无 catch_unwind（IN-01）+ on_event/on_event_win 回调未包 catch_unwind（REVIEW WR-02） | Phase 4 | Phase 4 goal「错误处理打磨」+ plan 04-02 |
| 4 | 窗口创建失败永久卡死 pending_close（REVIEW WR-01） | Phase 4 | Phase 4 goal「错误处理打磨」+ plan 04-02；仅创建失败路径可达 |
| 5 | 零命令 fallback 块 144px 在 128px 窗口被裁切（REVIEW WR-03） | Phase 4 | 生产恒有 ≥4 内置命令不可达；latent |

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| crates/modules/palette/src/filter.rs | `Match.keyword_hit` + `KeywordHit`（keyword 字符串 + fuzzy_indices 字符索引）；keyword 梯队取最高分 | ✓ VERIFIED | L52-56 KeywordHit（自带 derive）；L44 keyword_hit 字段；L122-140 梯队分支 fuzzy_indices + best-score；L89/111/114 其余分支 None |
| crates/modules/palette/src/ui.rs | `keyword_tag_job` 渲染「 · {keyword}」+ 命中字符 ACCENT #FF6000；`draw_command_row` 接收 keyword_hit | ✓ VERIFIED | L487-512 keyword_tag_job；L485 KEYWORD_TAG_SEP；L386/434-443 渲染；L351 传参；L26 ACCENT=#FF6000；CR-01 修复落地 |
| crates/modules/palette/src/lib.rs | RedrawRequested 帧循环 Hidden 守卫 + `window.set_visible(false)` 同步隐藏早退 | ✓ VERIFIED | L297 egui_ctx.run → L309-312 守卫（Hidden 判定 + set_visible(false) + return）→ L314 apply_textures；守卫在 paint/present/request_redraw 之前 |
| crates/modules/palette/src/bin/palette_checks.rs | E2E 探针 `keyword_highlight` + `click_hide_before_capture` + main 分发 + usage | ✓ VERIFIED | L2095 check_keyword_highlight（jt→filtered[0] + tuichu→filtered[1] + ACCENT 像素断言）；L2299 check_click_hide_before_capture（基线 is_visible()==Some(true) + stage 3 is_visible()==Some(false)/counter==0/Destroy + stage 4 无二次 Destroy）；L2048 accent_pixels_in_row_band；L2518-2519 分发臂；L2522 usage |
| crates/modules/palette/tests/integration.rs | 测试 11/12（`palette_keyword_highlight` / `palette_click_hide_before_capture`，#[ignore] + run_check 接线） | ✓ VERIFIED | L176-180 + L191-195；本 verifier 桌面会话实跑 12/12 PASS |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|----|---------|---------|
| filter.rs keyword 梯队分支（fuzzy_indices） | Match.keyword_hit | `fuzzy_indices(kw, &query)` 返回 (score, indices)——取最高分 keyword 及字符索引存入 Match | ✓ WIRED | L122-140；单测断言 jietu/[0,3] + 4 拼音 keyword 全携带 |
| ui.rs draw_command_row | keyword tag LayoutJob（ACCENT 命中字符） | keyword_tag_job 对 indices 着色 #FF6000；「 · 」分隔符 TEXT_DIM（KEYWORD_TAG_SEP 推导字节） | ✓ WIRED | L434-443 合并进 desc_job；单测偏移 4/7 正确；E2E 16/51 ACCENT px |
| ui.rs resp.clicked() → execute → session.close() → Hidden | lib.rs 帧循环 Hidden 守卫 → window.set_visible(false) | egui_ctx.run 返回后检查 state==Hidden → 同步隐藏 + return（跳过 paint/present/request_redraw） | ✓ WIRED | L309-312；探针 stage 3 经真实闭包断言 is_visible()==Some(false) 先于 gated 读屏 |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|-------|--------------------|--------|
| Match.keyword_hit | keyword_hit: Option<KeywordHit> | `fuzzy_indices(kw, &query)` 遍历 cmd.keywords（Command 注册表静态数据） | Yes — 5 个拼音 keyword 全部携带索引（单测）；E2E 渲染路径实测 ACCENT 像素 | ✓ FLOWING（**03-10**） |
| keyword_tag_job 渲染 | desc_job 合并 sections | Match.keyword_hit → char_indices_to_byte_ranges → LayoutJob | Yes — 单测偏移 4/7 + CJK 段 4..10；E2E jt/tuichu 阶段帧缓冲实测 #FF6000 像素 | ✓ FLOWING（**03-10**） |
| Hidden 帧循环守卫 | session.state() | 点击帧内 execute→session.close() 置 Hidden | Yes — 探针 stage 3 断言 state==Hidden + is_visible()==Some(false) + 读屏 counter==0 | ✓ FLOWING（**03-10**） |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| 全 workspace 单测 | `cargo nextest run --workspace` | **233 passed / 0 failed**（20 skipped） | ✓ PASS |
| palette filter 单测 | `cargo nextest run -p mybox-palette filter::` | 10/10（含 keyword_hit 4 项新断言） | ✓ PASS |
| palette ui 单测 | `cargo nextest run -p mybox-palette ui::` | 8/8（含 keyword_tag_job 偏移 4/7 + CJK 无 panic） | ✓ PASS |
| 编译健康 | `cargo check --workspace` | exit 0，0 warning | ✓ PASS |
| E2E 二进制构建 | `cargo build -p mybox-palette --bin palette_checks` | exit 0 | ✓ PASS |
| E2E 集成测试（桌面会话，本 verifier 实跑） | `cargo test -p mybox-palette --test integration -- --ignored` | **12/12 PASS** in 3.67s（keyword_highlight: jt 16 px / tuichu 51 px；click_hide_before_capture OK） | ✓ PASS |

### Probe Execution

| Probe | Command | Result | Status |
|-------|---------|--------|--------|
| keyword_highlight（03-10 新增） | `cargo test -p mybox-palette --test integration -- --ignored` | jt 阶段 16 ACCENT px + tuichu 阶段 51 ACCENT px（行 1 带内），ESC→Destroy→Hidden 收尾 | ✓ PASS（**Gap 1 关闭**，本 verifier 实跑） |
| click_hide_before_capture（03-10 新增） | 同上 | stage 0 基线可见 → stage 3 Hidden + is_visible()==Some(false) + counter==0 + Destroy 入队 → stage 4 counter==1 无二次 Destroy | ✓ PASS（**Gap 2 关闭**，本 verifier 实跑） |
| 既有 10 探针（summon_render/fuzzy_navigation_execute/capture_hides_first/five_summon_esc_no_residue/consecutive_summon_close/glyph_shape/position_stable_on_filter/hover_click_alignment/ctrl_pn_navigation/ime_commit_updates_input） | 同上 | 10/10 PASS | ✓ PASS（回归） |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|---------|
| PAL-01 | 03-01 / 03-03 / 03-09 | 用户按全局快捷键唤出命令面板浮窗 | ✓ SATISFIED | REQUIREMENTS.md:39 `[x]` Complete；consecutive_summon_close + ime_commit_updates_input 实跑 |
| PAL-02 | 03-01 / 03-04 | 命令面板列出所有模块注册的命令 | ✓ SATISFIED | REQUIREMENTS.md:40 `[x]` Complete；glyph_shape aa_spread=242 实跑 |
| PAL-03 | 03-02 / 03-05 / 03-08 / 03-09 / **03-10** | 输入关键词模糊过滤命令列表 | ✓ SATISFIED | REQUIREMENTS.md:41 `[x]` Complete；fuzzy_navigation_execute + **keyword_highlight（keyword 梯队全路径高亮）** 实跑 |
| PAL-04 | 03-02 / 03-05 / 03-06 / 03-07 / **03-10** | 方向键导航选择，回车执行 | ✓ SATISFIED | REQUIREMENTS.md:42 `[x]` Complete；hover_click_alignment + ctrl_pn_navigation + **click_hide_before_capture（点击路径时序与 Enter 一致）** 实跑 |
| PAL-05 | 03-02 / **03-10** | ESC 关闭命令面板 | ✓ SATISFIED | REQUIREMENTS.md:43 `[x]` Complete；five_summon_esc_no_residue + click_hide_before_capture Destroy 配对实跑 |

**追踪表状态：** REQUIREMENTS.md 中 PAL-01..PAL-05 全部 `[x]` 且 traceability 表全部 Complete。10 张 PLAN 的 `requirements:` 字段合计认领覆盖全部 5 个 ID（03-10 新增认领 PAL-03/PAL-04 强化 keyword 梯队高亮 + 点击路径时序；PAL-05 经 03-10 探针补充覆盖）——**无 orphaned requirements**。

### Anti-Patterns Found

**本轮关闭：**

| ID | File | Line | Pattern | Severity | Status |
|----|------|------|---------|----------|--------|
| UAT gap 1 | filter.rs + ui.rs | filter L122-140 / ui L434-443, 487-512 | keyword 梯队命中无数据无渲染（fuzzy_match 仅计分 + keywords 从不显示） | ⚠️ Minor（UAT test 5） | **closed（03-10）**——keyword_hit 数据通道 + keyword tag 渲染 |
| UAT gap 2 | lib.rs | L309-312 | 点击路径 Destroy 排出延迟与截图读屏竞态（面板被拍进截图） | 🛑 Major（UAT test 11） | **closed（03-10）**——Hidden 守卫 set_visible(false) 同步隐藏 + 早退 |
| CR-01 | ui.rs | L485-512（修复后） | keyword_tag_job 分隔符硬编码 3 字节→ACCENT 区间左移 1 字节 + CJK 切片 panic | 🛑 Critical（03-REVIEW） | **closed（03-10, commit 32561c8）**——KEYWORD_TAG_SEP + sep_len 推导 + 单测偏移修正 4/7 + CJK 无 panic 测试 |

**仍开放（per 最新 03-REVIEW.md，均为 warning/info，无 blocker；已 deferred Phase 4）：**

| ID | File | Pattern | Severity | Impact / Disposition |
|----|------|---------|----------|---------------------|
| WR-01 | crates/mybox-core/src/app.rs | App::create_window 失败仅日志，session 永久 pending_close 卡死 | ⚠️ Warning | 仅创建失败路径可达；deferred Phase 4 plan 04-02 |
| WR-02 | crates/mybox-core/src/app.rs | on_event/on_event_win 模块回调未包 catch_unwind | ⚠️ Warning | Phase 4 错误处理打磨范围（CR-01 panic 面已消除——CJK keyword 不再 panic） |
| WR-03 | lib.rs + ui.rs | 零命令 fallback 块 144px 仍被 128px 窗口裁切 | ⚠️ Warning | 生产恒有 ≥4 内置命令不可达；latent |
| IN-01..IN-05 | — | run_command expect / 锁顺序 / realize_window 注释 / config_dir 降级 / 64 字符回弹 | ℹ️ Info | 均 deferred Phase 4 或文档级 |

**债务标记扫描：** 本轮 5 个被改文件内 `TBD|FIXME|XXX` **0 命中**；`placeholder` 命中仅 ui.rs L78 输入框占位文本注释（合法 UI 元素，非 stub）。无 stub/占位实现。

### Human Verification Required

以下 5 项无法在验证进程内程序化完成（OS 物理链路 / 视觉 / 真实截图 / 进程副作用）——其中 2 项为本轮 03-10 关闭项的最终复验：

### 1. UAT 5 重跑（keyword 高亮——03-10 关闭项）

**Test:** 输入 jt / jietu / tuichu / peizhi / chongqi / rizhi，观察命令行 description 行尾的「 · {keyword}」标签
**Expected:** 命中命令排前，且标签中命中的拼音字符（如 j/t）以 #FF6000 橙色高亮
**Why human:** E2E 探针已断言帧缓冲存在 ACCENT 像素（16/51 px），但命中字符最终着色位置（CR-01 修复后应为 j/t 而非 e）只能肉眼确认

### 2. UAT 11 重跑（鼠标点击截图时序——03-10 关闭项）

**Test:** 在面板中用鼠标点击「开始截图」
**Expected:** 面板先关闭、截图画面中绝不含面板本身（与 Enter 路径一致）
**Why human:** 探针经合成指针事件 + gated 模拟读屏断言 is_visible()==Some(false) 先于读屏；真实截图的屏幕内容只能人工查看

### 3. UAT 1 重跑（物理热键循环）

**Test:** 按 Cmd+Shift+Space 唤出→再按关闭→再唤出 ≥3 轮；执行「开始截图」后再唤出
**Expected:** 每次保持显示不闪退
**Why human:** 探针走 bus 级 summon，OS 热键注册→回调链路只能真人验证

### 4. UAT 4 重跑（内置命令 OS 副作用）

**Test:** 在面板中依次执行退出/重启/打开配置目录/打开日志
**Expected:** 各自 OS 副作用正确（进程生命周期/文件管理器打开正确位置）
**Why human:** 进程生命周期/文件管理器无法在验证进程内执行

### 5. UAT 10 重跑（真实输入法——首次唤出 AND ESC 关闭后重唤出）

**Test:** 首次唤出输入中文确认候选窗出现；ESC 关闭后重唤出再输入中文
**Expected:** 两次都能输入中文，候选窗两次都出现（03-09 GAP-8 代码层已关闭）
**Why human:** OS 候选窗出现/交互无法合成；探针仅注入合成 Ime 事件

### Gaps Summary

**本轮 0 个开放代码 gap。** UAT 的 2 个留存 issue + 03-REVIEW 的 CR-01 全部关闭：

- **UAT gap 1 关闭（keyword 梯队高亮，minor）**：filter.rs `Match.keyword_hit`/`KeywordHit` 数据通道（fuzzy_indices 取最高分 keyword + 字符索引）+ ui.rs 「 · {keyword}」行尾标签渲染（命中字符 #FF6000）。单测覆盖全部 5 个拼音 keyword 命中索引；E2E 探针 jt/tuichu 两阶段帧缓冲 ACCENT 像素实测。
- **UAT gap 2 关闭（点击截图时序，major）**：lib.rs Hidden 帧循环守卫在点击帧内 `set_visible(false)` 同步隐藏（Destroy 排出与截图读屏前已离屏）并跳过本帧 paint/present/request_redraw。E2E 探针断言 is_visible()==Some(false) + gated 读屏 counter==0 + 无二次 Destroy——Enter 与点击两路径时序收敛。
- **CR-01 关闭（03-REVIEW Critical）**：KEYWORD_TAG_SEP const + sep_len() 推导字节数，单测偏移修正（j→4、t→7）+ CJK 无 panic 锁定（commit 32561c8）。本 verifier 复核提交 diff 与代码现状一致。

**质量账本：** 233 单测 / 12 E2E 探针（12/12 桌面会话实跑）/ cargo check+build 全绿无 warning；REQUIREMENTS.md PAL-01..05 全 Complete；无 TBD/FIXME/XXX；10/10 plans 全部 `[x]`。仍开放 3 warning + 5 info（均非 blocker，已 deferred Phase 4 或 latent 不可达）。

**推进建议：** 自动化层面 Phase 3 已达成全部 5 条成功标准 + 25/25 truths，UAT 15 项中 13 项 pass + 2 项 issue 已代码层关闭并具备 E2E 回归。剩余 5 项人工 UAT（1/4/5/10/11）执行通过即可将本阶段状态从 `human_needed` 推进至 `passed`。

**说明：** 03-UAT.md / 03-HUMAN-UAT.md / .planning/debug/ 两个 debug 文件的 gap 状态与 fix 落地标记属 orchestrator 所有（03-10 SUMMARY 明示留待 orchestrator 更新），本 verifier 不修改。

---

_Verified: 2026-08-17T15:30:00Z_
_Verifier: the agent (gsd-verifier)_
