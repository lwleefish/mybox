---
status: partial
phase: 03-命令面板
source: [03-VERIFICATION.md]
started: 2026-08-15T00:00:00Z
updated: 2026-08-15T07:00:00Z
---

## Current Test

[awaiting human testing]

## Tests

### 1. 真实桌面热键唤出
expected: 运行 mybox-app 后按 Cmd+Shift+Space，当前活动显示器中央出现深色无边框置顶浮窗，输入框自动获得焦点，列出 ≥5 个命令（4 内置 + 1 截图）；连续 3 次唤出/关闭循环均正常，无闪退
result: [pending] — 用户未报告闪退复发（后续交互正常），但连续唤出循环未明确复验

### 2. 过滤/导航走查
expected: 按 manual_checklist.md（crates/modules/palette/tests/manual_checklist.md）步骤 3/4/6 走查——「截图」/「jt」过滤命中且高亮正确、↑/↓ 环绕导航、ESC 关闭且不执行命令
result: [pending]

### 3. 截图时序硬约束
expected: 在面板中执行「开始截图」——面板在截图覆盖层出现前已关闭，截图画面中绝不含面板本身
result: [pending]

### 4. 四个内置命令副作用
expected: 在面板中依次执行退出/重启/打开配置目录/打开日志
result: [pending]

### 5. 视觉走查（GAP-2 复验）
expected: 中文命令名 CJK 字形正常渲染（无豆腐块/灰色块）
result: passed — 用户可识别中文命令项（问题 5 基于可读的中文项提出），文字渲染正常；圆角未确认

### 6. 面板位置稳定性
expected: 唤出面板后位置固定；输入过滤改变命令项数量时面板不位移
result: failed — 输入后命令项减少导致面板下降（窗口尺寸随内容收缩并重新居中，位置漂移）

### 7. Hover 高亮对齐
expected: 鼠标移入命令项时高亮块与实际文字区域精确重叠
result: failed — hover 高亮块相对文字区域偏上，两者不重叠

### 8. 鼠标点击命令项
expected: 点击命令项执行对应命令
result: failed — 点击无反应（疑似与 #7 布局偏移同源：命中区域与视觉区域错位）

### 9. Ctrl+P / Ctrl+N 键盘选择
expected: ↑/↓ 之外的常用导航键（Ctrl+P/Ctrl+N）可选择命令
result: failed — 仅 ↑/↓ 生效，Ctrl+P/Ctrl+N 无反应

### 10. 中文输入与命令前缀发现
expected: 输入框可输入中文（IME），或命令提供可输入的拼音/前缀别名，用户能发现并输入中文命令
result: failed — 命令项全中文但输入框不能输入中文，用户无从得知命令前缀

## Summary

total: 10
passed: 1
issues: 5
pending: 4
skipped: 0
blocked: 0

## Gaps

- [x] GAP-1 热键重复唤出闪退 — 03-03 已关闭（Pressed 守卫 + on_created 配对），连续唤出循环待人工最终复验
- [x] GAP-2 文字灰色块 — 03-04 已关闭（UV 判别 + 原位图集补丁），文字可识别
- [ ] GAP-3 面板位置漂移：输入过滤导致窗口收缩+重新居中，面板下降
- [ ] GAP-4 hover 高亮与文字区域错位（高亮偏上）
- [ ] GAP-5 鼠标点击命令项无反应
- [ ] GAP-6 Ctrl+P/Ctrl+N 导航快捷键缺失
- [ ] GAP-7 中文输入不可用 + 中文命令前缀不可发现（IME 未接入输入框 / 缺少拼音别名）
