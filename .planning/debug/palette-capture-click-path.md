---
status: resolved
trigger: "UAT Test 11: 鼠标点击「开始截图」面板先关闭立马又出现，面板会被截图；Enter 路径正常"
created: 2026-08-17T00:00:00Z
updated: 2026-08-17T06:52:00Z
---

## Current Focus

hypothesis: 点击路径的 execute() 在 egui 帧内（RedrawRequested）调用，Destroy 排出被同帧 paint+present 工作延迟 2-10ms+，与 capture 工作线程的 xcap 屏幕读取（点击后 3-20ms）竞态重叠 → 面板被拍进截图
test: 对比 Enter 路径（on_palette_key 帧外早退，Destroy 微秒级排出）与点击路径（帧内 execute，Destroy 等帧完成才排出）
expecting: 已确认——两条路径的 Destroy 入队顺序相同（先 Destroy 后 run_command），但主线程到达 about_to_wait 的延迟不同：Enter 无帧工作（µs），点击路径有一整帧 paint+present（2-10ms+）
next_action: 输出诊断结论（find_root_cause_only，不修复）

## Symptoms
<!-- Written during gathering, then IMMUTABLE -->

expected: 面板执行「开始截图」时先 Destroy（hide_before_execute），截图覆盖层出现前面板已关闭，截图画面绝不含面板
actual: 键盘 Enter 路径正常（面板先关不再出现）；鼠标点击路径面板先关闭立马又出现，面板会被截图
errors: 无
reproduction: 鼠标点击命令项「开始截图」（Test 11）
started: 2026-08-17 UAT 期间发现

## Eliminated
<!-- APPEND only -->

- hypothesis: Destroy 未真正销毁 winit 窗口（Arc 未归零 / 窗口未关闭）
  evidence: 用户明确报告"面板先关闭"——Destroy 生效，窗口确实被关闭；WindowManager::destroy 移除 state 后 WindowState(含 window Arc 与 renderer surface Arc) 全部 drop，refcount 归零
  timestamp: 2026-08-17

- hypothesis: 点击触发了再次 summon（热键 toggle / 重入）
  evidence: 唯一 summon 路径是 hotkey toggle（lib.rs toggle_palette）；鼠标点击不经过热键；session 状态在 execute 后为 Hidden，无任何代码路径可重新 summon
  timestamp: 2026-08-17

## Evidence
<!-- APPEND only -->

- timestamp: 2026-08-17T00:00:00Z
  checked: ui.rs draw_command_row (L415-419)
  found: `resp.clicked()` → `execute::execute(...)` 在 `egui_ctx.run` 帧闭包内被调用（RedrawRequested 帧处理中）
  implication: 点击路径的 Destroy 入队发生在帧内，而非帧外事件处理器

- timestamp: 2026-08-17T00:00:00Z
  checked: lib.rs on_event_win RedrawRequested 帧循环 (L286-343)
  found: execute() 之后帧循环仍执行 apply_textures → tessellate → raster::paint（全量重绘面板）→ handle_platform_output → geometry 检查（set_executing 已 bump revision）→ sync_window_geometry（Hidden 早退，WR-04）→ `window.request_redraw()`（L341）
  implication: 点击帧在 execute() 后仍完整渲染并请求额外重绘——面板内容在 execute 后继续被绘制

- timestamp: 2026-08-17T00:00:00Z
  checked: app.rs window_event (L479-481) + about_to_wait (L515-543)
  found: RedrawRequested → on_event_win 后执行 handle_redraw（on_draw blit + present，面板上屏）；Destroy 只在 about_to_wait 排出（WindowManager::destroy 移除 state → Arc<Window> 归零 → winit 窗口关闭）
  implication: 点击帧的 present 先于 Destroy 排出；Destroy 从 execute() 到实际关闭被整帧工作延迟约 2-10ms+

- timestamp: 2026-08-17T00:00:00Z
  checked: lib.rs on_palette_key Enter 路径 (L249-269, L463-477)
  found: KeyboardInput Enter → on_palette_key → execute() → 返回 true → on_event_win `return` 早退（L267），该事件不跑帧循环；同一事件迭代的 about_to_wait 微秒级排出 Destroy
  implication: Enter 路径 Destroy 在 µs 内生效，远早于 capture 工作线程的屏幕读取

- timestamp: 2026-08-17T00:00:00Z
  checked: execute.rs (L44-54) + capture/lib.rs start_capture (L262-308) + capture.rs capture_all_monitors
  found: 两条路径 Destroy 均先于 run_command 入队（排队序正确）；但 capture 链（cmd 线程 → runner → start_capture → mybox-capture 线程 → xcap capture_image）并发推进，屏幕读取发生在点击后约 3-20ms
  implication: 点击路径下 Destroy 排出延迟（2-10ms+）与 capture 屏幕读取（3-20ms）窗口重叠 → 面板仍可见/未被窗口服务器移除时被 xcap 读入截图

- timestamp: 2026-08-17T00:00:00Z
  checked: capture/overlay.rs premultiply_dimmed_pixmap (L71-85) + create_overlays (L97-120)
  found: 覆盖层 base 层是"被调暗的真实截图"——截图含面板，用户就在覆盖层里看到面板"立马又出现"
  implication: 「面板先关闭立马又出现」= Destroy 生效（先关闭）+ 覆盖层显示含面板的调暗截图（视觉上又出现）

- timestamp: 2026-08-17T00:00:00Z
  checked: palette_checks.rs check_capture_hides_palette_first (L588-653)
  found: 探针是 headless——直接调 execute::execute，无真实窗口、无鼠标点击；只断言 Destroy 入队先于 runner 运行（队列顺序），不断言"销毁时刻早于截图时刻"的屏幕时序
  implication: 探针未覆盖点击路径的屏幕时序竞态，UAT Test 11 才暴露

## Resolution
<!-- OVERWRITE as understanding evolves -->

root_cause: "鼠标点击路径的 execute()（含 hide_before_execute 的 Destroy 入队）发生在 egui 帧内（RedrawRequested 的 egui_ctx.run 闭包中），点击帧在 execute() 后仍完成 raster::paint + present 整帧工作（且 L341 额外 request_redraw），主线程直到 about_to_wait 才排出 Destroy 关闭窗口——从 execute() 到窗口实际消失被延迟约 2-10ms+。与此同时 capture.start 的 runner 链（cmd 线程→start_capture→capture 线程→xcap capture_image）并发读取屏幕（点击后约 3-20ms），两个时间窗重叠，面板仍可见/未被窗口服务器从合成输出移除时被拍进截图。Enter 路径的 execute() 在 on_palette_key 内帧外调用，on_event_win 早退无帧工作，Destroy 在同事件迭代的 about_to_wait 微秒级排出，远早于 capture 屏幕读取，故从不被拍进截图。用户所见「面板先关闭立马又出现」= Destroy 确实生效，随后 capture 覆盖层显示被调暗的真实截图（含面板），视觉上像面板又出现。"
fix: "03-10 落地：lib.rs 帧循环 Hidden 守卫 window.set_visible(false) 同步隐藏 + execute.rs 队列序保持"
verification: ""
files_changed: []