//! Tauri backend: fetches store pages and turns them into products using
//! `search-core`. Results stream to the UI per store via the `search-store`
//! event so one slow or failing store never blocks the others.

use encoding_rs::{EUC_KR, Encoding, UTF_8};
use reqwest::header::{ACCEPT, ACCEPT_LANGUAGE, CONTENT_TYPE, HeaderMap, HeaderValue};
use search_core::extract::{Platform, detect_platform};
use search_core::query::{Analysis, analyze, query_candidates};
use search_core::{Language, PageStatus, Product, RawProduct, evaluate, process_html, store_kind, url};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

const WORKERS: usize = 6;
/// Irrelevant products kept per store for the "hidden results" view.
const MAX_HIDDEN_PER_STORE: usize = 30;
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36";

// ---------------------------------------------------------------- sites

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SiteEncoding {
    Utf8,
    EucKr,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SiteProfile {
    /// "UTF-8" (send UTF-8 when EUC-KR cannot encode the query) or "skip".
    euc_kr_fallback: Option<String>,
    preferred_language: Option<Language>,
}

#[derive(Debug, Clone, Deserialize)]
struct SiteConfig {
    name: String,
    url: String,
    #[serde(default)]
    platform: Option<String>,
    #[serde(default)]
    profile: Option<SiteProfile>,
}

#[derive(Debug, Clone)]
struct Site {
    name: String,
    url: String,
    encoding: SiteEncoding,
    platform: Platform,
    profile: SiteProfile,
}

fn sites() -> &'static [Site] {
    static SITES: OnceLock<Vec<Site>> = OnceLock::new();
    SITES.get_or_init(|| {
        let groups: HashMap<String, Vec<SiteConfig>> =
            serde_json::from_str(include_str!("../../sites.json")).expect("sites.json is invalid");
        groups
            .into_iter()
            .flat_map(|(group, configs)| {
                let encoding = if group.eq_ignore_ascii_case("EUC-KR") { SiteEncoding::EucKr } else { SiteEncoding::Utf8 };
                configs.into_iter().map(move |c| Site {
                    platform: c.platform.as_deref().and_then(Platform::from_label).unwrap_or_else(|| detect_platform(&c.url)),
                    name: c.name,
                    url: c.url,
                    encoding,
                    profile: c.profile.unwrap_or_default(),
                })
            })
            .collect()
    })
}

// ---------------------------------------------------------------- http

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("text/html,application/xhtml+xml,application/xml;q=0.9,application/json;q=0.8,*/*;q=0.7"));
        headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("ko-KR,ko;q=0.9,ja;q=0.6,en;q=0.5"));
        reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .default_headers(headers)
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(8))
            .redirect(reqwest::redirect::Policy::limited(6))
            .build()
            .expect("failed to build HTTP client")
    })
}

struct Fetched {
    status: u16,
    final_url: String,
    body: String,
}

async fn fetch(target: &str) -> Result<Fetched, String> {
    let mut last_error = String::new();
    for attempt in 0..2 {
        match client().get(target).send().await {
            Ok(response) => {
                let status = response.status().as_u16();
                let final_url = response.url().to_string();
                let content_type = response.headers().get(CONTENT_TYPE).and_then(|v| v.to_str().ok()).map(str::to_owned);
                let bytes = response.bytes().await.map_err(|e| e.to_string())?;
                let charset = search_core::sniff_charset(content_type.as_deref(), &bytes);
                let encoding = charset.as_deref().and_then(|c| Encoding::for_label(c.as_bytes())).unwrap_or(UTF_8);
                let (body, _, _) = encoding.decode(&bytes);
                return Ok(Fetched { status, final_url, body: body.into_owned() });
            }
            Err(error) => {
                last_error = describe_error(&error);
                // Retry once only for transient connection problems.
                if attempt == 0 && (error.is_connect() || error.is_timeout()) {
                    continue;
                }
                break;
            }
        }
    }
    Err(last_error)
}

fn describe_error(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "응답 시간 초과".into()
    } else if error.is_connect() {
        "연결 실패".into()
    } else {
        error.to_string()
    }
}

/// Percent-encodes the query for the site's charset. None = skip this store.
fn encode_query(site: &Site, query: &str) -> Option<(String, bool)> {
    match site.encoding {
        SiteEncoding::Utf8 => Some((url::percent_encode(query.as_bytes()), false)),
        SiteEncoding::EucKr => {
            let (bytes, _, had_errors) = EUC_KR.encode(query);
            if !had_errors {
                return Some((url::percent_encode(&bytes), false));
            }
            match site.profile.euc_kr_fallback.as_deref() {
                Some("skip") => None,
                _ => Some((url::percent_encode(query.as_bytes()), true)),
            }
        }
    }
}

// ---------------------------------------------------------------- results

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Attempt {
    query: String,
    url: String,
    status: PageStatus,
    http_status: Option<u16>,
    products: usize,
    relevant: usize,
    encoding_fallback: bool,
    duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoreResult {
    search_id: String,
    store: String,
    platform: &'static str,
    status: PageStatus,
    /// URL of the store's own search page, for "open in browser".
    search_url: Option<String>,
    products: Vec<Product>,
    relevant: usize,
    hidden: usize,
    message: Option<String>,
    attempts: Vec<Attempt>,
    duration_ms: u64,
}

impl StoreResult {
    fn new(search_id: &str, site: &Site) -> Self {
        StoreResult {
            search_id: search_id.to_string(),
            store: site.name.clone(),
            platform: site.platform.label(),
            status: PageStatus::Unparsed,
            search_url: None,
            products: vec![],
            relevant: 0,
            hidden: 0,
            message: None,
            attempts: vec![],
            duration_ms: 0,
        }
    }

    fn set_products(&mut self, mut products: Vec<Product>) {
        let relevant = products.iter().filter(|p| p.relevant).count();
        let hidden = products.len() - relevant;
        // products are sorted relevant-first; cap the irrelevant tail
        products.truncate(relevant + hidden.min(MAX_HIDDEN_PER_STORE));
        self.relevant = relevant;
        self.hidden = hidden;
        self.products = products;
    }
}

fn status_message(status: PageStatus, http_status: Option<u16>) -> Option<String> {
    Some(match status {
        PageStatus::Results => return None,
        PageStatus::NoRelevant => "검색어와 맞는 상품이 없어요".into(),
        PageStatus::Empty => "검색 결과 없음".into(),
        PageStatus::Unparsed => "상품 목록을 읽지 못했어요".into(),
        PageStatus::LoginRequired => "로그인이 필요한 페이지예요".into(),
        PageStatus::RateLimited => "요청 제한(429) — 잠시 후 다시 시도하세요".into(),
        PageStatus::Blocked => "자동 접근이 차단됐어요".into(),
        PageStatus::HttpError => format!("HTTP {}", http_status.unwrap_or(0)),
        PageStatus::NetworkError => "네트워크 오류".into(),
        PageStatus::LinkOnly => "앱 안에서 검색할 수 없는 판매처예요. 브라우저로 열어 보세요".into(),
        PageStatus::Skipped => "이 판매처 인코딩으로 검색어를 보낼 수 없어요".into(),
    })
}

async fn search_store(app: &AppHandle, search_id: &str, site: &Site, analysis: &Analysis) -> StoreResult {
    let started = Instant::now();
    let mut result = StoreResult::new(search_id, site);
    let candidates = query_candidates(analysis, site.profile.preferred_language);

    if site.platform == Platform::NaverStore {
        result.status = PageStatus::LinkOnly;
        result.search_url = candidates.first().and_then(|q| encode_query(site, q)).and_then(|(enc, _)| url::fill_template(&site.url, &enc).ok());
        result.message = status_message(PageStatus::LinkOnly, None);
        return result;
    }

    let mut best: Option<(Vec<Product>, PageStatus, Option<u16>)> = None;
    for query in candidates.iter().take(2) {
        let Some((encoded, encoding_fallback)) = encode_query(site, query) else {
            if result.attempts.is_empty() && best.is_none() {
                result.status = PageStatus::Skipped;
            }
            continue;
        };
        let target = match url::fill_template(&site.url, &encoded) {
            Ok(t) => t,
            Err(error) => {
                result.status = PageStatus::Skipped;
                result.message = Some(error);
                return result;
            }
        };
        if result.search_url.is_none() {
            result.search_url = Some(target.clone());
        }
        let attempt_started = Instant::now();
        let (status, http_status, products) = if site.platform == Platform::Bunjang {
            search_bunjang(site, analysis, query, &encoded).await
        } else {
            match fetch(&target).await {
                Ok(page) => {
                    let outcome = process_html(&site.name, site.platform, analysis, query, page.status, &page.final_url, &page.body);
                    write_debug_artifact(app, search_id, site, query, &target, &page, &outcome);
                    (outcome.status, Some(page.status), outcome.products)
                }
                Err(error) => {
                    result.message = Some(error);
                    (PageStatus::NetworkError, None, vec![])
                }
            }
        };
        let relevant = products.iter().filter(|p| p.relevant).count();
        result.attempts.push(Attempt {
            query: query.clone(),
            url: target,
            status,
            http_status,
            products: products.len(),
            relevant,
            encoding_fallback,
            duration_ms: attempt_started.elapsed().as_millis() as u64,
        });
        let better = best.as_ref().is_none_or(|(b, _, _)| relevant > b.iter().filter(|p| p.relevant).count() || (relevant == 0 && b.is_empty() && !products.is_empty()));
        if better {
            best = Some((products, status, http_status));
        }
        if !status.worth_retry() {
            break;
        }
    }

    if let Some((products, status, http_status)) = best {
        result.status = status;
        result.set_products(products);
        // Keep the concrete network error text; otherwise describe the status.
        if status != PageStatus::NetworkError || result.message.is_none() {
            result.message = status_message(status, http_status);
        }
    } else if result.message.is_none() {
        result.message = status_message(result.status, None);
    }
    result.duration_ms = started.elapsed().as_millis() as u64;
    result
}

// ---------------------------------------------------------------- bunjang

#[derive(Deserialize)]
struct BunjangResponse {
    #[serde(default)]
    list: Vec<serde_json::Value>,
}

fn json_string(value: &serde_json::Value, key: &str) -> Option<String> {
    match value.get(key)? {
        serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn bunjang_products(body: &str) -> Result<Vec<RawProduct>, String> {
    let response: BunjangResponse = serde_json::from_str(body).map_err(|e| format!("번개장터 응답 해석 실패: {e}"))?;
    Ok(response
        .list
        .iter()
        .filter(|item| !item.get("ad").and_then(|v| v.as_bool()).unwrap_or(false))
        .filter_map(|item| {
            let pid = json_string(item, "pid")?;
            let name = json_string(item, "name")?;
            let price = json_string(item, "price").and_then(|p| p.replace(',', "").parse::<u64>().ok()).filter(|p| *p > 0);
            let image = json_string(item, "product_image").map(|i| i.replace("{res}", "360").replace("{cnt}", "1"));
            let status = json_string(item, "status");
            Some(RawProduct {
                name,
                url: Some(format!("https://m.bunjang.co.kr/products/{pid}")),
                price,
                image,
                sold_out: status.is_some_and(|s| s != "0"),
                badges: String::new(),
            })
        })
        .collect())
}

async fn search_bunjang(site: &Site, analysis: &Analysis, query: &str, encoded: &str) -> (PageStatus, Option<u16>, Vec<Product>) {
    let api = format!("https://api.bunjang.co.kr/api/1/find_v2.json?q={encoded}&order=score&page=0&n=60&stat_device=w&version=4");
    match fetch(&api).await {
        Ok(page) if page.status == 429 => (PageStatus::RateLimited, Some(429), vec![]),
        Ok(page) if page.status >= 400 => (PageStatus::HttpError, Some(page.status), vec![]),
        Ok(page) => match bunjang_products(&page.body) {
            Ok(raws) if raws.is_empty() => (PageStatus::Empty, Some(page.status), vec![]),
            Ok(raws) => {
                let products = evaluate(&site.name, query, analysis, raws, store_kind(Platform::Bunjang));
                let status = if products.iter().any(|p| p.relevant) { PageStatus::Results } else { PageStatus::NoRelevant };
                (status, Some(page.status), products)
            }
            Err(error) => {
                log::warn!("{error}");
                (PageStatus::Unparsed, Some(page.status), vec![])
            }
        },
        Err(error) => {
            log::warn!("bunjang: {error}");
            (PageStatus::NetworkError, None, vec![])
        }
    }
}

// ---------------------------------------------------------------- debug capture

fn debug_capture_enabled() -> bool {
    cfg!(debug_assertions) || std::env::var("FIGURESEARCH_DEBUG").is_ok_and(|v| v == "1")
}

fn safe_file_name(value: &str) -> String {
    value.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

/// Saves the raw page and a parse summary under the app's log folder so store
/// layouts can be turned into test fixtures.
fn write_debug_artifact(app: &AppHandle, search_id: &str, site: &Site, query: &str, target: &str, page: &Fetched, outcome: &search_core::PageOutcome) {
    if !debug_capture_enabled() {
        return;
    }
    let Ok(base) = app.path().app_local_data_dir() else { return };
    let directory = base.join("logs").join(format!("search-{}", safe_file_name(search_id)));
    if std::fs::create_dir_all(&directory).is_err() {
        return;
    }
    let stem = format!("{}-{}", safe_file_name(&site.name), safe_file_name(query));
    let report = serde_json::json!({
        "store": site.name,
        "platform": site.platform.label(),
        "query": query,
        "requested_url": target,
        "final_url": page.final_url,
        "http_status": page.status,
        "status": outcome.status,
        "product_links": outcome.product_links,
        "scoped": outcome.scoped,
        "products": outcome.products.iter().take(40).map(|p| serde_json::json!({
            "name": p.name, "price": p.price, "relevant": p.relevant, "missing": p.missing,
            "category": p.category, "url": p.url,
        })).collect::<Vec<_>>(),
    });
    let _ = std::fs::write(directory.join(format!("{stem}.html")), &page.body);
    let _ = std::fs::write(directory.join(format!("{stem}.json")), serde_json::to_string_pretty(&report).unwrap_or_default());
}

// ---------------------------------------------------------------- commands

fn latest_search() -> &'static Mutex<String> {
    static LATEST: OnceLock<Mutex<String>> = OnceLock::new();
    LATEST.get_or_init(|| Mutex::new(String::new()))
}

fn is_current(search_id: &str) -> bool {
    latest_search().lock().map(|id| *id == search_id).unwrap_or(true)
}

/// Groups sites by host so the same server is never hit in parallel.
fn host_groups(selected: Vec<Site>) -> VecDeque<Vec<Site>> {
    let mut groups: Vec<(String, Vec<Site>)> = Vec::new();
    for site in selected {
        let host = url::host(&site.url);
        match groups.iter_mut().find(|(h, _)| *h == host) {
            Some((_, list)) => list.push(site),
            None => groups.push((host, vec![site])),
        }
    }
    groups.into_iter().map(|(_, list)| list).collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchSummary {
    search_id: String,
    stores: usize,
    cancelled: bool,
}

#[tauri::command]
async fn search_stores(app: AppHandle, search_id: String, term: String, stores: Vec<String>) -> Result<SearchSummary, String> {
    let analysis = Arc::new(analyze(&term));
    if analysis.is_empty() {
        return Err("검색어를 입력해 주세요.".into());
    }
    let selected: Vec<Site> = sites().iter().filter(|s| stores.contains(&s.name)).cloned().collect();
    if selected.is_empty() {
        return Err("검색할 판매처를 하나 이상 선택해 주세요.".into());
    }
    if let Ok(mut latest) = latest_search().lock() {
        *latest = search_id.clone();
    }
    let total = selected.len();
    let queue = Arc::new(Mutex::new(host_groups(selected)));
    let workers = WORKERS.min(queue.lock().map(|q| q.len()).unwrap_or(1)).max(1);
    let mut handles = Vec::with_capacity(workers);
    for _ in 0..workers {
        let (app, queue, analysis, search_id) = (app.clone(), queue.clone(), analysis.clone(), search_id.clone());
        handles.push(tauri::async_runtime::spawn(async move {
            loop {
                let group = queue.lock().ok().and_then(|mut q| q.pop_front());
                let Some(group) = group else { break };
                for site in group {
                    if !is_current(&search_id) {
                        return;
                    }
                    let result = search_store(&app, &search_id, &site, &analysis).await;
                    log::info!(
                        "[search] {} status={:?} relevant={} hidden={} attempts={}",
                        result.store, result.status, result.relevant, result.hidden, result.attempts.len()
                    );
                    if is_current(&search_id) {
                        let _ = app.emit("search-store", result);
                    }
                }
            }
        }));
    }
    for handle in handles {
        let _ = handle.await;
    }
    Ok(SearchSummary { cancelled: !is_current(&search_id), search_id, stores: total })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SiteCheck {
    ok: bool,
    status: Option<u16>,
    detail: String,
}

/// Lightweight reachability check for each store's search page.
#[tauri::command]
async fn check_sites() -> HashMap<String, SiteCheck> {
    let mut handles = Vec::new();
    for group in host_groups(sites().to_vec()) {
        handles.push(tauri::async_runtime::spawn(async move {
            let mut out = Vec::new();
            for site in group {
                let check = if site.platform == Platform::NaverStore {
                    SiteCheck { ok: true, status: None, detail: "브라우저로 열기".into() }
                } else {
                    let target = encode_query(&site, "figure").and_then(|(q, _)| url::fill_template(&site.url, &q).ok());
                    match target {
                        None => SiteCheck { ok: false, status: None, detail: "URL 설정 오류".into() },
                        Some(target) => match fetch(&target).await {
                            Ok(page) => SiteCheck {
                                ok: page.status < 400 || page.status == 429,
                                status: Some(page.status),
                                detail: if page.status == 429 { "요청 제한".into() } else { "응답 수신".into() },
                            },
                            Err(error) => SiteCheck { ok: false, status: None, detail: error },
                        },
                    }
                };
                out.push((site.name, check));
            }
            out
        }));
    }
    let mut checks = HashMap::new();
    for handle in handles {
        if let Ok(list) = handle.await {
            checks.extend(list);
        }
    }
    checks
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![search_stores, check_sites])
        .setup(|app| {
            app.handle().plugin(tauri_plugin_log::Builder::default().level(log::LevelFilter::Info).build())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_configured_site_loads() {
        assert!(sites().len() >= 30);
        assert!(sites().iter().any(|s| s.encoding == SiteEncoding::EucKr));
        assert!(sites().iter().any(|s| s.platform == Platform::NaverStore));
        for site in sites() {
            assert!(url::fill_template(&site.url, "x").is_ok(), "{}", site.name);
        }
    }

    #[test]
    fn euc_kr_encoding_and_fallback() {
        let site = sites().iter().find(|s| s.encoding == SiteEncoding::EucKr).unwrap().clone();
        assert_eq!(encode_query(&site, "미쿠"), Some(("%B9%CC%C4%ED".to_string(), false)));
        // Emoji cannot be represented in EUC-KR, so the UTF-8 fallback is used.
        let (encoded, fallback) = encode_query(&site, "미쿠 🎉").unwrap();
        assert!(fallback && encoded.starts_with("%EB%AF%B8"), "UTF-8 fallback: {encoded}");
        let mut skip = site.clone();
        skip.profile.euc_kr_fallback = Some("skip".into());
        assert_eq!(encode_query(&skip, "미쿠 🎉"), None);
    }

    #[test]
    fn parses_bunjang_payload() {
        let body = r#"{"list":[
            {"pid":"301","name":"넨도로이드 하츠네 미쿠 미개봉","price":"45000","product_image":"https://media.bunjang.co.kr/product/301_{cnt}_1_w{res}.jpg","status":"0"},
            {"pid":302,"name":"광고 상품","price":"1000","ad":true},
            {"pid":"303","name":"figma 미쿠","price":"30000","status":"3"}
        ]}"#;
        let products = bunjang_products(body).unwrap();
        assert_eq!(products.len(), 2);
        assert_eq!(products[0].url.as_deref(), Some("https://m.bunjang.co.kr/products/301"));
        assert_eq!(products[0].price, Some(45000));
        assert_eq!(products[0].image.as_deref(), Some("https://media.bunjang.co.kr/product/301_1_1_w360.jpg"));
        assert!(products[1].sold_out);
    }
}
