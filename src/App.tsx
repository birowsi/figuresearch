import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-shell";
import { invoke } from "@tauri-apps/api/core";
import sitesData from "../sites.json";
import { buildSearchUrl, type SearchEncoding } from "./search-url";
import { analyzeSearch, selectQueryCandidate, type SearchProfile } from "./search-query";
import { deduplicateResults, parseProviderResponse, type NormalizedResult, type ParseReport } from "./provider";

type Site = { id: string; name: string; url: string; encoding: SearchEncoding; host: string; profile?: SearchProfile };
type ProviderFetch = { status: number; body: string; detail: string; final_url: string; content_type: string | null; duration_ms: number };
type SiteConfig = { name: string; url: string; profile?: SearchProfile };
type SiteState = { ok: boolean; status: number | null; detail: string };

const allSites: Site[] = Object.entries(sitesData).flatMap(([encoding, sites]) =>
  (sites as SiteConfig[]).map((site, index) => ({
    id: `${encoding}-${index}-${site.name}`, name: site.name, url: site.url,
    encoding: encoding as SearchEncoding, profile: site.profile,
    host: new URL(site.url).hostname.replace(/^www\./, ""),
  })),
);

type RequestUrlResult = { url: string; usedFallback: boolean; skipped: boolean };

async function requestUrl(site: Site, candidate: string): Promise<RequestUrlResult> {
  if (site.encoding === "UTF-8") return { url: buildSearchUrl(site, candidate), usedFallback: false, skipped: false };
  const encoded = await invoke<{ encoded: string; used_fallback: boolean }>("encode_euc_kr", { query: candidate });
  if (encoded.used_fallback && site.profile?.eucKrFallback === "skip") {
    return { url: "", usedFallback: true, skipped: true };
  }
  return { url: buildSearchUrl(site, candidate, encoded.encoded), usedFallback: encoded.used_fallback, skipped: false };
}

async function withConcurrency<T, R>(items: T[], limit: number, worker: (item: T) => Promise<R>) {
  const results: R[] = [];
  let next = 0;
  async function consume() {
    while (next < items.length) {
      const item = items[next++];
      results.push(await worker(item));
    }
  }
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, consume));
  return results;
}

const isDebug = import.meta.env.DEV;
const reportProvider = (sessionId: string, site: Site, input: string, query: string, requestedUrl: string | null, response: ProviderFetch, report: ParseReport, usedFallback = false) => {
  const entry = {
    search_session_id: sessionId, provider: site.name, user_input: input, normalized_query: query,
    actual_provider_query: query, requested_url: requestedUrl,
    encoding_fallback: usedFallback, fallback_policy: site.profile?.eucKrFallback ?? "none",
    encoding: site.encoding, http_status: response.status, final_url: response.final_url,
    content_type: response.content_type, response_bytes: new TextEncoder().encode(response.body).length,
    duration_ms: response.duration_ms, response_classification: report.classification,
    adapter: report.adapter, raw_candidates: report.rawCandidates, accepted_results: report.acceptedResults,
    rejected_results: report.rejectedResults, rejected_reason_counts: report.rejectedReasonCounts,
    accepted_samples: report.acceptedSamples, rejected_samples: report.rejectedSamples,
  };
  console.info("[FigureSearch]", entry);
  if (isDebug) {
    void invoke("write_debug_artifact", {
      sessionId, provider: site.name, response: response.body, report: JSON.stringify(entry, null, 2),
    }).catch((error) => console.warn("debug artifact write failed", error));
  }
};

function App() {
  const [term, setTerm] = useState("");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(() => {
    const saved = localStorage.getItem("figure-search-selected");
    return saved ? new Set<string>(JSON.parse(saved)) : new Set(allSites.map((site) => site.id));
  });
  const [filter, setFilter] = useState("");
  const [encoding, setEncoding] = useState<"all" | SearchEncoding>("all");
  const [notice, setNotice] = useState("");
  const [checks, setChecks] = useState<Record<string, SiteState>>({});
  const [checking, setChecking] = useState(false);
  const [showFailedOnly, setShowFailedOnly] = useState(false);
  const [skipFailed, setSkipFailed] = useState(true);
  const [results, setResults] = useState<NormalizedResult[]>([]);
  const [searching, setSearching] = useState(false);
  const [progress, setProgress] = useState({ completed: 0, failed: 0, total: 0 });

  useEffect(() => { localStorage.setItem("figure-search-selected", JSON.stringify([...selected])); }, [selected]);
  const visibleSites = useMemo(() => allSites.filter((site) =>
    (encoding === "all" || site.encoding === encoding) &&
    (!showFailedOnly || checks[site.id]?.ok === false) &&
    `${site.name} ${site.host}`.toLocaleLowerCase().includes(filter.toLocaleLowerCase())), [checks, encoding, filter, showFailedOnly]);
  const selectedSites = allSites.filter((site) => selected.has(site.id) && (!skipFailed || checks[site.id]?.ok !== false));

  const checkSites = async () => {
    setChecking(true);
    const checked = await withConcurrency(allSites, 4, async (site) => {
      try {
        const request = await requestUrl(site, "figure");
        if (request.skipped) return [site.id, { ok: false, status: null, detail: "EUC-KR fallback 정책으로 건너뜀" }] as const;
        const response = await invoke<ProviderFetch>("fetch_provider", { url: request.url });
        return [site.id, { ok: response.status > 0 && (response.status < 400 || response.status === 429), status: response.status || null, detail: response.detail }] as const;
      } catch (error) {
        return [site.id, { ok: false, status: null, detail: String(error) }] as const;
      }
    });
    setChecks(Object.fromEntries(checked));
    setChecking(false);
    setNotice(`${checked.filter(([, state]) => state.ok).length}/${allSites.length}곳이 응답했습니다.`);
  };
  useEffect(() => { void checkSites(); }, []);

  const runSearch = async () => {
    const analysis = analyzeSearch(term);
    if (!analysis.normalized) return setNotice("검색어를 입력해 주세요.");
    if (!selectedSites.length) return setNotice("검색할 사이트를 하나 이상 선택해 주세요.");
    setQuery(analysis.normalized);
    setResults([]);
    setSearching(true);
    setProgress({ completed: 0, failed: 0, total: selectedSites.length });
    setNotice(`${selectedSites.length}곳에서 검색 중...`);
    const sessionId = crypto.randomUUID();
    const responseSets = await withConcurrency(selectedSites, 4, async (site) => {
      try {
        const candidate = selectQueryCandidate(analysis, site.profile);
        const request = await requestUrl(site, candidate);
        if (request.skipped) {
          console.info("[FigureSearch]", {
            search_session_id: sessionId, provider: site.name, user_input: term,
            normalized_query: analysis.normalized, actual_provider_query: candidate,
            requested_url: null, encoding: site.encoding, encoding_fallback: true,
            fallback_policy: site.profile?.eucKrFallback ?? "none",
            response_classification: "unsupported", parser: null,
            raw_candidates: 0, accepted_results: 0, rejected_results: 0,
            rejected_reason_counts: { encoding_fallback_skipped: 1 },
          });
          setProgress((state) => ({ ...state, completed: state.completed + 1, failed: state.failed + 1 }));
          return [] as NormalizedResult[];
        }
        const response = await invoke<ProviderFetch>("fetch_provider", { url: request.url });
        const outcome = parseProviderResponse(site.name, site.url, candidate, response.body, response.status, response.final_url);
        reportProvider(sessionId, site, term, analysis.normalized, request.url, response, outcome.report, request.usedFallback);
        if (response.status >= 400 || !response.body) throw new Error(`${response.status}: ${response.detail}`);
        const parsed = deduplicateResults(outcome.results);
        setResults((current) => deduplicateResults([...current, ...parsed]));
        setProgress((state) => ({ ...state, completed: state.completed + 1 }));
        return parsed;
      } catch (error) {
        console.warn("[FigureSearch]", {
          search_session_id: sessionId, provider: site.name, user_input: term,
          normalized_query: analysis.normalized, response_classification: "unexpected_html",
          error: String(error),
        });
        setProgress((state) => ({ ...state, completed: state.completed + 1, failed: state.failed + 1 }));
        setNotice(`${site.name}: ${String(error)}`);
        return [] as NormalizedResult[];
      }
    });
    setSearching(false);
    const total = deduplicateResults(responseSets.flat()).length;
    setNotice(`${selectedSites.length}곳 검색 완료 · ${total}개 상품`);
  };

  const toggle = (id: string) => setSelected((current) => {
    const next = new Set(current); next.has(id) ? next.delete(id) : next.add(id); return next;
  });
  const selectVisible = (value: boolean) => setSelected((current) => {
    const next = new Set(current); visibleSites.forEach((site) => value ? next.add(site.id) : next.delete(site.id)); return next;
  });
  const minPrice = results.filter((item) => item.price !== null).sort((a, b) => a.price! - b.price!)[0]?.priceText ?? "정보 없음";

  return (
    <main className="shell">
      <header className="app-header">
        <div><h1>FigureSearch</h1><p>피규어 판매처 통합 검색 <span className="header-status">· {selectedSites.length}곳 선택됨</span></p></div>
        <button className="refresh" disabled={checking} onClick={() => void checkSites()}>{checking ? "확인 중..." : "상태 새로고침"}</button>
      </header>
      <section className="search-panel">
        <label htmlFor="search">검색어</label>
        <div className="search-row">
          <input id="search" autoFocus value={term} onChange={(event) => setTerm(event.target.value)} onKeyDown={(event) => event.key === "Enter" && void runSearch()} placeholder="예: 넨도로이드 하츠네 미쿠" />
          <button className="primary" disabled={searching} onClick={() => void runSearch()}>{searching ? "검색 중..." : "검색"}</button>
        </div>
        {notice && <p className="notice">{notice}</p>}
      </section>
      {searching || results.length > 0 ? <section className="summary" aria-live="polite">
        <span>판매처 {progress.completed}/{progress.total}</span><span>실패 {progress.failed}</span><span>상품 {results.length}</span><span>최저가 {minPrice}</span>
      </section> : null}
      <section className="toolbar">
        <div><h2>검색할 판매처</h2><span className="muted">{allSites.length}개 사이트 · {query ? `"${query}"` : "원하는 곳을 선택하세요"}</span></div>
        <div className="controls"><input value={filter} onChange={(event) => setFilter(event.target.value)} placeholder="판매처 필터" /><select value={encoding} onChange={(event) => setEncoding(event.target.value as "all" | SearchEncoding)}><option value="all">전체 인코딩</option><option value="UTF-8">UTF-8</option><option value="EUC-KR">EUC-KR</option></select></div>
      </section>
      <div className="selection-actions">
        <label><input type="checkbox" checked={skipFailed} onChange={(event) => setSkipFailed(event.target.checked)} /> 실패 사이트 검색에서 제외</label>
        <label><input type="checkbox" checked={showFailedOnly} onChange={(event) => setShowFailedOnly(event.target.checked)} /> 실패 사이트만 보기</label>
        <button onClick={() => selectVisible(true)}>전체 선택</button><button onClick={() => selectVisible(false)}>전체 해제</button>
      </div>
      <section className="site-grid">{visibleSites.map((site) => <label className={`site-card ${selected.has(site.id) ? "selected" : ""}`} key={site.id}>
        <input type="checkbox" checked={selected.has(site.id)} onChange={() => toggle(site.id)} /><span className="checkmark">✓</span><span className="site-info"><strong>{site.name}</strong><small>{site.host}</small></span><span className={`status ${checks[site.id] ? checks[site.id].ok ? "ok" : "failed" : "unknown"}`}>{checks[site.id] ? checks[site.id].status === 429 ? "제한" : checks[site.id].ok ? "정상" : "실패" : "미확인"}</span><span className="encoding">{site.encoding}</span>
      </label>)}</section>
      {results.length > 0 && <section className="results"><h2>검색 결과</h2><div className="result-list">{results.map((result) => <article className="result-row" key={result.id} onClick={() => result.url && void open(result.url)} role={result.url ? "button" : undefined} tabIndex={result.url ? 0 : undefined}>
        {result.imageUrl ? <img src={result.imageUrl} alt="" /> : <span className="image-placeholder" />}
        <div className="result-main"><strong>{result.name}</strong><small>{result.store} · {result.availability === "in-stock" ? "판매 중" : result.availability === "out-of-stock" ? "품절" : "상태 미상"} · {result.query}</small></div><span className="result-price">{result.priceText ?? "가격 정보 없음"}</span>
      </article>)}</div></section>}
      {!visibleSites.length && <div className="empty">조건에 맞는 판매처가 없습니다.</div>}
      <footer>검색 결과는 이 앱 안에 표시되며, 상품을 클릭할 때만 외부 브라우저가 열립니다.</footer>
    </main>
  );
}
export default App;
