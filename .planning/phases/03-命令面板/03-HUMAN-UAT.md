---
status: partial
phase: 03-命令面板
source: [03-VERIFICATION.md]
started: 2026-08-15T00:00:00Z
updated: 2026-08-15T00:00:00Z
---

## Current Test

[awaiting human testing]

## Tests

### 1. 真实桌面热键唤出
expected: 运行 mybox-app 后按 Cmd+Shift+Space，当前活动显示器中央出现深色无边框置顶浮窗，输入框自动获得焦点，列出 ≥5 个命令（4 内置 + 1 截图）
result: failed — 第一次能唤出窗口，之后再次唤出窗口一闪而过（立即关闭）

### 2. 过滤/导航走查
expected: 按 manual_checklist.md（crates/modules/palette/tests/manual_checklist.md）步骤 3/4/6 走查——「截图」/「jt」过滤命中且高亮正确、↑/↓ 环绕导航、ESC 关闭且不执行命令
result: [pending]

### 3. 截图时序硬约束
expected: 在面板中执行「开始截图」——面板在截图覆盖层出现前已关闭，截图画面中绝不含面板本身（REVIEW WR-04 race 注意事项：真实环境确认）
result: [pending]

### 4. 四个内置命令副作用
expected: 在面板中依次执行退出/重启/打开配置目录/打开日志——退出应用正常、重启拉起新实例、文件管理器打开正确路径、日志文件存在
result: [pending]

### 5. 视觉走查
expected: 面板四角 12px 圆角生效（无黑色方块）、中文命令名 CJK 字形正常渲染（无豆腐块）
result: failed — 窗口内文字全是灰色块形状，无法识别（字形渲染失败，疑似字体/纹理问题）

## Summary

total: 5
passed: 0
issues: 2
pending: 3
skipped: 0
blocked: 0

## Gaps

- [ ] 热键重复唤出：第一次唤出正常，第二次及之后面板一闪而过（立即关闭）——疑似建销生命周期/pending_close 配对缺陷
- [ ] 文字渲染：窗口内所有文字为灰色块（豆腐块/字形缺失）——疑似字体安装或纹理路径缺陷
