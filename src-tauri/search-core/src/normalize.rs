//! Text normalization used for matching product names against queries.
//!
//! `display` keeps the text readable (width-folded, collapsed whitespace).
//! `match_key` additionally lowercases, folds hiragana to katakana and turns
//! punctuation into spaces so that "Nendoroid", "ねんどろいど" and
//! "ネンドロイド" compare the way people expect.

/// Width folding similar to NFKC for the characters that matter here:
/// full-width ASCII, ideographic space, half-width katakana (with voiced marks).
pub fn fold_width(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let code = c as u32;
        if (0xFF01..=0xFF5E).contains(&code) {
            out.push(char::from_u32(code - 0xFEE0).unwrap());
        } else if c == '\u{3000}' || c == '\u{00A0}' {
            out.push(' ');
        } else if (0xFF61..=0xFF9F).contains(&code) {
            let base = halfwidth_kana(c);
            let next = chars.get(i + 1).copied();
            if next == Some('\u{FF9E}') {
                if let Some(v) = voiced(base) {
                    out.push(v);
                    i += 2;
                    continue;
                }
            } else if next == Some('\u{FF9F}')
                && let Some(v) = semi_voiced(base) {
                    out.push(v);
                    i += 2;
                    continue;
                }
            out.push(base);
        } else if c == '￦' {
            out.push('₩');
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

fn halfwidth_kana(c: char) -> char {
    const TABLE: &str = "。「」、・ヲァィゥェォャュョッーアイウエオカキクケコサシスセソタチツテトナニヌネノハヒフヘホマミムメモヤユヨラリルレロワン゛゜";
    let index = (c as u32 - 0xFF61) as usize;
    TABLE.chars().nth(index).unwrap_or(c)
}

fn voiced(c: char) -> Option<char> {
    let src = "カキクケコサシスセソタチツテトハヒフヘホウ";
    let dst = "ガギグゲゴザジズゼゾダヂヅデドバビブベボヴ";
    src.chars().position(|s| s == c).and_then(|p| dst.chars().nth(p))
}

fn semi_voiced(c: char) -> Option<char> {
    let src = "ハヒフヘホ";
    let dst = "パピプペポ";
    src.chars().position(|s| s == c).and_then(|p| dst.chars().nth(p))
}

/// Readable normalized form: width folded, whitespace collapsed, trimmed.
pub fn display(input: &str) -> String {
    fold_width(input).split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '/' || c == 'ー'
}

/// Matching form: lowercase, katakana-folded, punctuation as spaces.
/// A `/` between digits is kept so scales such as `1/7` survive.
pub fn match_key(input: &str) -> String {
    let folded = fold_width(input).to_lowercase();
    let mut out = String::with_capacity(folded.len());
    let chars: Vec<char> = folded.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        let c = hira_to_kata(c);
        if c == '/' {
            let digit_before = i > 0 && chars[i - 1].is_ascii_digit();
            let digit_after = chars.get(i + 1).is_some_and(|n| n.is_ascii_digit());
            out.push(if digit_before && digit_after { '/' } else { ' ' });
        } else if c == '.' && i > 0 && chars[i - 1].is_alphabetic() && chars.get(i + 1).is_some_and(|n| n.is_alphabetic()) {
            // "s.h.figuarts" -> "shfiguarts"
        } else if is_word_char(c) {
            out.push(c);
        } else {
            out.push(' ');
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Match key without any spaces ("하츠네 미쿠" == "하츠네미쿠").
pub fn compact_key(input: &str) -> String {
    match_key(input).chars().filter(|c| *c != ' ').collect()
}

fn hira_to_kata(c: char) -> char {
    let code = c as u32;
    if (0x3041..=0x3096).contains(&code) {
        char::from_u32(code + 0x60).unwrap_or(c)
    } else {
        c
    }
}

pub fn is_hangul(c: char) -> bool {
    ('\u{AC00}'..='\u{D7A3}').contains(&c) || ('\u{3131}'..='\u{318E}').contains(&c)
}

pub fn is_cjk_or_kana(c: char) -> bool {
    ('\u{3040}'..='\u{30FF}').contains(&c) || ('\u{4E00}'..='\u{9FFF}').contains(&c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_halfwidth_katakana_and_fullwidth_ascii() {
        assert_eq!(display("  ﾊﾂﾈ　ﾐｸ   ０１ "), "ハツネ ミク 01");
        assert_eq!(display("ｶﾞﾝﾀﾞﾑ ﾎﾟｹﾓﾝ"), "ガンダム ポケモン");
    }

    #[test]
    fn match_key_folds_case_kana_and_punctuation() {
        assert_eq!(match_key("ねんどろいど【初音ミク】"), "ネンドロイド 初音ミク");
        assert_eq!(match_key("Nendoroid #2301"), "nendoroid 2301");
        assert_eq!(match_key("1/7 스케일 / S.H.Figuarts"), "1/7 스케일 shfiguarts");
        assert_eq!(compact_key("하츠네 미쿠"), "하츠네미쿠");
    }
}
