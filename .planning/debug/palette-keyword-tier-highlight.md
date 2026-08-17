---
status: resolved
trigger: "UAT Test 5 (03-UAT.md): 输入 jt「开始截图」命中排前但命中字符没有橙色高亮（拼音关键词命中路径无高亮）"
created: 2026-08-17T05:30:00Z
updated: 2026-08-17T06:52:00Z
---

## Current Focus

hypothesis: keyword 梯队命中（fuzzy_match 仅返回分数）不产生任何高亮索引，且 UI 只渲染 name/description、keywords 字符串从不渲染——拼音关键词命中路径在数据与渲染两层都没有高亮机制
test: 临时诊断测试打印 "jt" 查询 capture.start 的 Match 字段 + fuzzy_indices 在 name/description 上的结果（已执行并回滚）
expecting: name_indices/description_indices 均为空、tier=2；fuzzy_indices(name/desc, "jt") 均为 None —— 证实无索引且无可渲染目标
next_action: 完成——返回 ROOT CAUSE FOUND 诊断

## Symptoms

expected: 输入「jt」或「jietu」，「开始截图」命中排前，命中字符以 #FF6000 橙色高亮（UAT 5）
actual: 输入 jt「开始截图」命中排前（过滤与排名正常），但命中字符没有橙色高亮
errors: None（功能正常，仅视觉高亮缺失）
reproduction: 唤出面板 → 输入框输入 jt（或 jietu）→ 观察「开始截图」行
started: UAT 阶段发现（2026-08-17）

## Eliminated

- hypothesis: 高亮索引超出 name 字节范围导致 char_indices_to_byte_ranges 跳过
  evidence: 实测 fuzzy_indices("开始截图", "jt") = None、fuzzy_indices(description, "jt") = None——name/description 根本不含查询字符，谈不上"越界跳过"；根因在更上游：keyword 梯队根本不计算索引
  timestamp: 2026-08-17T05:30:00Z
- hypothesis: 高亮计算逻辑本身有 bug（字节/字符位置转换）
  evidence: char_indices_to_byte_ranges 有 4 个单测覆盖（含越界跳过），且「截图」查询（name 命中）高亮正常——转换层无问题
  timestamp: 2026-08-17T05:30:00Z

## Evidence

- timestamp: 2026-08-17T05:20:00Z
  checked: crates/modules/palette/src/filter.rs:94-102（keyword 梯队分支）
  found: 关键字命中用 `matcher.fuzzy_match(kw, &query)`（仅返回 Option<i64> 分数），Match 构造为 `(s - KEYWORD_TIER_OFFSET, Vec::new(), Vec::new())`——name_indices/description_indices 恒为空
  implication: keyword 梯队命中永远不带高亮索引；Match 结构体根本没有 keyword 索引字段
- timestamp: 2026-08-17T05:21:00Z
  checked: 临时诊断测试实测（已回滚）
  found: `DIAG cmd_index=0 score=-199958 name_indices=[] description_indices=[] tier=2`；`fuzzy_indices('开始截图','jt')=None`；`fuzzy_indices(desc,'jt')=None`
  implication: 过滤/排名正常（tier=2 排首位，与用户报告一致），但索引为空；且查询字符在 name/description 中不存在——即使传入索引也没有可高亮的字符
- timestamp: 2026-08-17T05:22:00Z
  checked: crates/modules/palette/src/ui.rs draw_command_row:424-429 + highlight_job:436-455 + draw:343-351
  found: UI 仅渲染 cmd.name（TEXT）+ cmd.description（TEXT_DIM），仅把 name_indices/description_indices 传给 highlight_job；keywords 字段从不渲染。空索引 → highlight_job 生成单段 base 色 LayoutJob，无 #FF6000
  implication: UI 层对 keyword 命中无渲染目标——「jietu」字符串本身不出现在行里
- timestamp: 2026-08-17T05:23:00Z
  checked: .planning/phases/03-命令面板/03-02-PLAN.md 行 122（设计契约）
  found: 设计明文规定「keywords 取 fuzzy_match 最高分为第三梯队」、`name_indices`/`description_indices` 取各自 fuzzy_indices 的 Vec（无命中为空）——keyword 梯队只算分数，从设计之初就不产出高亮索引
  implication: 非回归引入，是 03-02 计划时的既有设计选择；UAT gap truth「含拼音关键词命中路径」从未被实现
- timestamp: 2026-08-17T05:24:00Z
  checked: .planning/phases/03-命令面板/03-RESEARCH.md 行 464-469
  found: RESEARCH 示例 `fuzzy_indices("开始截图", "jt") = None`，keyword 命中 `fuzzy_match("jietu","jt") = Some(score)` 仅用于排序
  implication: 设计文档确认 keyword 命中路径只有分数、无索引、无高亮机制
- timestamp: 2026-08-17T05:25:00Z
  checked: crates/mybox-core/src/command.rs 内置命令 keywords（tuichu/peizhi/chongqi/rizhi）
  found: 全部内置命令的拼音 keyword 命中（UAT 13 场景：tuichu→退出应用 等）同属 keyword 梯队，同样无高亮
  implication: 缺陷是 keyword 梯队通病，不止 capture.start——修复须覆盖整个梯队

## Resolution

root_cause: 双层缺失。(1) 数据层：filter.rs 的 keyword 梯队命中用 fuzzy_match（只返回分数），构造的 Match 恒为空的 name_indices/description_indices，且 Match 无 keyword 索引字段——keyword 命中（含拼音 "jt"/"jietu" 命中 "jietu"）不产生任何高亮索引；(2) 渲染层：ui.rs 行只渲染 name+description，keywords 字符串从不显示，查询字符 "j"/"t" 在 name/description 中根本不存在（实测 fuzzy_indices 均为 None），即使传入索引也无处高亮。该设计源自 03-02-PLAN（keyword 梯队仅 fuzzy_match 计分），UAT truth「含拼音关键词命中路径」从未被实现。过滤/排名正常（tier=2 排首位）。
fix: "03-10 落地：filter.rs Match.keyword_hit + ui.rs keyword tag #FF6000（CR-01 字节偏移已修复）"
verification: 临时诊断测试实测：jt 查询 Match{cmd_index:0, score:-199958, name_indices:[], description_indices:[], tier:2}；fuzzy_indices("开始截图"/desc, "jt") 均 None
files_changed: []（filter.rs 临时测试已回滚，工作树干净）
