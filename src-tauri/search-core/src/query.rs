//! Query analysis, store query candidates and relevance scoring.

use crate::normalize::{compact_key, display, is_cjk_or_kana, is_hangul, match_key};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Language {
    Korean,
    Japanese,
    English,
}

#[derive(Debug, Clone)]
pub struct Alias {
    pub korean: Vec<String>,
    pub japanese: Vec<String>,
    pub english: Vec<String>,
    pub abbreviations: Vec<String>,
    /// Optional words such as manufacturers that product names often omit.
    pub soft: bool,
}

impl Alias {
    fn full_terms(&self) -> impl Iterator<Item = &String> {
        self.korean.iter().chain(&self.japanese).chain(&self.english)
    }
    fn all_terms(&self) -> impl Iterator<Item = &String> {
        self.full_terms().chain(&self.abbreviations)
    }
    fn first(&self, language: Language) -> Option<&String> {
        match language {
            Language::Korean => self.korean.first(),
            Language::Japanese => self.japanese.first(),
            Language::English => self.english.first(),
        }
    }
}

pub fn parse_aliases(source: &str) -> Vec<Alias> {
    source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let (soft, line) = match line.strip_prefix('~') {
                Some(rest) => (true, rest),
                None => (false, line),
            };
            let columns: Vec<Vec<String>> = line
                .split('|')
                .map(|col| col.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect())
                .collect();
            let column = |i: usize| columns.get(i).cloned().unwrap_or_default();
            let alias = Alias { korean: column(0), japanese: column(1), english: column(2), abbreviations: column(3), soft };
            (!alias.korean.is_empty()).then_some(alias)
        })
        .collect()
}

pub fn aliases() -> &'static [Alias] {
    static ALIASES: OnceLock<Vec<Alias>> = OnceLock::new();
    ALIASES.get_or_init(|| parse_aliases(include_str!("../data/aliases.txt")))
}

const SOFT_WORDS: &[&str] = &[
    "피규어", "figure", "フィギュア", "정품", "신품", "새상품", "미개봉", "중고", "국내", "정발", "판매",
    "구매", "재판", "재판매", "한정판", "예약", "특전", "굿즈", "상품", "버전", "ver", "version",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum TermKind {
    /// Must appear in the product name.
    Core,
    /// Product numbers, scales and JAN codes: always required.
    Identifier,
    /// Nice to have; never required.
    Soft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MatchRule {
    /// Digits / scales: occurrence not touching other digits.
    Number,
    /// ASCII words: occurrence not touching other ASCII letters/digits.
    AsciiBoundary,
    /// Kana, kanji or longer Hangul: substring of the space-free name.
    Contains,
    /// Short Hangul typed by the user: whole token, or a token prefix/suffix.
    TokenAffix,
    /// Short alias abbreviations: whole token only.
    TokenExact,
    /// Single Hangul syllables: whole token or token suffix ("바니걸" has "걸").
    TokenSuffix,
}

#[derive(Debug, Clone)]
struct Alternative {
    key: String,
    compact: String,
    rule: MatchRule,
}

#[derive(Debug, Clone)]
pub struct TermGroup {
    pub text: String,
    pub kind: TermKind,
    alias: Option<usize>,
    alternatives: Vec<Alternative>,
}

#[derive(Debug, Clone)]
pub struct Analysis {
    pub original: String,
    pub display: String,
    pub groups: Vec<TermGroup>,
}

impl Analysis {
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }
    pub fn identifiers(&self) -> impl Iterator<Item = &TermGroup> {
        self.groups.iter().filter(|g| g.kind == TermKind::Identifier)
    }
}

fn valid_jan(digits: &str) -> bool {
    if ![8, 12, 13, 14].contains(&digits.len()) {
        return false;
    }
    let values: Vec<u32> = digits.chars().filter_map(|c| c.to_digit(10)).collect();
    let (body, check) = values.split_at(values.len() - 1);
    let sum: u32 = body.iter().rev().enumerate().map(|(i, d)| d * if i % 2 == 0 { 3 } else { 1 }).sum();
    (10 - sum % 10) % 10 == check[0]
}

fn is_scale(key: &str) -> bool {
    key.strip_prefix("1/").is_some_and(|rest| !rest.is_empty() && rest.len() <= 3 && rest.chars().all(|c| c.is_ascii_digit()))
}

fn identifier_kind(key: &str) -> Option<TermKind> {
    if is_scale(key) {
        return Some(TermKind::Identifier);
    }
    if !key.is_empty() && key.chars().all(|c| c.is_ascii_digit()) {
        if key.len() <= 6 || valid_jan(key) {
            return Some(TermKind::Identifier);
        }
        return None;
    }
    // Product codes such as "4580590123456" handled above; "rx78" / "b0123" / "mg2" here.
    let compact: String = key.chars().filter(|c| *c != ' ').collect();
    let has_digit = compact.chars().any(|c| c.is_ascii_digit());
    let has_alpha = compact.chars().any(|c| c.is_ascii_alphabetic());
    if has_digit && has_alpha && compact.len() >= 3 && compact.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Some(TermKind::Identifier);
    }
    None
}

fn rule_for(key: &str, abbreviation: bool) -> MatchRule {
    let compact: String = key.chars().filter(|c| *c != ' ').collect();
    if !compact.is_empty() && compact.chars().all(|c| c.is_ascii_digit() || c == '/') {
        return MatchRule::Number;
    }
    if compact.chars().all(|c| c.is_ascii_alphanumeric()) {
        return MatchRule::AsciiBoundary;
    }
    let hangul_only = compact.chars().all(is_hangul);
    let length = compact.chars().count();
    if hangul_only && length <= 2 {
        return match (abbreviation, length) {
            (true, _) => MatchRule::TokenExact,
            (false, 1) => MatchRule::TokenSuffix,
            _ => MatchRule::TokenAffix,
        };
    }
    if !hangul_only && length == 1 && !compact.chars().all(is_cjk_or_kana) {
        return MatchRule::TokenExact;
    }
    MatchRule::Contains
}

fn alternative(text: &str, abbreviation: bool) -> Option<Alternative> {
    let key = match_key(text);
    if key.is_empty() {
        return None;
    }
    let compact = key.replace(' ', "");
    Some(Alternative { rule: rule_for(&key, abbreviation), key, compact })
}

fn alias_group(text: String, index: usize) -> TermGroup {
    let alias = &aliases()[index];
    let mut alternatives: Vec<Alternative> = alias.full_terms().filter_map(|t| alternative(t, false)).collect();
    alternatives.extend(alias.abbreviations.iter().filter_map(|t| alternative(t, true)));
    // What the user typed is always an acceptable spelling (unless it is one
    // of the alias spellings already, which keeps abbreviation strictness).
    if let Some(typed) = alternative(&text, false)
        && !alternatives.iter().any(|a| a.compact == typed.compact) {
            alternatives.push(typed);
        }
    TermGroup { text, kind: if alias.soft { TermKind::Soft } else { TermKind::Core }, alias: Some(index), alternatives }
}

fn plain_group(text: String) -> Option<TermGroup> {
    let key = match_key(&text);
    if key.is_empty() {
        return None;
    }
    let kind = if SOFT_WORDS.iter().any(|w| match_key(w) == key) {
        TermKind::Soft
    } else {
        identifier_kind(&key).unwrap_or(TermKind::Core)
    };
    let alternatives = vec![alternative(&text, false)?];
    Some(TermGroup { text, kind, alias: None, alternatives })
}

fn find_alias_exact(compact: &str) -> Option<usize> {
    aliases().iter().position(|alias| alias.all_terms().any(|t| compact_key(t) == compact))
}

/// Splits a long space-free token ("하츠네미쿠넨도로이드") on known alias spellings.
fn split_by_aliases(token: &str) -> Option<Vec<(String, Option<usize>)>> {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() < 5 {
        return None;
    }
    let lowered: Vec<char> = compact_key(token).chars().collect();
    if lowered.len() != chars.len() {
        return None; // punctuation inside; keep as-is
    }
    let mut terms: Vec<(Vec<char>, usize)> = aliases()
        .iter()
        .enumerate()
        .flat_map(|(i, alias)| alias.full_terms().map(move |t| (compact_key(t).chars().collect::<Vec<_>>(), i)))
        .filter(|(t, _)| t.len() >= 3 && t.len() < lowered.len())
        .collect();
    terms.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    let mut covered = vec![None; chars.len()];
    for (term, index) in &terms {
        let mut start = 0;
        while start + term.len() <= lowered.len() {
            if lowered[start..start + term.len()] == term[..] && covered[start..start + term.len()].iter().all(Option::is_none) {
                for slot in &mut covered[start..start + term.len()] {
                    *slot = Some(*index);
                }
                start += term.len();
            } else {
                start += 1;
            }
        }
    }
    if covered.iter().all(Option::is_none) {
        return None;
    }
    let mut parts: Vec<(String, Option<usize>)> = Vec::new();
    for (i, slot) in covered.iter().enumerate() {
        match parts.last_mut() {
            Some((text, last)) if *last == *slot && (slot.is_none() || i > 0 && covered[i - 1] == *slot) => text.push(chars[i]),
            _ => parts.push((chars[i].to_string(), *slot)),
        }
    }
    // Leftover single characters are noise ("의", particles)
    parts.retain(|(text, alias)| alias.is_some() || text.chars().count() >= 2);
    Some(parts)
}

pub fn analyze(input: &str) -> Analysis {
    let display_text = display(input);
    let tokens: Vec<&str> = display_text.split(' ').filter(|t| !t.is_empty()).collect();
    let mut groups = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let mut matched = None;
        for len in (1..=4.min(tokens.len() - i)).rev() {
            let joined = tokens[i..i + len].join(" ");
            if let Some(index) = find_alias_exact(&compact_key(&joined)) {
                matched = Some((len, joined, index));
                break;
            }
        }
        if let Some((len, joined, index)) = matched {
            groups.push(alias_group(joined, index));
            i += len;
            continue;
        }
        let token = tokens[i].to_string();
        match split_by_aliases(&token) {
            Some(parts) => {
                for (text, alias) in parts {
                    match alias {
                        Some(index) => groups.push(alias_group(text, index)),
                        None => groups.extend(plain_group(text)),
                    }
                }
            }
            None => groups.extend(plain_group(token)),
        }
        i += 1;
    }
    // The same alias typed twice ("미쿠 하츠네 미쿠") counts once.
    let mut seen = Vec::new();
    groups.retain(|g| match g.alias {
        Some(index) if seen.contains(&index) => false,
        Some(index) => {
            seen.push(index);
            true
        }
        None => true,
    });
    Analysis { original: input.to_string(), display: display_text, groups }
}

fn render(analysis: &Analysis, language: Option<Language>, skip_soft: bool) -> String {
    analysis
        .groups
        .iter()
        .filter(|g| !(skip_soft && g.kind == TermKind::Soft))
        .map(|g| match (g.alias, language) {
            (Some(index), Some(language)) => aliases()[index].first(language).cloned().unwrap_or_else(|| g.text.clone()),
            _ => g.text.clone(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Queries to send to a store, best first. Stores are tried with at most the
/// first two; the rest are kept for diagnostics.
pub fn query_candidates(analysis: &Analysis, preferred: Option<Language>) -> Vec<String> {
    let original = render(analysis, None, false);
    let core_only = render(analysis, None, true);
    let mut out = Vec::new();
    match preferred {
        Some(language) => {
            out.push(render(analysis, Some(language), false));
            out.push(render(analysis, Some(language), true));
            out.push(original);
        }
        None => {
            out.push(original);
            out.push(render(analysis, Some(Language::Korean), false));
            out.push(core_only);
        }
    }
    let mut unique: Vec<String> = Vec::new();
    for candidate in out {
        if !candidate.trim().is_empty() && !unique.iter().any(|u| compact_key(u) == compact_key(&candidate)) {
            unique.push(candidate);
        }
    }
    unique
}

/// A product name prepared for matching.
pub struct NameKey {
    spaced: String,
    compact: String,
    tokens: Vec<String>,
}

impl NameKey {
    pub fn new(name: &str) -> Self {
        let spaced = match_key(name);
        let compact = spaced.replace(' ', "");
        let tokens = spaced.split(' ').map(String::from).collect();
        NameKey { spaced, compact, tokens }
    }
}

fn bounded_occurrence(haystack: &str, needle: &str, blocks: impl Fn(char) -> bool) -> bool {
    if needle.is_empty() {
        return false;
    }
    let mut from = 0;
    while let Some(pos) = haystack[from..].find(needle) {
        let start = from + pos;
        let end = start + needle.len();
        let before = haystack[..start].chars().next_back();
        let after = haystack[end..].chars().next();
        if !before.is_some_and(&blocks) && !after.is_some_and(&blocks) {
            return true;
        }
        from = start + haystack[start..].chars().next().map(char::len_utf8).unwrap_or(1);
    }
    false
}

fn alternative_matches(alt: &Alternative, name: &NameKey) -> bool {
    match alt.rule {
        MatchRule::Number => {
            bounded_occurrence(&name.spaced, &alt.compact, |c| c.is_ascii_digit() || c == '/')
        }
        MatchRule::AsciiBoundary => {
            let blocks = |c: char| c.is_ascii_alphanumeric();
            bounded_occurrence(&name.spaced, &alt.key, blocks)
                || (alt.key.contains(' ') && bounded_occurrence(&name.compact, &alt.compact, blocks))
        }
        MatchRule::Contains => name.compact.contains(&alt.compact),
        MatchRule::TokenAffix => name.tokens.iter().any(|t| t.starts_with(&alt.compact) || t.ends_with(&alt.compact)),
        MatchRule::TokenExact => name.tokens.contains(&alt.compact),
        MatchRule::TokenSuffix => name.tokens.iter().any(|t| t.ends_with(&alt.compact)),
    }
}

pub fn group_matches(group: &TermGroup, name: &NameKey) -> bool {
    group.alternatives.iter().any(|alt| alternative_matches(alt, name))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Relevance {
    pub score: f32,
    pub passed: bool,
    pub missing: Vec<String>,
}

/// Decides whether a product name actually answers the query.
///
/// Every identifier (product number, scale, JAN) must match. Core words must
/// all match for short queries; long queries tolerate one miss per three words.
/// Soft words (manufacturers, "피규어", "정품"...) only adjust the score.
pub fn relevance(analysis: &Analysis, name: &str) -> Relevance {
    if analysis.groups.is_empty() {
        return Relevance { score: 1.0, passed: true, missing: vec![] };
    }
    let key = NameKey::new(name);
    let mut total = 0.0;
    let mut gained = 0.0;
    let mut missing = Vec::new();
    let mut core_total = 0usize;
    let mut core_missed = 0usize;
    let mut identifier_missed = false;
    for group in &analysis.groups {
        let weight = match group.kind {
            TermKind::Core => 1.0,
            TermKind::Identifier => 1.5,
            TermKind::Soft => 0.25,
        };
        total += weight;
        let hit = group_matches(group, &key);
        if hit {
            gained += weight;
        }
        match group.kind {
            TermKind::Core => {
                core_total += 1;
                if !hit {
                    core_missed += 1;
                    missing.push(group.text.clone());
                }
            }
            TermKind::Identifier if !hit => {
                identifier_missed = true;
                missing.push(group.text.clone());
            }
            _ => {}
        }
    }
    let allowed_misses = core_total.saturating_sub(1) / 3;
    let has_required = core_total > 0 || analysis.identifiers().next().is_some();
    let passed = !identifier_missed && core_missed <= allowed_misses && (has_required || gained > 0.0 || total == 0.0);
    // Pure soft queries ("피규어") accept anything the store returned.
    let passed = passed || !has_required;
    Relevance { score: if total > 0.0 { gained / total } else { 1.0 }, passed, missing }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn passes(query: &str, name: &str) -> bool {
        relevance(&analyze(query), name).passed
    }

    #[test]
    fn alias_file_parses() {
        let miku = aliases().iter().find(|a| a.korean[0] == "하츠네 미쿠").unwrap();
        assert_eq!(miku.japanese, vec!["初音ミク"]);
        assert!(aliases().iter().any(|a| a.soft && a.korean.contains(&"굿스마일".to_string())));
    }

    #[test]
    fn groups_multi_word_aliases_and_identifiers() {
        let a = analyze("넨도 하츠네 미쿠 2301 피규어");
        let kinds: Vec<_> = a.groups.iter().map(|g| (g.text.as_str(), g.kind)).collect();
        assert_eq!(
            kinds,
            vec![
                ("넨도", TermKind::Core),
                ("하츠네 미쿠", TermKind::Core),
                ("2301", TermKind::Identifier),
                ("피규어", TermKind::Soft)
            ]
        );
    }

    #[test]
    fn splits_space_free_alias_runs() {
        let a = analyze("하츠네미쿠넨도로이드");
        assert_eq!(a.groups.len(), 2);
        assert!(passes("하츠네미쿠넨도로이드", "넨도로이드 하츠네 미쿠 V4X"));
    }

    #[test]
    fn rejects_unrelated_products() {
        assert!(passes("넨도로이드 미쿠", "[예약] 넨도로이드 하츠네 미쿠 16th Anniver. Ver."));
        assert!(passes("넨도로이드 미쿠", "ねんどろいど 初音ミク"));
        assert!(passes("넨도로이드 미쿠", "Nendoroid Hatsune Miku"));
        assert!(!passes("넨도로이드 미쿠", "넨도로이드 카가미네 린"));
        assert!(!passes("넨도로이드 미쿠", "figma 하츠네 미쿠"));
        assert!(!passes("미쿠", "Mikuni Shimokawa CD"));
    }

    #[test]
    fn identifiers_are_mandatory_and_bounded() {
        assert!(passes("넨도로이드 2301", "넨도로이드 2301 하츠네 미쿠"));
        assert!(passes("넨도로이드 2301", "넨도로이드#2301"));
        assert!(!passes("넨도로이드 2301", "넨도로이드 23015 누군가"));
        assert!(!passes("넨도로이드 2301", "넨도로이드 2302 누군가"));
        assert!(passes("미쿠 1/7", "初音ミク 1/7 スケールフィギュア"));
        assert!(!passes("미쿠 1/7", "하츠네 미쿠 1/8 스케일"));
    }

    #[test]
    fn manufacturer_and_generic_words_are_optional() {
        assert!(passes("굿스마일 미쿠 넨도로이드 피규어", "넨도로이드 하츠네 미쿠"));
        assert!(passes("피규어", "아무 상품"));
    }

    #[test]
    fn short_hangul_and_abbreviations_respect_word_boundaries() {
        assert!(!passes("홀로라이브", "홀로그램 카드 세트"));
        assert!(passes("홀로", "홀로라이브 호쇼 마린 피규어"));
        assert!(!passes("에바", "에바폼 매트"), "abbreviation must not match inside other words");
        assert!(passes("렘", "리제로 렘 & 람 피규어"));
        assert!(!passes("렘", "렘브란트 화집"));
        assert!(passes("RG 건담", "RG건담 RX-78-2"));
        assert!(!passes("MG", "image of something"));
    }

    #[test]
    fn long_queries_tolerate_one_missing_word() {
        assert!(passes("블루 아카이브 아리스 바니 걸 버전", "블루 아카이브 아리스 바니걸 1/7"));
        assert!(!passes("블루 아카이브 아리스", "블루 아카이브 유우카 1/7"));
    }

    #[test]
    fn candidates_prefer_original_then_korean_alias_then_core() {
        let a = analyze("初音ミク ねんどろいど 피규어");
        let c = query_candidates(&a, None);
        assert_eq!(c[0], "初音ミク ねんどろいど 피규어");
        assert_eq!(c[1], "하츠네 미쿠 넨도로이드 피규어");
        assert_eq!(c[2], "初音ミク ねんどろいど");
        let jp = query_candidates(&analyze("넨도 미쿠"), Some(Language::Japanese));
        assert_eq!(jp[0], "ねんどろいど 初音ミク");
    }

    #[test]
    fn jan_codes_need_a_valid_checksum() {
        assert_eq!(analyze("4006381333931").groups[0].kind, TermKind::Identifier);
        assert_eq!(analyze("4904810912345").groups[0].kind, TermKind::Core);
    }
}
