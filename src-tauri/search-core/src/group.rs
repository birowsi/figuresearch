//! Grouping key for listings of the same product across stores.
//!
//! Two listings get the same key when their names agree after dropping store
//! noise (sale state, shipping, release dates) and manufacturer names, and after
//! folding every alias spelling ("하츠네 미쿠", "初音ミク", "Hatsune Miku") to one
//! entry. Spacing is ignored, and so is the position of alias words.

use crate::classify::{Category, Tag};
use crate::normalize::{compact_key, grouping_title_key};
use crate::query::aliases;
use std::collections::BTreeSet;
use std::sync::OnceLock;

/// Words stores add around the real product name.
const NOISE: &[&str] = &[
    "예약", "예약판매", "예약상품", "선주문", "재판", "재입고", "입고", "입고완료", "입고예정", "발매", "출시", "발송", "예정",
    "당일발송", "즉시발송", "빠른배송", "무료배송", "정품", "신품", "새상품", "미개봉", "국내", "국내정발", "정발",
    "일본", "일본판", "해외", "마감", "품절", "판매중", "재고", "pre", "order", "preorder",
    "ver", "version", "버전", "초특가", "특가",
];

/// "25년", "3월", "2차", "25년3월".
fn is_date(word: &str) -> bool {
    let word = ["중입고예정", "입고예정", "입고완료", "입하예정", "입하", "입고", "발매예정", "발매"]
        .iter().find_map(|suffix| word.strip_suffix(suffix)).unwrap_or(word);
    if let Some((year, month)) = word.split_once('.')
        && year.len() == 4 && year.parse::<u16>().is_ok_and(|n| (2000..=2099).contains(&n))
        && month.len() <= 2 && month.parse::<u8>().is_ok_and(|n| (1..=12).contains(&n))
    {
        return true;
    }
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

fn is_product_number(word: &str) -> bool {
    !word.is_empty() && word.bytes().all(|byte| byte.is_ascii_digit())
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

/// An exact grouping key and, for named figure editions, a key without the serial number.
pub fn group_keys(name: &str, category: Category, tags: &[Tag]) -> (String, Option<String>) {
    let serial_figure = matches!(category, Category::Nendoroid | Category::NendoroidDoll | Category::Figma);
    let mut key = grouping_title_key(name);
    if serial_figure {
        // These labels describe the franchise, including 01 from its name.
        key = format!(" {key} ");
        for label in ["캐릭터 보컬 시리즈", "캐릭터보컬시리즈", "character vocal series"] {
            key = key.replace(&format!(" {label} 01 "), " ")
                .replace(&format!(" {label}01 "), " ")
                .replace(&format!(" {label} "), " ");
        }
    }
    let mut tokens = key.split_whitespace().peekable();
    let mut words = Vec::new();
    while let Some(word) = tokens.next() {
        if NOISE.contains(&word) || is_date(word)
            || (serial_figure && matches!(word, "vocaloid" | "보컬로이드" | "ボーカロイド" | "피규어" | "figure"))
        {
            continue;
        }
        if serial_figure && matches!(word, "no" | "넘버") && tokens.peek().is_some_and(|next| is_product_number(next)) {
            continue;
        }
        let word = if serial_figure {
            word.strip_prefix("no").or_else(|| word.strip_suffix('번')).filter(|number| is_product_number(number)).unwrap_or(word)
        } else {
            word
        };
        // Sellers write the same version as either "Ver.2.0" or "Ver2.0".
        let word = word.strip_prefix("ver").filter(|version| version.starts_with(|c: char| c.is_ascii_digit()))
            .or_else(|| word.strip_suffix("ver").filter(|name| name.chars().last().is_some_and(|c| !c.is_ascii() && c.is_alphabetic())))
            .unwrap_or(word);
        words.push(word);
    }
    let serial = if serial_figure {
        let mut numbers = words.iter().copied().filter(|word| word.len() <= 5 && is_product_number(word));
        numbers.next().filter(|_| numbers.next().is_none())
    } else {
        None
    };
    if let Some(number) = serial {
        words.retain(|word| *word != number);
    }
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
    if identifying.is_empty() && rest.is_empty() && serial.is_none() {
        return (String::new(), None);
    }
    let condition = match (tags.contains(&Tag::Used), tags.contains(&Tag::Bootleg)) {
        (true, true) => "used,bootleg",
        (true, false) => "used",
        (false, true) => "bootleg",
        (false, false) => "new",
    };
    let bonus = if tags.contains(&Tag::Bonus) { "|bonus" } else { "" };
    let title = format!("{category:?}|{condition}{bonus}|{}|{rest}", identifying.join(","));
    let exact = serial.map_or_else(|| title.clone(), |number| format!("{title}|{number}"));
    let named_edition = serial_figure && !identifying.is_empty() && rest.chars().any(char::is_alphabetic);
    (exact, named_edition.then_some(title))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str) -> String {
        group_keys(name, Category::Nendoroid, &[]).0
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
        assert_ne!(group_keys("넨도로이드 하츠네 미쿠 2301", Category::Nendoroid, &[Tag::Used]).0, base);
        assert_ne!(key("홀로그램 스티커"), key("홀로라이브 스티커"));
    }

    #[test]
    fn groups_number_and_version_spellings_but_keeps_editions_and_bundles_separate() {
        let name = "넨도로이드 하츠네 미쿠 3068 메스머라이저";
        let base = key(name);
        assert_eq!(key("[예약] Nendoroid Hatsune Miku No.3068 메스머라이저 Ver. 2026.11 입고"), base);
        assert_eq!(key("넨도로이드 하츠네 미쿠 3068번 메스머라이저 버전"), base);
        assert_eq!(key("넨도로이드 하츠네 미쿠 메스머라이저 No3068"), base);
        assert_eq!(key("[신품][VOCALOID] 넨도로이드 No.3068 하츠네 미쿠 메스머라이저 Ver."), base);
        assert_eq!(key("[26년11월입하]굿스마일 넨도로이드 3068 하츠네 미쿠 메스머라이저Ver."), base);
        assert_eq!(key("[예약상품/26년 11월~12월 입고예정][굿스마일컴퍼니][보컬로이드] 넨도로이드 No.3068 캐릭터 보컬 시리즈 01: 하츠네 미쿠 메스머라이저 Ver."), base);
        assert_eq!(key("[26년 10월 발매] 하츠네 미쿠 보컬로이드 굿스마일 컴퍼니 넨도로이드 3068 피규어 - 하츠네 미쿠 메스머라이저 Ver."), base);
        assert_ne!(key(&format!("{name} 한정")), base);
        assert_ne!(key(&format!("{name} 세트")), base);
        assert_ne!(group_keys(name, Category::Nendoroid, &[Tag::Bonus]).0, base);
        assert_ne!(group_keys(name, Category::Plamo, &[]).0, base);
        let fallback = group_keys("<초특가> 넨도로이드 하츠네 미쿠 메스머라이저 ver.", Category::Nendoroid, &[]);
        assert_eq!(group_keys(name, Category::Nendoroid, &[]).1, Some(fallback.0));
        assert_eq!(
            group_keys("넨도로이드 캐릭터 보컬 시리즈01 하츠네 미쿠 메스머라이저ver.", Category::Nendoroid, &[]).1,
            group_keys(name, Category::Nendoroid, &[]).1,
        );
        assert!(group_keys("넨도로이드 하츠네 미쿠", Category::Nendoroid, &[]).1.is_none());

        let kit = |name| group_keys(name, Category::Plamo, &[]).0;
        assert_eq!(kit("RG RX-78-2 건담 Ver2.0"), kit("RG RX782 건담 Ver. 2.0"));
        assert_ne!(kit("RG RX-78-2 건담 2.0"), kit("RG RX-78-2 건담 1.0"));
        assert_ne!(kit("RG RX-78-2 건담 2.0"), kit("RG RX-78-2 건담 20"));
    }

    #[test]
    fn empty_when_only_noise() {
        assert_eq!(key("[예약] 25년 3월 입고"), "");
        assert!(is_date("25년3월") && is_date("2차") && !is_date("2301") && !is_date("년"));
    }
}
