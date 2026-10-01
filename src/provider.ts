export type PageClassification =
  | "search_results"
  | "empty_results"
  | "login_required"
  | "rate_limited"
  | "bot_challenge"
  | "unexpected_html"
  | "unsupported";

export type RejectionReason =
  | "navigation"
  | "auth_link"
  | "customer_service"
  | "pagination"
  | "no_product_evidence"
  | "duplicate"
  | "invalid_url"
  | "low_relevance";

export type NormalizedResult = {
  id: string;
  store: string;
  name: string;
  url: string | null;
  price: number | null;
  priceText: string | null;
  availability: "in-stock" | "out-of-stock" | "unknown";
  imageUrl: string | null;
  query: string;
};

export type ParseReport = {
  provider: string;
  adapter: string;
  classification: PageClassification;
  rawCandidates: number;
  acceptedResults: number;
  rejectedResults: number;
  rejectedReasonCounts: Partial<Record<RejectionReason, number>>;
  acceptedSamples: Array<{ name: string; url: string | null }>;
  rejectedSamples: Array<{ text: string; reason: RejectionReason }>;
  warnings: string[];
};

export type ParseOutcome = {
  results: NormalizedResult[];
  report: ParseReport;
};

const genericTerms = new Set([
  "본문 바로가기", "네이버", "로그인", "로그인하기", "qr 코드 로그인", "아이디 찾기",
  "비밀번호 찾기", "회원가입", "스마트봇 상담", "고객센터", "장바구니", "더보기",
  "검색", "검색하기", "home", "my", "next", "prev", "이전", "다음",
]);
const authPattern = /로그인|회원가입|아이디 찾기|비밀번호 찾기|qr\s*코드|인증/iu;
const navigationPattern = /본문 바로가기|고객센터|스마트봇|장바구니|메뉴|카테고리|home|my/iu;
const productUrlPattern = /\/(?:product|goods|item|detail|products?)\/|\/(?:p|g)\/\d+/iu;

const decode = (value: string) => value
  .replace(/&amp;/gu, "&").replace(/&quot;/gu, "\"").replace(/&#39;/gu, "'")
  .replace(/&lt;/gu, "<").replace(/&gt;/gu, ">");
const strip = (value: string) => decode(value.replace(/<[^>]*>/gu, " ").replace(/\s+/gu, " ").trim());
const absolute = (base: string, value: string | undefined) => {
  if (!value) return null;
  try { return new URL(value, base).toString(); } catch { return null; }
};
const countReason = (report: ParseReport, reason: RejectionReason) => {
  report.rejectedReasonCounts[reason] = (report.rejectedReasonCounts[reason] ?? 0) + 1;
  report.rejectedResults += 1;
};

function classifyPage(html: string, status: number, finalUrl: string) {
  const text = strip(html).slice(0, 12000);
  if (status === 429) return "rate_limited" as const;
  if (/captcha|robot check|보안 확인|자동입력 방지/iu.test(text)) return "bot_challenge" as const;
  if (authPattern.test(text) && !productUrlPattern.test(html)) return "login_required" as const;
  if (!html.trim() || /검색 결과가 없습니다|검색된 상품이 없습니다|no results found/iu.test(text)) return "empty_results" as const;
  if (new URL(finalUrl).pathname === "/" && !productUrlPattern.test(html)) return "unexpected_html" as const;
  return "search_results" as const;
}

function parseJsonLd(store: string, baseUrl: string, query: string, html: string, report: ParseReport) {
  const results: NormalizedResult[] = [];
  for (const match of html.matchAll(/<script[^>]+type=["']application\/ld\+json["'][^>]*>([\s\S]*?)<\/script>/giu)) {
    try {
      const data = JSON.parse(match[1].trim());
      const nodes = Array.isArray(data) ? data : data["@graph"] ?? [data];
      for (const node of nodes) {
        if (!/Product|ItemList/iu.test(String(node["@type"]))) continue;
        const items = node["@type"] === "ItemList" ? (node.itemListElement ?? []) : [node];
        for (const item of items) {
          const product = item.item ?? item;
          if (!product.name) continue;
          const offer = Array.isArray(product.offers) ? product.offers[0] : product.offers;
          const url = absolute(baseUrl, product.url ?? offer?.url);
          const price = Number(offer?.price);
          results.push({
            id: `${store}:${url ?? product.name}`.toLocaleLowerCase(), store, name: String(product.name),
            url, price: Number.isFinite(price) ? price : null,
            priceText: Number.isFinite(price) ? `${price.toLocaleString("ko-KR")}원` : null,
            availability: /outofstock|품절/iu.test(String(offer?.availability)) ? "out-of-stock" : offer ? "in-stock" : "unknown",
            imageUrl: absolute(baseUrl, Array.isArray(product.image) ? product.image[0] : product.image), query,
          });
        }
      }
    } catch { report.warnings.push("malformed_json_ld"); }
  }
  return results;
}

function parseStrictAnchors(store: string, baseUrl: string, query: string, html: string, report: ParseReport) {
  const results: NormalizedResult[] = [];
  const linkPattern = /<a\b([^>]*href\s*=\s*["'][^"']+["'][^>]*)>([\s\S]*?)<\/a>/giu;
  for (const match of html.matchAll(linkPattern)) {
    const attrs = match[1];
    const text = strip(match[2]);
    const href = attrs.match(/href\s*=\s*["']([^"']+)["']/iu)?.[1];
    report.rawCandidates += 1;
    const lowerText = text.toLocaleLowerCase();
    let rejection: RejectionReason | null = null;
    if (!text || genericTerms.has(lowerText) || authPattern.test(text)) rejection = authPattern.test(text) ? "auth_link" : "navigation";
    else if (navigationPattern.test(text)) rejection = "navigation";
    const url = absolute(baseUrl, href);
    const context = `${attrs} ${match[2]}`;
    const image = match[2].match(/<img\b[^>]*src\s*=\s*["']([^"']+)["']/iu)?.[1] ?? attrs.match(/data-src\s*=\s*["']([^"']+)["']/iu)?.[1];
    const priceMatch = context.match(/(?:₩|￦|가격|price[^0-9]{0,10})([\d,]{3,})/iu);
    const hasProductEvidence = Boolean(url && productUrlPattern.test(url) && (priceMatch || image || text.length >= 8));
    if (!rejection && !hasProductEvidence) rejection = "no_product_evidence";
    if (rejection) {
      countReason(report, rejection);
      if (report.rejectedSamples.length < 8) report.rejectedSamples.push({ text: text.slice(0, 120), reason: rejection });
      continue;
    }
    const priceText = priceMatch ? priceMatch[1].replace(/,/gu, "") : null;
    const result: NormalizedResult = {
      id: `${store}:${url}`.toLocaleLowerCase(), store, name: text, url,
      price: priceText ? Number(priceText) : null,
      priceText: priceText ? `${Number(priceText).toLocaleString("ko-KR")}원` : null,
      availability: /품절|sold\s*out|out\s*of\s*stock/iu.test(context) ? "out-of-stock" : priceText ? "in-stock" : "unknown",
      imageUrl: absolute(baseUrl, image), query,
    };
    if (results.some((item) => item.id === result.id)) {
      countReason(report, "duplicate");
    } else results.push(result);
  }
  return results;
}

export function parseProviderResponse(store: string, baseUrl: string, query: string, html: string, status = 200, finalUrl = baseUrl): ParseOutcome {
  const adapter = new URL(baseUrl).hostname.includes("naver.com") ? "NaverSmartStoreAdapter" : "StrictGenericAdapter";
  const classification = classifyPage(html, status, finalUrl);
  const report: ParseReport = {
    provider: store, adapter, classification, rawCandidates: 0, acceptedResults: 0, rejectedResults: 0,
    rejectedReasonCounts: {}, acceptedSamples: [], rejectedSamples: [], warnings: [],
  };
  if (classification !== "search_results") return { results: [], report };
  let results = parseJsonLd(store, baseUrl, query, html, report);
  if (!results.length) results = parseStrictAnchors(store, baseUrl, query, html, report);
  results = deduplicateResults(results);
  report.acceptedResults = results.length;
  report.acceptedSamples = results.slice(0, 5).map(({ name, url }) => ({ name, url }));
  if (!results.length && report.classification === "search_results") report.classification = "unsupported";
  return { results, report };
}

export function parseProviderResults(store: string, baseUrl: string, query: string, html: string) {
  return parseProviderResponse(store, baseUrl, query, html).results;
}

export function deduplicateResults(results: NormalizedResult[]) {
  const seen = new Set<string>();
  return results.filter((result) => {
    const key = result.url?.toLocaleLowerCase() ?? `${result.store}:${result.name.toLocaleLowerCase()}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}
