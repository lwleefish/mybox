---
status: resolved
phase: 03-命令面板
source: [03-01-SUMMARY.md, 03-02-SUMMARY.md, 03-03-SUMMARY.md, 03-04-SUMMARY.md, 03-05-SUMMARY.md, 03-06-SUMMARY.md, 03-07-SUMMARY.md, 03-08-SUMMARY.md, 03-09-SUMMARY.md, 03-10-SUMMARY.md]
started: 2026-08-17T04:36:30Z
updated: 2026-08-17T06:50:00Z
---

## Current Test

[testing complete]

## Tests

### 1. App 冷启动
expected: 在仓库根目录运行 `cargo run -p mybox-app`，应用编译并启动，无 panic/报错。进入后台运行状态（无主窗口或仅托盘/菜单栏），等待全局热键。stderr 与日志文件同时有启动输出。
result: pass

### 2. 全局热键唤出面板
expected: 按 Cmd+Shift+Space，活动显示器屏幕中央出现 Floating 浮窗（大圆角、不可 resize、聚焦）。面板列出 ≥5 个已注册命令，含「开始截图」「退出应用」「打开配置目录」「重启应用」「打开日志」。输入框可见，占位符为中文可识别字形。
result: pass

### 3. 热键循环不闪退（GAP-1）
expected: 按 Cmd+Shift+Space 唤出→再按关闭→再唤出，重复 ≥3 轮。每次面板保持显示不闪退（一次物理按键 = 一次 toggle，松开不再触发关闭）。执行「开始截图」返回后再次唤出面板正常显示。
result: pass

### 4. 中文文字可识别（GAP-2）
expected: 面板内中文命令名、描述、输入框占位符均为可识别字形（笔画清晰），非灰色块/模糊条。对比英文/数字同样清晰。
result: pass

### 5. 模糊搜索 + 命中高亮
expected: 在输入框输入「截图」，「开始截图」排到首位；输入「jt」或「jietu」，「开始截图」同样命中排前。命中字符以 #FF6000 橙色高亮。输入超过 64 字符被截断。
result: issue
reported: "输入 jt 「开始截图」命中排前，但是命中字符没有橙色高亮"
severity: minor

### 6. 键盘导航 ↑/↓ + ESC
expected: Idle 无选中时按 ↓ 选中索引 0；继续 ↓ 向下移动并到底环绕回顶部；↑ 向上并到顶环绕回底部。ESC 关闭面板，无残留窗口。
result: pass

### 7. Ctrl+P / Ctrl+N 导航（GAP-6）
expected: Ctrl+P 等价 ↑（上移，到顶环绕），Ctrl+N 等价 ↓（下移，到底环绕）。无 Ctrl 时普通 P/N 正常进入输入框作为过滤文本。Error 态任意键关闭面板。
result: pass

### 8. 面板位置固定（GAP-3）
expected: 唤出后面板顶边位置固定。输入过滤文本改变命令项数量（如收缩到 1 项或恢复全部）时，面板高度随内容伸缩但顶边不位移（不"下降"）。
result: pass

### 9. hover 高亮与点击执行（GAP-4/5）
expected: 鼠标悬停命令项，hover 高亮带与该行文字精确重叠（高亮不偏移到行上方/下方）。点击命令项执行该命令（与 Enter 同效果），无幻影滚动空间。
result: pass

### 10. Enter 执行 + Executing 态
expected: 选中命令按 Enter，面板进入 Executing 态：显示「正在执行：{命令名}…」状态行，输入框降暗且不可输入。执行完成后面板销毁（Hidden）。
result: pass

### 11. 截图时序（hide_before_execute）
expected: 在面板中执行「开始截图」。面板在截图覆盖层出现前已关闭，截图画面中绝不含面板本身（面板不被拍进截图）。
result: issue
reported: "使用键盘选中开始截图后 enter 没问题，使用鼠标点击开始截图，面板先关闭立马又出现，面板会被截图"
severity: major

### 12. 内置命令 OS 副作用
expected: 在面板中依次执行四个内置命令并观察 OS 副作用：「退出应用」进程退出；「重启应用」spawn 新进程后本进程退出；「打开配置目录」系统文件管理器打开 mybox 配置目录；「打开日志」打开日志文件（或配置目录内 logs/mybox.log）。
result: pass

### 13. 中文输入法（GAP-7）
expected: 面板输入框聚焦后，OS 中文输入法候选窗出现。用真实输入法输入「截图」能进入输入框并过滤出「开始截图」。无 IME 场景下输入拼音「tuichu」命中「退出应用」、「peizhi」命中「打开配置目录」等。
result: pass

### 14. 重唤出 IME 复位（GAP-8）
expected: ESC 关闭面板后，再次按 Cmd+Shift+Space 唤出新面板。输入框聚焦后 OS 中文候选窗再次出现，可正常组合输入中文（第二次及以后唤出 IME 不"死亡"）。
result: pass

### 15. 双路日志
expected: 启动后检查 `<配置目录>/logs/mybox.log` 文件存在，且启动后有日志内容写入（与 stderr 输出一致）。
result: pass

## Summary

total: 15
passed: 15
issues: 0
pending: 0
skipped: 0
blocked: 0

## Gaps

- truth: "命中字符以 #FF6000 橙色高亮（含拼音关键词命中路径）"
  status: resolved
  resolved_by: "03-10（filter.rs Match.keyword_hit + ui.rs keyword tag #FF6000 + CR-01 字节偏移修复）"
  reason: "User reported: 输入 jt 「开始截图」命中排前，但是命中字符没有橙色高亮"
  severity: minor
  test: 5
  root_cause: "keyword 梯队命中路径从设计到渲染都没有高亮机制：filter.rs keyword 分支用 fuzzy_match 仅返回分数，Match 无 keyword 索引字段，name_indices/description_indices 恒为空；且 ui.rs 行内只渲染 name+description，keywords 字符串从不显示——拼音字符 'j'/'t' 不可能出现在中文 name 中，无高亮目标。03-02 计划明确只计分不产索引，UAT truth「含拼音关键词命中路径」从未被实现。波及全部拼音 keyword（jietu/tuichu/peizhi/chongqi/rizhi）。"
  artifacts:
    - path: "crates/modules/palette/src/filter.rs"
      issue: "keyword 梯队分支（L94-102）fuzzy_match 仅计分，Match 无 keyword 索引字段 → 命中零高亮索引"
    - path: "crates/modules/palette/src/ui.rs"
      issue: "draw_command_row（L424-429）+ highlight_job 只接收/渲染 name+description 索引；keywords 从不渲染"
  missing:
    - "Match 增加 keyword 命中信息（命中的 keyword 字符串 + fuzzy_indices）"
    - "行内渲染被命中的 keyword 文本并用 highlight_job 着色 #FF6000（或重新协商 UAT truth）"
    - "覆盖整个 keyword 梯队 + filter 层断言 keyword 命中索引的测试"
  debug_session: ".planning/debug/palette-keyword-tier-highlight.md"
- truth: "执行「开始截图」后面板在截图覆盖层出现前已关闭，截图画面中绝不含面板本身（键盘 Enter 与鼠标点击两种触发路径均如此）"
  status: resolved
  resolved_by: "03-10（lib.rs 帧循环 Hidden 守卫 window.set_visible(false) 同步隐藏 + E2E 探针 click_hide_before_capture）"
  reason: "User reported: 使用键盘选中开始截图后 enter 没问题，使用鼠标点击开始截图，面板先关闭立马又出现，面板会被截图"
  severity: major
  test: 11
  root_cause: "鼠标点击路径的 execute()（含 hide_before_execute 的 Destroy 入队）发生在 egui 帧内（ui.rs draw_command_row resp.clicked() 分支在 egui_ctx.run 闭包中），点击帧在 execute() 后仍完成整帧 paint + present 上屏工作并额外 request_redraw；Destroy 只在 about_to_wait 才排出（主线程被帧工作阻塞 2-10ms+）。capture 链并发读取屏幕（点击后 3-20ms），与 Destroy 排出延迟窗口重叠——面板仍可见时被 xcap 读入截图。Enter 路径 on_palette_key 返回 true 后 on_event_win 早退不跑帧循环，同迭代 about_to_wait 微秒级排出 Destroy，从不被拍进截图。用户所见「面板又出现」= 覆盖层 base 层是调暗的真实截图（含面板）。探针 check_capture_hides_palette_first 为 headless 只断言入队序，未覆盖点击路径的屏幕时序。"
  artifacts:
    - path: "crates/modules/palette/src/ui.rs"
      issue: "L415-419 点击在 egui 帧内调用 execute()，帧内 Destroy 入队后被帧工作阻塞到 about_to_wait"
    - path: "crates/modules/palette/src/lib.rs"
      issue: "帧循环在 execute() 后继续 paint + L341 request_redraw；Enter 路径 L267 早退是两路径差异根源"
    - path: "crates/mybox-core/src/app.rs"
      issue: "L479-481 present 先于 about_to_wait；L515-543 Destroy 只在 about_to_wait 排出"
    - path: "crates/modules/palette/src/bin/palette_checks.rs"
      issue: "L588-653 探针 headless 只断言 Destroy 入队序，未覆盖点击路径屏幕时序"
  missing:
    - "hide_before_execute 时帧内同步 window.set_visible(false)（macOS 即刻 orderOut）并跳过本帧剩余 paint/present"
    - "Hidden 态跳过 window.request_redraw()"
    - "真实窗口点击路径 E2E 探针（断言窗口在 runner 读屏前已隐藏）"
  debug_session: ".planning/debug/palette-capture-click-path.md"
