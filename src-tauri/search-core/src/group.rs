//! Grouping key for listings of the same product across stores.
//!
//! Two listings get the same key when their names agree after dropping store
//! noise (sale state, shipping, release dates) and manufacturer names, and after
//! folding every alias spelling ("하츠네 미쿠", "初音ミク", "Hatsune Miku") to one
//! entry. Spacing is ignored, and so is the position of alias words.

use crate::classify::Tag;
use crate::normalize::{compact_key, match_key};
use crate::query::aliases;
use std::collections::BTreeSet;
use std::sync::OnceLock;

/// Words stores add around the real product name.
const NOISE: &[&str] = &[
    "예약", "예약판매", "예약상품", "선주문", "재판", "재입고", "입고", "입고완료", "발매", "출시", "발송", "예정",
    "당일발송", "즉시발송", "빠른배송", "무료배송", "정품", "신품", "새상품", "미개봉", "국내", "국내정발", "정발",
    "일본", "일본판", "해외", "특전", "한정", "마감", "품절", "판매중", "재고", "pre", "order", "preorder",
];

/// "25년", "3월", "2차", "25년3월".
fn is_date(word: &str) -> bool {
    let mut saw_digit = false;
    let mut complete = false;
    for c in word.chars() {
        if c.is_ascii_digit() {
            saw_digit = true;
            complete = false;
        } else if saw_digit && matches!(c, '년' | '월' | '일' | '차') {
            saw_digit = false;
            complete = true;
        } else {
            return false;
        }
    }
    complete
}

struct Spelling {
    compact: String,
    alias: usize,
    /// Abbreviations only count as a whole word ("홀로" is not in "홀로그램").
    whole_only: bool,
}

/// Every alias spelling, longest first so "넨도로이드돌" wins over "넨도로이드".
fn spellings() -> &'static [Spelling] {
    static SPELLINGS: OnceLock<Vec<Spelling>> = OnceLock::new();
    SPELLINGS.get_or_init(|| {
        let mut list: Vec<Spelling> = aliases()
            .iter()
            .enumerate()
            .flat_map(|(alias, a)| {
                let full = a.korean.iter().chain(&a.japanese).chain(&a.english).map(move |t| (t, alias, false));
                full.chain(a.abbreviations.iter().map(move |t| (t, alias, true)))
            })
            .map(|(term, alias, whole_only)| Spelling { compact: compact_key(term), alias, whole_only })
            .filter(|s| !s.compact.is_empty())
            .collect();
        list.sort_by(|a, b| b.compact.chars().count().cmp(&a.compact.chars().count()));
        list
    })
}

fn whole_match(text: &str) -> Option<usize> {
    spellings().iter().find(|s| s.compact == text).map(|s| s.alias)
}

/// Cuts alias spellings out of a single word ("ネンドロイド初音ミク").
fn strip_spellings(word: &str, found: &mut BTreeSet<usize>) -> String {
    let mut rest = word.to_string();
    for s in spellings().iter().filter(|s| !s.whole_only && s.compact.chars().count() >= 3) {
        if rest.contains(&s.compact) {
            rest = rest.replace(&s.compact, " ");
            found.insert(s.alias);
        }
    }
    rest.split_whitespace().collect()
}

/// Key shared by listings of the same product. Empty when nothing identifying is left.
pub fn group_key(name: &str, tags: &[Tag]) -> String {
    let key = match_key(name);
    let words: Vec<&str> = key.split(' ').filter(|w| !w.is_empty() && !NOISE.contains(w) && !is_date(w)).collect();
    let mut found = BTreeSet::new();
    let mut rest = String::new();
    let mut i = 0;
    'words: while i < words.len() {
        // Spellings written with spaces ("하츠네 미쿠", "Good Smile Company").
        for len in (1..=4.min(words.len() - i)).rev() {
            if let Some(alias) = whole_match(&words[i..i + len].concat()) {
                found.insert(alias);
                i += len;
                continue 'words;
            }
        }
        rest.push_str(&strip_spellings(words[i], &mut found));
        i += 1;
    }
    // Manufacturers are often left out of names, so they never split a group.
    let identifying: Vec<String> =
        found.into_iter().filter(|&a| !aliases()[a].soft).map(|a| a.to_string()).collect();
    if identifying.is_empty() && rest.is_empty() {
        return String::new();
    }
    let condition = match (tags.contains(&Tag::Used), tags.contains(&Tag::Bootleg)) {
        (true, true) => "used,bootleg",
        (true, false) => "used",
        (false, true) => "bootleg",
        (false, false) => "new",
    };
    format!("{condition}|{}|{rest}", identifying.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str) -> String {
        group_key(name, &[])
    }

    #[test]
    fn ignores_store_noise_spacing_and_alias_spelling() {
        let base = key("넨도로이드 하츠네 미쿠 2301");
        assert!(!base.is_empty());
        assert_eq!(key("[예약] 넨도로이드 하츠네미쿠 #2301 (25년 3월 발매)"), base);
        assert_eq!(key("하츠네 미쿠 넨도로이드 ２３０１ 국내정발"), base);
        assert_eq!(key("ねんどろいど 初音ミク 2301"), base);
        assert_eq!(key("Nendoroid Hatsune Miku 2301"), base);
        assert_eq!(key("ネンドロイド初音ミク 2301"), base);
        assert_eq!(key("굿스마일컴퍼니 넨도로이드 미쿠 2301"), base);
    }

    #[test]
    fn keeps_different_products_apart() {
        let base = key("넨도로이드 하츠네 미쿠 2301");
        assert_ne!(key("넨도로이드 하츠네 미쿠 2302"), base);
        assert_ne!(key("넨도로이드 돌 하츠네 미쿠 2301"), base);
        assert_ne!(key("figma 하츠네 미쿠 2301"), base);
        assert_ne!(group_key("넨도로이드 하츠네 미쿠 2301", &[Tag::Used]), base);
        assert_ne!(key("홀로그램 스티커"), key("홀로라이브 스티커"));
    }

    #[test]
    fn empty_when_only_noise() {
        assert_eq!(key("[예약] 25년 3월 입고"), "");
        assert!(is_date("25년3월") && is_date("2차") && !is_date("2301") && !is_date("년"));
    }
}
