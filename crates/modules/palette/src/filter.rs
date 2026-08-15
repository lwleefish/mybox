//! Fuzzy command filtering (PAL-03): pure scoring + highlight indices.
//!
//! No winit/egui surface — headless-testable like `capture::selection`. The
//! scoring primitive is `SkimMatcherV2` (via the `SkimMatcher` trait methods —
//! the top-level `fuzzy_match`/`fuzzy_indices` fns are deprecated since
//! 0.3.5). Ranking is three-tier: name matches beat description matches beat
//! keyword matches; within a tier, higher skim score first; ties keep
//! registration order (UI-SPEC lifecycle rule 4 — deterministic).

use mybox_core::command::Command;
use mybox_core::fuzzy_matcher::skim::SkimMatcherV2;
use mybox_core::fuzzy_matcher::FuzzyMatcher;

/// Maximum query length in characters (Security V5 — bound the matching cost
/// before the matcher ever sees the pattern; also T-3-03 DoS mitigation).
pub const MAX_QUERY_LEN: usize = 64;

/// Score offset applied to description-tier matches so every name match ranks
/// above every description match regardless of raw skim score.
pub const DESCRIPTION_TIER_OFFSET: i64 = 100_000;
/// Score offset for keyword-tier matches (below the description tier).
pub const KEYWORD_TIER_OFFSET: i64 = 200_000;

/// One ranked match: the command index into the snapshotted list, its score,
/// and the **char-position** highlight indices for name and description
/// (`fuzzy_indices` returns char positions — the UI layer converts them to
/// byte ranges for the `LayoutJob`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub cmd_index: usize,
    /// Skim score with the tier offset applied (sort key).
    pub score: i64,
    /// Char positions of the query hits in `Command::name` (empty = none).
    pub name_indices: Vec<usize>,
    /// Char positions of the query hits in `Command::description` (empty = none).
    pub description_indices: Vec<usize>,
}

/// The tier a match belongs to, derived from its index fields (a name hit may
/// also carry description indices — the name tier takes precedence).
fn tier_of(m: &Match) -> u8 {
    if !m.name_indices.is_empty() {
        0
    } else if !m.description_indices.is_empty() {
        1
    } else {
        2
    }
}

/// Rank `cmds` against `query`.
///
/// Empty (trimmed) query → every command in registration order with no
/// highlight indices (Idle semantics). Otherwise the query is truncated to
/// [`MAX_QUERY_LEN`] chars and each command is scored: name tier first
/// (`fuzzy_indices` on the name), then description tier, then the best
/// keyword score; commands with no hit anywhere are excluded. Sorting is
/// tier asc → score desc → registration order (stable/deterministic).
pub fn filter_commands(cmds: &[Command], query: &str) -> Vec<Match> {
    let query = query.trim();
    if query.is_empty() {
        return cmds
            .iter()
            .enumerate()
            .map(|(i, _)| Match {
                cmd_index: i,
                score: 0,
                name_indices: Vec::new(),
                description_indices: Vec::new(),
            })
            .collect();
    }
    let query: String = query.chars().take(MAX_QUERY_LEN).collect();
    let matcher = SkimMatcherV2::default().smart_case();

    let mut matches: Vec<Match> = Vec::new();
    for (cmd_index, cmd) in cmds.iter().enumerate() {
        let name_hit = matcher.fuzzy_indices(&cmd.name, &query);
        let description_hit = matcher.fuzzy_indices(&cmd.description, &query);
        // The tier is decided by priority order; indices for both fields are
        // computed independently so a name-tier match still highlights query
        // hits inside the description (D-10).
        let (score, name_indices, description_indices) = if let Some((s, inds)) = name_hit {
            (
                s,
                inds,
                description_hit
                    .map(|(_, inds)| inds)
                    .unwrap_or_default(),
            )
        } else if let Some((s, inds)) = description_hit {
            (s - DESCRIPTION_TIER_OFFSET, Vec::new(), inds)
        } else {
            match cmd
                .keywords
                .iter()
                .filter_map(|kw| matcher.fuzzy_match(kw, &query))
                .max()
            {
                Some(s) => (s - KEYWORD_TIER_OFFSET, Vec::new(), Vec::new()),
                None => continue, // no hit anywhere — excluded
            }
        };
        matches.push(Match {
            cmd_index,
            score,
            name_indices,
            description_indices,
        });
    }

    matches.sort_by(|a, b| {
        tier_of(a)
            .cmp(&tier_of(b))
            .then_with(|| b.score.cmp(&a.score))
            .then_with(|| a.cmd_index.cmp(&b.cmd_index))
    });
    matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn cmd(id: &'static str, name: &str, description: &str, keywords: &[&'static str]) -> Command {
        Command {
            id,
            name: name.to_string(),
            description: description.to_string(),
            keywords: keywords.to_vec(),
            hide_before_execute: false,
            runner: Arc::new(|| Box::pin(async { Ok(()) })),
        }
    }

    /// The Phase 3 command inventory (UI-SPEC): capture.start with the pinyin
    /// keyword "jietu" (Pitfall 6 data fix) + four builtins.
    fn inventory() -> Vec<Command> {
        vec![
            cmd(
                "capture.start",
                "开始截图",
                "截取屏幕区域并复制/保存",
                &["截图", "capture", "screen", "jietu"],
            ),
            cmd("builtin.quit", "退出应用", "退出 mybox 应用", &["退出", "quit", "exit"]),
            cmd(
                "builtin.open_config",
                "打开配置目录",
                "在文件管理器中打开 mybox 配置目录",
                &["配置", "config"],
            ),
            cmd("builtin.restart", "重启应用", "重启 mybox 应用", &["重启", "restart"]),
            cmd(
                "builtin.open_log",
                "打开日志文件",
                "打开 mybox 运行日志",
                &["日志", "log"],
            ),
        ]
    }

    #[test]
    fn query_jietu_hits_capture_via_pinyin_keyword() {
        // Pitfall 6 data path: "jt" is a subsequence of the pinyin keyword
        // "jietu" — the capture command must match without pinyin conversion.
        let matches = filter_commands(&inventory(), "jt");
        assert!(!matches.is_empty(), "jt must hit via the jietu keyword");
        assert_eq!(matches[0].cmd_index, 0, "jt must rank capture.start first");
    }

    #[test]
    fn query_screenshot_ranks_capture_first_and_indices_correct() {
        let matches = filter_commands(&inventory(), "截图");
        assert!(!matches.is_empty(), "截图 must match");
        assert_eq!(matches[0].cmd_index, 0, "截图 must rank capture.start first");
        assert_eq!(matches[0].name_indices, vec![2, 3], "name hit chars 2,3 (截图)");
        // The description "截取屏幕区域并复制/保存" contains no 图 — the name
        // tier hit carries no description highlight.
        assert!(
            matches[0].description_indices.is_empty(),
            "no 图 in the description — indices stay empty"
        );
    }

    #[test]
    fn no_match_returns_empty() {
        assert!(filter_commands(&inventory(), "zzzz不存在").is_empty());
    }

    #[test]
    fn empty_query_returns_all_in_order() {
        let matches = filter_commands(&inventory(), "   ");
        let indices: Vec<usize> = matches.iter().map(|m| m.cmd_index).collect();
        assert_eq!(indices, vec![0, 1, 2, 3, 4], "empty query keeps registration order");
        assert!(matches.iter().all(|m| m.name_indices.is_empty() && m.description_indices.is_empty()));
    }

    #[test]
    fn tie_break_keeps_registration_order() {
        // Two commands that score identically on the same query keep their
        // registration order (lifecycle rule 4 — deterministic).
        let cmds = vec![
            cmd("a", "alpha one", "d", &[]),
            cmd("b", "alpha two", "d", &[]),
        ];
        let matches = filter_commands(&cmds, "alpha");
        let indices: Vec<usize> = matches.iter().map(|m| m.cmd_index).collect();
        assert_eq!(indices, vec![0, 1], "tie must keep registration order");
        assert_eq!(matches[0].score, matches[1].score, "identical pattern/choice length scores equal");
    }

    #[test]
    fn query_over_64_chars_is_truncated() {
        // Security V5: the pattern the matcher sees is capped at 64 chars —
        // a 200-char query behaves exactly like its 64-char prefix.
        let cmds = vec![cmd("a", &"ji".repeat(50), "d", &[])];
        let long: String = "ji".repeat(100); // 200 chars
        let truncated: String = long.chars().take(MAX_QUERY_LEN).collect(); // 64 chars
        let a = filter_commands(&cmds, &long);
        let b = filter_commands(&cmds, &truncated);
        assert!(!a.is_empty(), "truncated pattern must still match");
        assert_eq!(a, b, "overlong query must be truncated before matching");
    }

    #[test]
    fn name_tier_beats_keyword_tier() {
        // cmd[1] matches "jietu" by name (tier 0); cmd[0] matches only via the
        // "jietu" keyword (tier 2) — the name tier must rank first even though
        // the keyword is an exact full-string match.
        let cmds = vec![
            cmd("a", "something else", "d", &["jietu"]),
            cmd("b", "jietu tool", "d", &[]),
        ];
        let matches = filter_commands(&cmds, "jietu");
        let indices: Vec<usize> = matches.iter().map(|m| m.cmd_index).collect();
        assert_eq!(indices, vec![1, 0], "name tier must beat keyword tier");
    }
}
