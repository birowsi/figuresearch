//! FigureSearch search core.
//!
//! Turns a store's search response into relevant, categorized products. This
//! crate has no dependencies so it can be tested without the Tauri toolchain;
//! networking and character decoding live in the app crate.

pub mod classify;
pub mod extract;
pub mod group;
pub mod html;
pub mod normalize;
pub mod query;
pub mod url;

pub use classify::{Category, StoreKind, Tag};
pub use extract::{Platform, RawProduct};
pub use query::{Analysis, Language};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Availability {
    InStock,
    SoldOut,
    Unknown,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct Product {
    pub id: String,
    pub store: String,
    pub name: String,
    pub url: Option<String>,
    pub price: Option<u64>,
    pub image_url: Option<String>,
    pub availability: Availability,
    pub category: Category,
    pub tags: Vec<Tag>,
    pub release: Option<String>,
    /// 0.0–1.0 share of query words found in the name.
    pub relevance: f32,
    /// Passed the relevance rules; irrelevant products are hidden by default.
    pub relevant: bool,
    /// Query words missing from the name (for "why was this hidden?").
    pub missing: Vec<String>,
    pub query: String,
    /// Equal for listings of the same product across stores; empty if unknown.
    pub group_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PageStatus {
    /// Relevant products found.
    Results,
    /// Products found, none matching the query.
    NoRelevant,
    /// The store said there were no results.
    Empty,
    /// Page loaded but no product cards could be read.
    Unparsed,
    LoginRequired,
    RateLimited,
    Blocked,
    HttpError,
    NetworkError,
    /// Not fetched in-app (e.g. Naver stores); open in a browser instead.
    LinkOnly,
    /// Skipped by configuration (e.g. EUC-KR cannot encode the query).
    Skipped,
}

impl PageStatus {
    /// Whether retrying with another query candidate could help.
    pub fn worth_retry(self) -> bool {
        matches!(self, PageStatus::NoRelevant | PageStatus::Empty | PageStatus::Unparsed)
    }
}

pub fn store_kind(platform: Platform) -> StoreKind {
    match platform {
        Platform::Aladin | Platform::Yes24 => StoreKind::Bookstore,
        Platform::Bunjang => StoreKind::Marketplace,
        _ => StoreKind::Shop,
    }
}

/// Scores and classifies raw products. Returns products sorted best first.
pub fn evaluate(store: &str, query_used: &str, analysis: &Analysis, raws: Vec<RawProduct>, kind: StoreKind) -> Vec<Product> {
    let mut products: Vec<Product> = raws
        .into_iter()
        .filter(|raw| !raw.name.trim().is_empty())
        .map(|raw| {
            let relevance = query::relevance(analysis, &raw.name);
            let classification = classify::classify(&raw.name, &raw.badges, kind);
            let availability = if raw.sold_out {
                Availability::SoldOut
            } else if raw.price.is_some() {
                Availability::InStock
            } else {
                Availability::Unknown
            };
            let group_key = group::group_key(&raw.name, &classification.tags);
            let id = format!("{store}:{}", raw.url.as_deref().unwrap_or(&raw.name)).to_lowercase();
            Product {
                id,
                store: store.to_string(),
                name: raw.name,
                url: raw.url,
                price: raw.price,
                image_url: raw.image,
                availability,
                category: classification.category,
                tags: classification.tags,
                release: classification.release,
                relevance: relevance.score,
                relevant: relevance.passed,
                missing: relevance.missing,
                query: query_used.to_string(),
                group_key,
            }
        })
        .collect();
    sort_products(&mut products);
    products
}

/// Relevant first, then by score, in-stock before sold-out, then cheaper.
pub fn sort_products(products: &mut [Product]) {
    let availability_rank = |a: Availability| match a {
        Availability::InStock => 0,
        Availability::Unknown => 1,
        Availability::SoldOut => 2,
    };
    products.sort_by(|a, b| {
        b.relevant
            .cmp(&a.relevant)
            .then(b.relevance.partial_cmp(&a.relevance).unwrap_or(std::cmp::Ordering::Equal))
            .then(availability_rank(a.availability).cmp(&availability_rank(b.availability)))
            .then(a.price.unwrap_or(u64::MAX).cmp(&b.price.unwrap_or(u64::MAX)))
    });
}

#[derive(Debug, Clone)]
pub struct PageOutcome {
    pub status: PageStatus,
    pub products: Vec<Product>,
    pub product_links: usize,
    pub scoped: bool,
}

impl PageOutcome {
    pub fn relevant_count(&self) -> usize {
        self.products.iter().filter(|p| p.relevant).count()
    }
}

/// Full pipeline for an HTML search page.
pub fn process_html(
    store: &str,
    platform: Platform,
    analysis: &Analysis,
    query_used: &str,
    http_status: u16,
    final_url: &str,
    html: &str,
) -> PageOutcome {
    if http_status == 429 {
        return PageOutcome { status: PageStatus::RateLimited, products: vec![], product_links: 0, scoped: false };
    }
    let extraction = extract::extract(html, final_url, platform);
    let products = evaluate(store, query_used, analysis, extraction.products, store_kind(platform));
    let relevant = products.iter().filter(|p| p.relevant).count();
    let status = if relevant > 0 {
        PageStatus::Results
    } else if !products.is_empty() {
        PageStatus::NoRelevant
    } else {
        match extraction.signal {
            extract::PageSignal::BotChallenge => PageStatus::Blocked,
            extract::PageSignal::LoginPage => PageStatus::LoginRequired,
            _ if matches!(http_status, 401 | 403) => PageStatus::Blocked,
            _ if http_status >= 400 => PageStatus::HttpError,
            extract::PageSignal::EmptyMessage => PageStatus::Empty,
            _ if html.trim().is_empty() => PageStatus::Empty,
            _ => PageStatus::Unparsed,
        }
    };
    PageOutcome { status, products, product_links: extraction.product_links, scoped: extraction.scoped }
}

/// Detects the character set from a Content-Type header or a `<meta>` tag.
pub fn sniff_charset(content_type: Option<&str>, head: &[u8]) -> Option<String> {
    let from_header = content_type.and_then(|ct| {
        ct.split(';')
            .map(str::trim)
            .find_map(|part| part.to_ascii_lowercase().strip_prefix("charset=").map(|v| v.trim_matches(['"', '\'']).to_string()))
    });
    if from_header.is_some() {
        return from_header;
    }
    let sample = &head[..head.len().min(4096)];
    let text: String = sample.iter().map(|&b| if b.is_ascii() { (b as char).to_ascii_lowercase() } else { ' ' }).collect();
    let pos = text.find("charset=")?;
    let value: String = text[pos + 8..]
        .trim_start_matches(['"', '\''])
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    (!value.is_empty()).then_some(value)
}
