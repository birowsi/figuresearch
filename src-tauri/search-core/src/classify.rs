//! Rule-based product categorization from product names and badges.

use crate::normalize::match_key;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Category {
    Nendoroid,
    NendoroidDoll,
    Figma,
    ActionFigure,
    ScaleFigure,
    PopUpParade,
    PrizeFigure,
    Kuji,
    Plamo,
    Gacha,
    Plush,
    Goods,
    Accessory,
    Book,
    Figure,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Tag {
    /// 예약 상품
    Preorder,
    /// 중고 / 개봉품
    Used,
    /// 특전·한정
    Bonus,
    /// 재판(再販)
    Reissue,
    /// 해적판 의심
    Bootleg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreKind {
    Shop,
    Bookstore,
    Marketplace,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Classification {
    pub category: Category,
    pub tags: Vec<Tag>,
    /// "2026년 3월" style release/arrival month when the name mentions one.
    pub release: Option<String>,
}

fn has_any(key: &str, words: &[&str]) -> bool {
    words.iter().any(|w| contains_word(key, w))
}

/// Substring match; ASCII words must not touch other ASCII letters/digits.
fn contains_word(key: &str, word: &str) -> bool {
    let word = match_key(word);
    if word.is_empty() {
        return false;
    }
    let ascii = word.chars().all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '/');
    let mut from = 0;
    while let Some(pos) = key[from..].find(&word) {
        let start = from + pos;
        let end = start + word.len();
        if !ascii {
            return true;
        }
        let before = key[..start].chars().next_back();
        let after = key[end..].chars().next();
        let blocks = |c: char| c.is_ascii_alphanumeric();
        if !before.is_some_and(blocks) && !after.is_some_and(blocks) {
            return true;
        }
        from = start + 1;
    }
    false
}

fn scale_denominator(key: &str) -> Option<u32> {
    key.split(' ').find_map(|token| {
        let rest = token.strip_prefix("1/")?;
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    })
}

const ACCESSORY: &[&str] = &[
    "넨도로이드 모어", "nendoroid more", "ねんどろいどもあ", "전용 케이스", "디스플레이 케이스",
    "아크릴 케이스", "보호 케이스", "교체 파츠", "교체용", "부품", "거치대", "figma 전용", "액션 베이스",
    "action base", "더미 박스", "스탠드 베이스",
];
const BOOK_WORDS: &[&str] = &[
    "코믹스", "만화", "소설", "라이트노벨", "단행본", "画集", "아트북", "artbook", "art book", "설정집",
    "화보집", "공식 가이드", "팬북", "fanbook", "잡지", "무크",
];
const KUJI: &[&str] = &["이치방쿠지", "이치방 쿠지", "一番くじ", "ichiban kuji", "제일복권", "일번쿠지"];
const NENDO_DOLL: &[&str] = &["넨도로이드 돌", "넨도로이드돌", "nendoroid doll", "ねんどろいどどーる", "넨돌"];
const NENDO: &[&str] = &["넨도로이드", "ねんどろいど", "nendoroid", "넨도"];
const FIGMA: &[&str] = &["figma", "피그마"];
const POP_UP: &[&str] = &["팝업 퍼레이드", "팝업퍼레이드", "pop up parade", "popupparade", "ポップアップパレード"];
const PLAMO: &[&str] = &[
    "프라모델", "건프라", "プラモデル", "plastic model", "plamo", "모데로이드", "moderoid", "피규어라이즈",
    "figure-rise", "figure rise", "30ms", "30mm", "hguc", "mgsd", "mgex", "re/100", "엔트리 그레이드",
    "entry grade", "조립식", "프라키트", "plakit",
];
const PLAMO_GRADES: &[&str] = &["hg", "mg", "rg", "pg", "sd", "eg", "bb전사"];
const ACTION: &[&str] = &[
    "피규아츠", "figuarts", "shfiguarts", "shf", "핫토이", "hot toys", "hottoys", "액션 피규어",
    "action figure", "가동 피규어", "리볼텍", "revoltech", "메자마", "mezco", "robot spirits", "로봇혼",
    "超合金", "초합금",
];
const PRIZE: &[&str] = &[
    "프라이즈", "경품", "prize", "큐포스켓", "qposket", "q posket", "반프레스토", "banpresto", "spm",
    "sss 피규어", "코어풀", "coreful", "트리오 트라이 아이트", "trio-try-it", "누들 스토퍼", "noodle stopper",
    "느들스토퍼", "글리터 앤 글래머러스", "glitter & glamours", "이치반쇼", "ichibansho", "grandista",
    "그랜디스타", "dxf", "maximatic", "relax time", "릴랙스 타임", "luminasta", "루미나스타", "aqua float girls",
];
const GACHA: &[&str] = &[
    "가챠", "가샤폰", "가차폰", "캡슐토이", "캡슐 토이", "ガチャ", "gashapon", "gacha", "식완", "식품완구",
    "shokugan", "블라인드", "랜덤 박스", "랜덤박스", "랜덤 피규어", "트레이딩 피규어", "트레이딩피규어",
    "コレクション", "박스 판매", "1box", "1 box",
];
const PLUSH: &[&str] = &["봉제", "인형", "누이", "ぬいぐるみ", "plush", "쿠션", "말랑", "모찌모찌", "마스코트 인형"];
const GOODS: &[&str] = &[
    "아크릴", "아크릴 스탠드", "아크스탠드", "키링", "키홀더", "열쇠고리", "뱃지", "캔뱃지", "배지", "포스터",
    "태피스트리", "타페스트리", "머그", "컵", "스티커", "클리어 파일", "클리어파일", "티셔츠", "포토카드",
    "카드", "색지", "브로마이드", "코롯토", "아크릴 블록", "마우스패드", "장패드", "파우치", "가방",
    "에코백", "부채", "손거울", "펜", "acrylic", "keychain", "badge", "poster", "tapestry",
];
const SCALE_WORDS: &[&str] = &["스케일", "scale", "スケール", "완성품 피규어", "완성품"];
const FIGURE_WORDS: &[&str] = &["피규어", "figure", "フィギュア", "스태추", "statue", "흉상", "bust", "디오라마", "피겨"];

const PREORDER: &[&str] = &[
    "예약", "선주문", "予約", "pre-order", "preorder", "pre order", "예판", "발매 예정", "발매예정",
    "입고 예정", "입고예정", "출시 예정", "출시예정",
];
const USED: &[&str] = &["중고", "used", "개봉품", "개봉 상품", "박스 손상", "미개봉 중고", "b급"];
const BONUS: &[&str] = &["특전", "한정", "限定", "limited", "초회", "선착", "購入特典", "특별판"];
const REISSUE: &[&str] = &["재판", "再販", "재생산", "reissue", "re-run", "재발매"];
const BOOTLEG: &[&str] = &["해적판", "짝퉁", "가품", "bootleg", "비정품", "중국판", "카피품", "복제품"];

pub fn classify(name: &str, badges: &str, store: StoreKind) -> Classification {
    let key = match_key(name);
    let badge_key = match_key(badges);
    let both = format!("{key} {badge_key}");

    let category = if has_any(&key, ACCESSORY) {
        Category::Accessory
    } else if has_any(&key, KUJI) {
        Category::Kuji
    } else if has_any(&key, NENDO_DOLL) {
        Category::NendoroidDoll
    } else if has_any(&key, NENDO) {
        Category::Nendoroid
    } else if has_any(&key, FIGMA) {
        Category::Figma
    } else if has_any(&key, POP_UP) {
        Category::PopUpParade
    } else if has_any(&key, PLAMO)
        || (has_any(&key, PLAMO_GRADES) && (has_any(&key, &["건담", "gundam", "ガンダム", "자쿠", "zaku"]) || scale_denominator(&key).is_some_and(|d| d >= 60)))
        || scale_denominator(&key).is_some_and(|d| d >= 35)
    {
        Category::Plamo
    } else if has_any(&key, ACTION) {
        Category::ActionFigure
    } else if has_any(&key, PRIZE) {
        Category::PrizeFigure
    } else if has_any(&key, GACHA) {
        Category::Gacha
    } else if scale_denominator(&key).is_some() || has_any(&key, SCALE_WORDS) {
        Category::ScaleFigure
    } else if has_any(&key, PLUSH) && !has_any(&key, FIGURE_WORDS) {
        Category::Plush
    } else if has_any(&key, GOODS) && !has_any(&key, FIGURE_WORDS) {
        Category::Goods
    } else if has_any(&key, BOOK_WORDS) || (store == StoreKind::Bookstore && !has_any(&key, FIGURE_WORDS)) {
        Category::Book
    } else if has_any(&key, FIGURE_WORDS) {
        Category::Figure
    } else {
        Category::Other
    };

    let mut tags = Vec::new();
    if has_any(&both, PREORDER) {
        tags.push(Tag::Preorder);
    }
    if store == StoreKind::Marketplace || has_any(&both, USED) {
        tags.push(Tag::Used);
    }
    if has_any(&both, BONUS) {
        tags.push(Tag::Bonus);
    }
    if has_any(&both, REISSUE) {
        tags.push(Tag::Reissue);
    }
    if has_any(&both, BOOTLEG) {
        tags.push(Tag::Bootleg);
    }

    Classification { category, tags, release: release_month(name) }
}

/// Finds "26년 3월", "2026년 03월", "2026.03" or "3월 발매/입고" in a name.
pub fn release_month(name: &str) -> Option<String> {
    let chars: Vec<char> = name.chars().collect();
    let digits_at = |start: usize| -> (String, usize) {
        let s: String = chars[start..].iter().take_while(|c| c.is_ascii_digit()).collect();
        let len = s.chars().count();
        (s, start + len)
    };
    let skip_ws = |mut i: usize| {
        while i < chars.len() && chars[i] == ' ' {
            i += 1;
        }
        i
    };
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() || (i > 0 && chars[i - 1].is_ascii_digit()) {
            i += 1;
            continue;
        }
        let (year, after_year) = digits_at(i);
        let year_num: u32 = year.parse().unwrap_or(0);
        let year_full = match year.len() {
            2 if (20..=40).contains(&year_num) => Some(2000 + year_num),
            4 if (2015..=2040).contains(&year_num) => Some(year_num),
            _ => None,
        };
        if let Some(year_full) = year_full {
            let j = skip_ws(after_year);
            let separator_len = usize::from(chars.get(j) == Some(&'년') || (matches!(chars.get(j), Some('.' | '/' | '-')) && year.len() == 4));
            if separator_len == 1 {
                let k = skip_ws(j + 1);
                if k < chars.len() && chars[k].is_ascii_digit() {
                    let (month, after_month) = digits_at(k);
                    let month_num: u32 = month.parse().unwrap_or(0);
                    let is_month_word = chars.get(skip_ws(after_month)) == Some(&'월');
                    if (1..=12).contains(&month_num) && month.len() <= 2 && (is_month_word || chars[j] != '년') {
                        return Some(format!("{year_full}년 {month_num}월"));
                    }
                }
            }
        }
        // "3월 발매" / "3월 입고"
        if year.len() <= 2 && (1..=12).contains(&year_num) {
            let j = skip_ws(after_year);
            if chars.get(j) == Some(&'월') {
                let rest: String = chars[j + 1..].iter().take(6).collect();
                if ["발매", "입고", "출시", "예약", "중순", "하순", "초", "말"].iter().any(|w| rest.trim_start().starts_with(w)) {
                    return Some(format!("{year_num}월"));
                }
            }
        }
        i = after_year.max(i + 1);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat(name: &str) -> Category {
        classify(name, "", StoreKind::Shop).category
    }

    #[test]
    fn categorizes_common_lines() {
        assert_eq!(cat("[예약] 넨도로이드 하츠네 미쿠 2301"), Category::Nendoroid);
        assert_eq!(cat("넨도로이드 돌 아냐 포저"), Category::NendoroidDoll);
        assert_eq!(cat("넨도로이드 모어 교체 얼굴 파츠"), Category::Accessory);
        assert_eq!(cat("figma 579 링크"), Category::Figma);
        assert_eq!(cat("POP UP PARADE 하츠네 미쿠"), Category::PopUpParade);
        assert_eq!(cat("初音ミク 1/7 スケールフィギュア"), Category::ScaleFigure);
        assert_eq!(cat("하츠네 미쿠 1/7 완성품 피규어"), Category::ScaleFigure);
        assert_eq!(cat("RG 1/144 건담 RX-78-2"), Category::Plamo);
        assert_eq!(cat("HG 건담 에어리얼"), Category::Plamo);
        assert_eq!(cat("S.H.Figuarts 손오공"), Category::ActionFigure);
        assert_eq!(cat("반프레스토 원피스 루피 프라이즈 피규어"), Category::PrizeFigure);
        assert_eq!(cat("이치방쿠지 귀멸의 칼날 A상"), Category::Kuji);
        assert_eq!(cat("블루아카이브 아크릴 스탠드 아리스"), Category::Goods);
        assert_eq!(cat("하츠네 미쿠 봉제 인형"), Category::Plush);
        assert_eq!(cat("포켓몬 캡슐토이 1BOX"), Category::Gacha);
        assert_eq!(cat("미쿠 피규어"), Category::Figure);
        assert_eq!(cat("미쿠"), Category::Other);
        assert_eq!(classify("주술회전 1", "", StoreKind::Bookstore).category, Category::Book);
        assert_eq!(cat("image of shg"), Category::Other, "ascii words need boundaries");
    }

    #[test]
    fn tags_status_from_name_badges_and_store() {
        let c = classify("[예약] 넨도로이드 미쿠 (특전 포함)", "", StoreKind::Shop);
        assert_eq!(c.tags, vec![Tag::Preorder, Tag::Bonus]);
        assert!(classify("넨도로이드 미쿠", "예약", StoreKind::Shop).tags.contains(&Tag::Preorder));
        assert!(classify("넨도로이드 미쿠", "", StoreKind::Marketplace).tags.contains(&Tag::Used));
        assert!(classify("미쿠 피규어 중국판", "", StoreKind::Shop).tags.contains(&Tag::Bootleg));
        assert!(classify("figma 미쿠 재판", "", StoreKind::Shop).tags.contains(&Tag::Reissue));
    }

    #[test]
    fn extracts_release_month() {
        assert_eq!(release_month("[26년 3월 발매] 넨도로이드"), Some("2026년 3월".into()));
        assert_eq!(release_month("넨도로이드 2026.11 입고"), Some("2026년 11월".into()));
        assert_eq!(release_month("[5월 입고예정] figma"), Some("5월".into()));
        assert_eq!(release_month("넨도로이드 2301"), None);
        assert_eq!(release_month("1/7 스케일"), None);
    }
}
