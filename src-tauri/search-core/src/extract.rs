//! Finds product cards in store search pages.
//!
//! Strategy:
//! 1. Recognise product-detail links by URL shape (Cafe24, 고도몰, 메이크샵,
//!    영카트, 아임웹, 알라딘, 예스24 and generic patterns) and reduce each to a
//!    stable product key so duplicate links collapse.
//! 2. Scope to the platform's search-result container when present; otherwise
//!    ignore links inside headers, menus, banners and "recommended" widgets.
//! 3. For each product, the card is the largest ancestor that contains links to
//!    that product only. Name, price, image, sold-out state and badges are read
//!    from the card.

use crate::html::{Document, NodeId, NodeKind};
use crate::url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Platform {
    Cafe24,
    Godomall,
    MakeShop,
    Youngcart,
    Imweb,
    Aladin,
    Yes24,
    NaverStore,
    Bunjang,
    Generic,
}

impl Platform {
    pub fn label(self) -> &'static str {
        match self {
            Platform::Cafe24 => "cafe24",
            Platform::Godomall => "godomall",
            Platform::MakeShop => "makeshop",
            Platform::Youngcart => "youngcart",
            Platform::Imweb => "imweb",
            Platform::Aladin => "aladin",
            Platform::Yes24 => "yes24",
            Platform::NaverStore => "naver_store",
            Platform::Bunjang => "bunjang",
            Platform::Generic => "generic",
        }
    }

    pub fn from_label(label: &str) -> Option<Platform> {
        Some(match label.to_ascii_lowercase().as_str() {
            "cafe24" => Platform::Cafe24,
            "godomall" | "godo" => Platform::Godomall,
            "makeshop" => Platform::MakeShop,
            "youngcart" => Platform::Youngcart,
            "imweb" => Platform::Imweb,
            "aladin" => Platform::Aladin,
            "yes24" => Platform::Yes24,
            "naver_store" | "naver" => Platform::NaverStore,
            "bunjang" => Platform::Bunjang,
            "generic" => Platform::Generic,
            _ => return None,
        })
    }
}

pub fn detect_platform(template: &str) -> Platform {
    let host = url::host(template);
    let lower = template.to_ascii_lowercase();
    if host.ends_with("smartstore.naver.com") || host.ends_with("brand.naver.com") || host == "shopping.naver.com" {
        Platform::NaverStore
    } else if host.ends_with("bunjang.co.kr") {
        Platform::Bunjang
    } else if host.ends_with("aladin.co.kr") {
        Platform::Aladin
    } else if host.ends_with("yes24.com") {
        Platform::Yes24
    } else if lower.contains("/product/search.html") {
        Platform::Cafe24
    } else if lower.contains("goods_search.php") || lower.contains("goods_list.php") {
        Platform::Godomall
    } else if lower.contains("shopbrand.html") || lower.contains("/shop/search_result") {
        Platform::MakeShop
    } else if lower.contains("/shop/search.php") {
        Platform::Youngcart
    } else if lower.contains("shopsearch") || lower.contains("shop_search") {
        Platform::Imweb
    } else {
        Platform::Generic
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawProduct {
    pub name: String,
    pub url: Option<String>,
    pub price: Option<u64>,
    pub image: Option<String>,
    pub sold_out: bool,
    /// Badge/icon text ("예약", "품절", "NEW") used for classification.
    pub badges: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageSignal {
    None,
    EmptyMessage,
    BotChallenge,
    LoginPage,
}

#[derive(Debug, Clone)]
pub struct Extraction {
    pub products: Vec<RawProduct>,
    pub signal: PageSignal,
    /// Number of distinct product links seen anywhere on the page.
    pub product_links: usize,
    pub scoped: bool,
}

/// Stable identity for a product-detail URL, or None if the URL is not one.
pub fn product_key(link: &str) -> Option<String> {
    let lower = link.to_ascii_lowercase();
    let path = url::path(&lower).to_string();
    let param = |k: &str| url::query_param(&lower, k).filter(|v| !v.is_empty()).map(str::to_string);
    let digits = |v: &str| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit());

    if path.contains("/board/") || path.contains("/myshop/") || path.contains("/member/") || path.contains("/order/") {
        return None;
    }
    if path.ends_with("goods_view.php") {
        return param("goodsno").map(|v| format!("goods:{v}"));
    }
    if path.ends_with("shopdetail.html") {
        return param("branduid").map(|v| format!("makeshop:{v}"));
    }
    if path.ends_with("/item.php") {
        return param("it_id").map(|v| format!("youngcart:{v}"));
    }
    if path.contains("wproduct.aspx") {
        return param("itemid").map(|v| format!("aladin:{v}"));
    }
    if path.contains("shop_view") {
        return param("idx").map(|v| format!("imweb:{v}"));
    }
    if path.ends_with("/product/detail.html") {
        return param("product_no").map(|v| format!("product:{v}"));
    }
    if path.contains("/goods/view") {
        return param("no").map(|v| format!("goods:{v}"));
    }
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if let Some(pos) = segments.iter().position(|s| matches!(*s, "product" | "products" | "goods" | "item" | "items" | "detail")) {
        let rest = &segments[pos + 1..];
        let blocked = ["search.html", "list.html", "recent_view_product.html", "compare.html", "search", "list"];
        if rest.first().is_some_and(|s| blocked.contains(s)) {
            return None;
        }
        if let Some(id) = rest.iter().take(3).find(|s| digits(s)) {
            return Some(format!("product:{id}"));
        }
    }
    for key in ["product_no", "goodsno", "goods_no", "it_id", "item_id", "itemid", "itemno", "prd_no", "pid", "pno", "branduid"] {
        if let Some(v) = param(key).filter(|v| v.chars().any(|c| c.is_ascii_digit())) {
            return Some(format!("product:{v}"));
        }
    }
    None
}

/// Where clicking this element goes: a real `href`, or `location.href='…'` in
/// an `onclick` (shops that render product names as clickable `<dd>`/`<div>`).
fn element_link(doc: &Document, id: NodeId) -> Option<&str> {
    if let Some(href) = doc.attr(id, "href").filter(|_| matches!(doc.tag(id), Some("a" | "area"))).map(str::trim) {
        let lower = href.to_ascii_lowercase();
        if !href.is_empty() && !href.starts_with('#') && !lower.starts_with("javascript:") {
            return Some(href);
        }
    }
    let onclick = doc.attr(id, "onclick")?;
    let after = &onclick[onclick.find("location.href")? + "location.href".len()..];
    let after = after.trim_start().strip_prefix('=')?.trim_start();
    let quote = after.chars().next().filter(|c| matches!(c, '\'' | '"'))?;
    let rest = &after[1..];
    rest.find(quote).map(|end| &rest[..end])
}

fn same_site(a: &str, b: &str) -> bool {
    let (ha, hb) = (url::host(a), url::host(b));
    !ha.is_empty() && (ha == hb || ha.ends_with(&format!(".{hb}")) || hb.ends_with(&format!(".{ha}")))
}

/// Containers that hold the real search results for known platforms.
fn is_result_container(doc: &Document, id: NodeId, platform: Platform) -> bool {
    let classes: Vec<String> = doc.classes(id).map(str::to_ascii_lowercase).collect();
    let element_id = doc.attr(id, "id").unwrap_or("").to_ascii_lowercase();
    let class_has = |needle: &str| classes.iter().any(|c| c.contains(needle));
    match platform {
        Platform::Cafe24 => class_has("xans-search-result"),
        Platform::Godomall => {
            classes.iter().any(|c| c == "goods_list_item" || c == "item_gallery_type" || c == "item_list_type")
                || element_id == "goodslist"
        }
        Platform::Aladin => element_id == "search3_result",
        Platform::Yes24 => element_id == "yesschlist",
        _ => false,
    }
}

const EXCLUDE_EXACT: &[&str] = &["nav", "best", "rank", "side", "top", "left", "right"];
const EXCLUDE_CONTAINS: &[&str] = &[
    "header", "gnb", "lnb", "snb", "footer", "menu", "aside", "quick", "recent", "todayview", "today_view",
    "recommend", "related", "banner", "popup", "rolling", "navigation", "cart", "wish", "ranking",
    "sidebar", "slide", "swiper", "bestitem", "best_item", "best-item", "mdpick", "md_pick", "popular",
];

fn in_excluded_zone(doc: &Document, id: NodeId) -> bool {
    doc.ancestors(id).any(|a| {
        if matches!(doc.tag(a), Some("header" | "footer" | "nav" | "aside")) {
            return true;
        }
        let id_attr = doc.attr(a, "id").unwrap_or("").to_ascii_lowercase();
        let mut names: Vec<String> = doc.classes(a).map(str::to_ascii_lowercase).collect();
        if !id_attr.is_empty() {
            names.push(id_attr);
        }
        names.iter().any(|n| EXCLUDE_EXACT.contains(&n.as_str()) || EXCLUDE_CONTAINS.iter().any(|x| n.contains(x)))
    })
}

#[derive(Clone, PartialEq)]
enum Owner {
    Empty,
    One(String),
    Many,
}

pub fn page_signal(doc: &Document, final_url: &str) -> PageSignal {
    let text = doc.text(0).to_lowercase();
    let lower_url = final_url.to_ascii_lowercase();
    const BOT: &[&str] = &[
        "captcha", "자동입력 방지", "자동 입력 방지", "보안문자", "robot check", "just a moment", "cf-chl",
        "비정상적인 접근", "접근이 차단", "access denied", "보안 확인",
        "보안절차를 거치고", "prove that you are human", "간단한 확인이 필요",
    ];
    const EMPTY: &[&str] = &[
        "검색 결과가 없습니다", "검색결과가 없습니다", "검색된 상품이 없습니다", "검색된 상품이 없어요",
        "찾으시는 상품이 없습니다", "상품이 없습니다", "검색 결과가 없어요", "결과가 없습니다", "no results",
        "등록된 상품이 없습니다", "일치하는 상품이 없습니다", "검색어와 일치하는",
        "검색결과 0개", "검색 결과 0개", "검색결과 0건", "검색 결과 0건", "검색결과(0)", "검색 결과(0)",
    ];
    if BOT.iter().any(|w| text.contains(w)) || url::path(&lower_url).contains("/challenge") {
        PageSignal::BotChallenge
    } else if url::host(&lower_url).starts_with("nid.naver.com")
        || ["login", "adult"].iter().any(|w| url::path(&lower_url).contains(w))
    {
        PageSignal::LoginPage
    } else if EMPTY.iter().any(|w| text.contains(w)) {
        PageSignal::EmptyMessage
    } else {
        PageSignal::None
    }
}

pub fn extract(html: &str, page_url: &str, platform: Platform) -> Extraction {
    let doc = Document::parse(html);
    let signal = page_signal(&doc, page_url);

    // 1. product links
    let mut link_keys: Vec<(NodeId, String, String)> = Vec::new(); // (anchor, key, absolute url)
    for id in doc.elements() {
        let Some(href) = element_link(&doc, id) else { continue };
        let Some(abs) = url::resolve(page_url, href) else { continue };
        if !same_site(&abs, page_url) {
            continue;
        }
        if let Some(key) = product_key(&abs) {
            link_keys.push((id, key, abs));
        }
    }
    let mut distinct: Vec<&str> = link_keys.iter().map(|(_, k, _)| k.as_str()).collect();
    distinct.sort_unstable();
    distinct.dedup();
    let product_links = distinct.len();

    // 2. scope
    let containers: Vec<NodeId> = doc.elements().filter(|&id| is_result_container(&doc, id, platform)).collect();
    let within = |anchor: NodeId, container: NodeId| doc.ancestors(anchor).any(|a| a == container);
    let scoped_links: Vec<&(NodeId, String, String)> = if !containers.is_empty() {
        link_keys.iter().filter(|(a, _, _)| containers.iter().any(|&c| within(*a, c))).collect()
    } else {
        Vec::new()
    };
    let scoped = !scoped_links.is_empty();
    let links: Vec<&(NodeId, String, String)> = if scoped {
        scoped_links
    } else {
        link_keys.iter().filter(|(a, _, _)| !in_excluded_zone(&doc, *a)).collect()
    };

    // 3. ownership: which product keys live under each element
    let mut owner = vec![Owner::Empty; doc.nodes.len()];
    for (anchor, key, _) in &link_keys {
        for node in std::iter::once(*anchor).chain(doc.ancestors(*anchor)) {
            owner[node] = match &owner[node] {
                Owner::Empty => Owner::One(key.clone()),
                Owner::One(k) if k == key => continue,
                _ => Owner::Many,
            };
        }
    }

    let mut seen: Vec<String> = Vec::new();
    let mut products = Vec::new();
    for (anchor, key, abs) in links {
        if seen.contains(key) {
            continue;
        }
        let mut card = *anchor;
        while let Some(parent) = doc.nodes[card].parent {
            if parent == 0 || matches!(doc.tag(parent), Some("body" | "html")) || owner[parent] != Owner::One(key.clone()) {
                break;
            }
            card = parent;
        }
        if let Some(product) = read_card(&doc, card, key, abs, page_url) {
            seen.push(key.clone());
            products.push(product);
        }
    }

    Extraction { products, signal, product_links, scoped }
}

const NAME_CLASSES: &[&str] = &[
    "name", "prd_name", "prdname", "item_name", "goods_name", "gd_name", "product_name", "prd-name",
    "goods-name", "item_tit", "item_tit_box", "tit", "subject", "bo3", "title",
];
const LABEL_WORDS: &[&str] = &[
    "상품명", "판매가", "소비자가", "할인", "적립", "배송", "리뷰", "상품요약", "옵션", "수량", "브랜드", "제조사",
];

fn has_letters(s: &str) -> bool {
    s.chars().any(|c| c.is_alphabetic())
}

fn clean_name(raw: &str) -> String {
    let mut name = crate::html::collapse_ws(raw);
    for prefix in ["상품명 :", "상품명:", "상품명"] {
        if let Some(rest) = name.strip_prefix(prefix) {
            name = rest.trim().to_string();
        }
    }
    // Drop a trailing price ("... ₩55,000" / "... 55,000원") that leaked into link text.
    if let Some((start, _, _)) = find_prices(&name).first()
        && *start > 0 {
            name = name[..*start].trim_end_matches([' ', '-', ':', '|', '/']).to_string();
        }
    name.trim().to_string()
}

fn looks_like_label(text: &str) -> bool {
    let compact: String = text.chars().filter(|c| !c.is_whitespace() && *c != ':').collect();
    LABEL_WORDS.iter().any(|w| compact == *w || compact.starts_with(w) && compact.chars().count() <= w.chars().count() + 2)
}

fn read_name(doc: &Document, card: NodeId, key: &str, page_url: &str) -> Option<String> {
    let nodes = doc.descendants(card);
    let mut best: Option<(usize, String)> = None;
    for &id in std::iter::once(&card).chain(nodes.iter()) {
        if doc.is_hidden(id) || doc.ancestors(id).take_while(|&a| a != card).any(|a| doc.is_hidden(a)) {
            continue;
        }
        let classes: Vec<String> = doc.classes(id).map(str::to_ascii_lowercase).collect();
        let Some(rank) = NAME_CLASSES.iter().position(|n| classes.iter().any(|c| c == n)) else { continue };
        let text = clean_name(&doc.text(id));
        if text.chars().count() < 2 || text.chars().count() > 300 || !has_letters(&text) || looks_like_label(&text) {
            continue;
        }
        if best.as_ref().is_none_or(|(r, _)| rank < *r) {
            best = Some((rank, text));
        }
    }
    if let Some((_, name)) = best {
        return Some(name);
    }
    // Longest visible text among this product's links.
    let mut link_text: Option<String> = None;
    for &id in std::iter::once(&card).chain(nodes.iter()) {
        let same = element_link(doc, id)
            .and_then(|h| url::resolve(page_url, h))
            .and_then(|u| product_key(&u))
            .is_some_and(|k| k == key);
        if !same {
            continue;
        }
        let text = clean_name(&doc.text(id));
        if has_letters(&text) && !looks_like_label(&text) && link_text.as_ref().is_none_or(|t| text.chars().count() > t.chars().count()) {
            link_text = Some(text);
        }
        if let Some(title) = doc.attr(id, "title").map(clean_name).filter(|t| has_letters(t))
            && link_text.is_none() {
                link_text = Some(title);
            }
    }
    if link_text.as_ref().is_some_and(|t| t.chars().count() >= 2) {
        return link_text;
    }
    nodes
        .iter()
        .filter(|&&id| doc.tag(id) == Some("img"))
        .find_map(|&id| doc.attr(id, "alt").or_else(|| doc.attr(id, "title")).map(clean_name).filter(|t| t.chars().count() >= 2 && has_letters(t)))
}

/// Prices inside a text: (byte start, value, has explicit currency marker).
fn find_prices(text: &str) -> Vec<(usize, u64, bool)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() || (i > 0 && (bytes[i - 1].is_ascii_digit() || bytes[i - 1] == b',' || bytes[i - 1] == b'.')) {
            i += 1;
            continue;
        }
        let start = i;
        let mut j = i;
        let mut digits = String::new();
        let mut grouped = false;
        while j < bytes.len() {
            if bytes[j].is_ascii_digit() {
                digits.push(bytes[j] as char);
                j += 1;
            } else if bytes[j] == b',' && bytes.get(j + 1).is_some_and(u8::is_ascii_digit) && bytes.get(j + 3).is_some_and(u8::is_ascii_digit) && !bytes.get(j + 4).is_some_and(u8::is_ascii_digit) {
                grouped = true;
                j += 1;
            } else {
                break;
            }
        }
        let before = text[..start].trim_end();
        let after = text[j..].trim_start();
        let won_before = before.ends_with('₩') || before.ends_with('￦') || before.to_ascii_lowercase().ends_with("krw");
        let won_after = after.starts_with('원') || after.to_ascii_lowercase().starts_with("krw");
        let marked = won_before || won_after;
        if let Ok(value) = digits.parse::<u64>() {
            let plausible = (100..=50_000_000).contains(&value) && (grouped || digits.len() >= 3);
            if plausible && (marked || grouped) {
                let price_start = if won_before { before.char_indices().next_back().map(|(p, _)| p).unwrap_or(start) } else { start };
                out.push((price_start, value, marked));
            }
        }
        i = j.max(i + 1);
    }
    out
}

const STRUCK_CLASSES: &[&str] = &[
    "strike", "consumer", "origin", "before", "retail", "line-through", "normal_price", "price_old",
    "old_price", "list_price", "fixed_price", "custom", "ori_price", "price_ori", "regular",
];
const EXCLUDED_CONTEXT: &[&str] = &["적립", "포인트", "마일리지", "배송비", "배송료", "할부", "쿠폰", "무료배송", "이상 구매"];
const CONSUMER_CONTEXT: &[&str] = &["소비자가", "정가", "시중가", "권장소비자"];

fn read_price(doc: &Document, card: NodeId) -> Option<u64> {
    let mut candidates: Vec<(u64, bool, bool)> = Vec::new(); // (value, discount-labelled, emphasized)
    let elements: Vec<NodeId> = std::iter::once(card).chain(doc.descendants(card)).filter(|&id| doc.tag(id).is_some()).collect();
    for &id in &elements {
        if doc.is_hidden(id) || doc.ancestors(id).take_while(|&a| a != card).any(|a| doc.is_hidden(a)) {
            continue;
        }
        let text = doc.text(id);
        if text.is_empty() || text.chars().count() > 60 {
            continue;
        }
        let prices = find_prices(&text);
        if prices.is_empty() {
            continue;
        }
        // Only the deepest element holding the price.
        let child_has = doc.nodes[id].children.iter().any(|&c| {
            matches!(doc.nodes[c].kind, NodeKind::Element { .. }) && !find_prices(&doc.text(c)).is_empty()
        });
        if child_has {
            continue;
        }
        let chain: Vec<NodeId> = std::iter::once(id).chain(doc.ancestors(id).take_while(|&a| a != card)).collect();
        let struck = chain.iter().any(|&a| {
            matches!(doc.tag(a), Some("del" | "s" | "strike"))
                || doc.classes(a).any(|c| STRUCK_CLASSES.iter().any(|s| c.to_ascii_lowercase().contains(s)))
                || doc.attr(a, "style").is_some_and(|s| s.contains("line-through"))
        });
        let context = doc.nodes[id]
            .parent
            .map(|p| doc.text(p))
            .filter(|t| t.chars().count() <= 150)
            .unwrap_or_else(|| text.clone());
        let consumer = CONSUMER_CONTEXT.iter().any(|w| context.contains(w)) && !context.contains("판매가");
        let excluded = EXCLUDED_CONTEXT.iter().any(|w| text.contains(w) || (context.contains(w) && !context.contains("판매가")));
        if struck || consumer || excluded {
            continue;
        }
        let discount = context.contains("할인") || context.to_ascii_lowercase().contains("sale");
        // Table layouts put an unlabeled points column next to the bold sale price.
        let emphasized = chain.iter().any(|&a| matches!(doc.tag(a), Some("b" | "strong")));
        for (_, value, _) in prices {
            candidates.push((value, discount, emphasized));
        }
    }
    if let Some(&(value, ..)) = candidates.iter().filter(|(_, d, _)| *d).min_by_key(|(v, ..)| *v) {
        return Some(value);
    }
    if let Some(&(value, ..)) = candidates.iter().find(|(_, _, e)| *e).or(candidates.first()) {
        return Some(value);
    }
    // Cafe24 / misc data attributes
    for &id in &elements {
        for attr in ["ec-data-price", "data-price", "data-sale-price", "data-sale_price"] {
            if let Some(v) = doc.attr(id, attr).and_then(|v| v.replace(',', "").split('.').next().and_then(|n| n.parse::<u64>().ok()))
                && (100..=50_000_000).contains(&v) {
                    return Some(v);
                }
        }
    }
    None
}

fn is_icon(doc: &Document, img: NodeId, src: &str) -> bool {
    let lower = src.to_ascii_lowercase();
    doc.classes(img).any(|c| c.to_ascii_lowercase().contains("icon"))
        || ["/icon", "icon_", "soldout", "sold_out", "blank.gif", "spacer", "loading", "noimage", "btn_", "/btn"].iter().any(|p| lower.contains(p))
        || doc.ancestors(img).take(2).any(|a| doc.classes(a).any(|c| { let c = c.to_ascii_lowercase(); c.contains("icon") || c.contains("badge") }))
}

fn read_image(doc: &Document, card: NodeId, page_url: &str) -> Option<String> {
    for id in doc.descendants(card) {
        if doc.tag(id) != Some("img") {
            continue;
        }
        let src = ["ec-data-src", "data-original", "data-src", "data-lazy", "data-lazy-src", "src"]
            .iter()
            .filter_map(|a| doc.attr(id, a))
            .find(|v| !v.trim().is_empty() && !v.starts_with("data:"));
        let Some(src) = src else { continue };
        if is_icon(doc, id, src) {
            continue;
        }
        return url::resolve(page_url, src);
    }
    None
}

const SOLD_OUT_WORDS: &[&str] = &["품절", "일시품절", "sold out", "soldout", "재고 없음", "재고없음", "판매종료", "판매 종료", "out of stock"];

fn read_status(doc: &Document, card: NodeId) -> (bool, String) {
    let mut sold_out = false;
    let mut badges = Vec::new();
    for id in std::iter::once(card).chain(doc.descendants(card)) {
        if doc.tag(id).is_none() || doc.is_hidden(id) || doc.ancestors(id).take_while(|&a| a != card).any(|a| doc.is_hidden(a)) {
            continue;
        }
        let classes: Vec<String> = doc.classes(id).map(str::to_ascii_lowercase).collect();
        if classes.iter().any(|c| c.contains("soldout") || c.contains("sold_out") || c.contains("sold-out")) {
            sold_out = true;
        }
        if doc.tag(id) == Some("img") {
            let alt = doc.attr(id, "alt").unwrap_or("");
            let src = doc.attr(id, "src").unwrap_or("").to_ascii_lowercase();
            if SOLD_OUT_WORDS.iter().any(|w| alt.to_lowercase().contains(w)) || src.contains("soldout") || src.contains("sold_out") {
                sold_out = true;
            }
            if is_icon(doc, id, &src) && !alt.is_empty() {
                badges.push(alt.to_string());
            }
        } else if classes.iter().any(|c| ["icon", "badge", "label", "flag", "tag", "sticker"].iter().any(|b| c.contains(b))) {
            let text = doc.text(id);
            if !text.is_empty() && text.chars().count() <= 40 {
                badges.push(text);
            }
        }
    }
    let visible = doc.text(card).to_lowercase();
    if SOLD_OUT_WORDS.iter().any(|w| visible.contains(w)) {
        sold_out = true;
    }
    (sold_out, badges.join(" "))
}

fn read_card(doc: &Document, card: NodeId, key: &str, abs: &str, page_url: &str) -> Option<RawProduct> {
    let name = read_name(doc, card, key, page_url)?;
    let (sold_out, badges) = read_status(doc, card);
    Some(RawProduct {
        name,
        url: Some(abs.to_string()),
        price: read_price(doc, card),
        image: read_image(doc, card, page_url),
        sold_out,
        badges,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_keys_cover_known_platforms() {
        assert_eq!(product_key("https://a.kr/product/넨도-미쿠/1234/category/1/display/1/").as_deref(), Some("product:1234"));
        assert_eq!(product_key("https://a.kr/product/detail.html?product_no=55&cate_no=1").as_deref(), Some("product:55"));
        assert_eq!(product_key("https://a.kr/goods/goods_view.php?goodsNo=1000001").as_deref(), Some("goods:1000001"));
        assert_eq!(product_key("https://a.kr/shop/shopdetail.html?branduid=99&xcode=1").as_deref(), Some("makeshop:99"));
        assert_eq!(product_key("https://a.kr/shop/item.php?it_id=1650000000").as_deref(), Some("youngcart:1650000000"));
        assert_eq!(product_key("https://www.aladin.co.kr/shop/wproduct.aspx?ItemId=123").as_deref(), Some("aladin:123"));
        assert_eq!(product_key("https://www.yes24.com/Product/Goods/1234").as_deref(), Some("product:1234"));
        assert_eq!(product_key("https://a.kr/product/search.html?keyword=x"), None);
        assert_eq!(product_key("https://a.kr/board/product/read.html?no=1&product_no=3"), None);
        assert_eq!(product_key("https://a.kr/goods/goods_list.php?cateCd=001"), None);
        assert_eq!(product_key("https://a.kr/shop/detail.php?pno=E836B7&ctype=1").as_deref(), Some("product:e836b7"));
        assert_eq!(product_key("https://a.kr/mall/Itemdetails.php?cate=&itemno=1376428277").as_deref(), Some("product:1376428277"));
    }

    #[test]
    fn finds_prices_with_markers() {
        let p = |s: &str| find_prices(s).into_iter().map(|(_, v, _)| v).collect::<Vec<_>>();
        assert_eq!(p("판매가 : 55,000원"), vec![55000]);
        assert_eq!(p("₩ 1,200"), vec![1200]);
        assert_eq!(p("넨도로이드 2301"), Vec::<u64>::new());
        assert_eq!(p("12,000 원 → 9,900원"), vec![12000, 9900]);
        assert_eq!(p("1/7 스케일"), Vec::<u64>::new());
    }

    #[test]
    fn detects_platform_from_template() {
        assert_eq!(detect_platform("https://figurepresso.com/product/search.html?banner_action=&keyword={input}"), Platform::Cafe24);
        assert_eq!(detect_platform("https://www.comiczone.co.kr/goods/goods_search.php?keyword={input}"), Platform::Godomall);
        assert_eq!(detect_platform("https://smartstore.naver.com/x/search?q={input}"), Platform::NaverStore);
        assert_eq!(detect_platform("https://m.bunjang.co.kr/search/products?q={input}"), Platform::Bunjang);
        assert_eq!(detect_platform("https://www.comicct.com/shop/search_result.php?search_str={input}"), Platform::MakeShop);
    }
}
