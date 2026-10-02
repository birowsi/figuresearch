import { useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-shell";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import sitesData from "../sites.json";
import {
  applyFiltersGrouped, categoryCounts, categoryLabels, defaultFilters, formatPrice, isFailure, lowestPrice,
  statusLabels, tagLabels, type Product, type ResultFilters, type SortKey, type StoreResult,
} from "./results";

type SiteConfig = { name: string; url: string };
type Site = { name: string; host: string; encoding: string };
type SiteCheck = { ok: boolean; status: number | null; detail: string };

const allSites: Site[] = Object.entries(sitesData).flatMap(([encoding, sites]) =>
  (sites as SiteConfig[]).map((site) => ({
    name: site.name,
    encoding,
    host: new URL(site.url.replace("{input}", "x")).hostname.replace(/^www\./, ""),
  })),
);
const siteNames = new Set(allSites.map((site) => site.name));
const SELECTED_KEY = "figure-search-selected-v2";

function loadSelected() {
  try {
    const saved = JSON.parse(localStorage.getItem(SELECTED_KEY) ?? "null") as string[] | null;
    const valid = saved?.filter((name) => siteNames.has(name));
    if (valid?.length) return new Set(valid);
  } catch { /* ignore broken storage */ }
  return new Set(siteNames);
}

const availabilityLabel = (p: Product) =>
  p.availability === "in_stock" ? "판매 중" : p.availability === "sold_out" ? "품절" : "재고 미확인";

function App() {
  const [term, setTerm] = useState("");
  const [searchedTerm, setSearchedTerm] = useState("");
  const [selected, setSelected] = useState(loadSelected);
  const [filter, setFilter] = useState("");
  const [notice, setNotice] = useState("");
  const [checks, setChecks] = useState<Record<string, SiteCheck>>({});
  const [checking, setChecking] = useState(false);
  const [skipFailed, setSkipFailed] = useState(true);
  const [stores, setStores] = useState<Record<string, StoreResult>>({});
  const [searching, setSearching] = useState(false);
  const [total, setTotal] = useState(0);
  const [filters, setFilters] = useState<ResultFilters>(defaultFilters);
  const searchId = useRef("");

  useEffect(() => {
    try { localStorage.setItem(SELECTED_KEY, JSON.stringify([...selected])); } catch { /* ignore */ }
  }, [selected]);

  useEffect(() => {
    const unlisten = listen<StoreResult>("search-store", (event) => {
      if (event.payload.searchId !== searchId.current) return;
      setStores((current) => ({ ...current, [event.payload.store]: event.payload }));
    });
    return () => { void unlisten.then((stop) => stop()); };
  }, []);

  const checkSites = async () => {
    setChecking(true);
    try {
      const result = await invoke<Record<string, SiteCheck>>("check_sites");
      setChecks(result);
      const ok = Object.values(result).filter((check) => check.ok).length;
      setNotice((current) => current || `${ok}/${allSites.length}곳이 응답했어요.`);
    } catch (error) {
      setNotice(`상태 확인 실패: ${String(error)}`);
    } finally {
      setChecking(false);
    }
  };
  useEffect(() => { void checkSites(); }, []);

  const visibleSites = useMemo(() => {
    const needle = filter.toLocaleLowerCase();
    return allSites.filter((site) => `${site.name} ${site.host}`.toLocaleLowerCase().includes(needle));
  }, [filter]);
  const selectedSites = allSites.filter((site) => selected.has(site.name) && (!skipFailed || checks[site.name]?.ok !== false));

  const runSearch = async () => {
    const query = term.trim();
    if (!query) return setNotice("검색어를 입력해 주세요.");
    if (!selectedSites.length) return setNotice("검색할 판매처를 하나 이상 선택해 주세요.");
    const id = crypto.randomUUID();
    searchId.current = id;
    setStores({});
    setTotal(selectedSites.length);
    setSearchedTerm(query);
    setFilters((current) => ({ ...current, category: "all" }));
    setSearching(true);
    setNotice(`${selectedSites.length}곳에서 검색 중...`);
    try {
      await invoke("search_stores", { searchId: id, term: query, stores: selectedSites.map((site) => site.name) });
      if (searchId.current === id) setNotice("");
    } catch (error) {
      if (searchId.current === id) setNotice(String(error));
    } finally {
      if (searchId.current === id) setSearching(false);
    }
  };

  const storeResults = Object.values(stores);
  const products = useMemo(() => Object.values(stores).flatMap((store) => store.products), [stores]);
  const visibleGroups = useMemo(() => applyFiltersGrouped(products, filters), [products, filters]);
  const counts = useMemo(() => categoryCounts(products, filters), [products, filters]);
  const relevantCount = products.filter((p) => p.relevant).length;
  const hiddenCount = storeResults.reduce((sum, store) => sum + store.hidden, 0);
  const failedCount = storeResults.filter((store) => isFailure(store.status)).length;
  const minPrice = lowestPrice(products);
  const orderedStores = [...storeResults].sort((a, b) => b.relevant - a.relevant || a.store.localeCompare(b.store, "ko"));

  const toggle = (name: string) => setSelected((current) => {
    const next = new Set(current);
    if (next.has(name)) next.delete(name); else next.add(name);
    return next;
  });
  const selectVisible = (value: boolean) => setSelected((current) => {
    const next = new Set(current);
    visibleSites.forEach((site) => value ? next.add(site.name) : next.delete(site.name));
    return next;
  });
  const updateFilter = <K extends keyof ResultFilters>(key: K, value: ResultFilters[K]) =>
    setFilters((current) => ({ ...current, [key]: value }));
  const openUrl = (url: string | null) => { if (url) void open(url); };

  const checkLabel = (name: string) => {
    const check = checks[name];
    if (!check) return ["unknown", "미확인"];
    if (check.status === 429) return ["limited", "제한"];
    if (check.ok && check.status === null) return ["ok", "링크"];
    return check.ok ? ["ok", "정상"] : ["failed", "실패"];
  };

  return (
    <main className="shell">
      <header className="app-header">
        <div><h1>FigureSearch</h1><p>피규어 판매처 통합 검색 <span className="header-status">· {selectedSites.length}곳 선택됨</span></p></div>
        <button className="refresh" disabled={checking} onClick={() => void checkSites()}>{checking ? "확인 중..." : "상태 새로고침"}</button>
      </header>

      <section className="search-panel">
        <label htmlFor="search">검색어</label>
        <div className="search-row">
          <input id="search" autoFocus value={term} onChange={(event) => setTerm(event.target.value)}
            onKeyDown={(event) => { if (event.key === "Enter" && !event.nativeEvent.isComposing) void runSearch(); }}
            placeholder="예: 넨도로이드 하츠네 미쿠, figma 600, 블루 아카이브 1/7" />
          <button className="primary" disabled={searching} onClick={() => void runSearch()}>{searching ? "검색 중..." : "검색"}</button>
        </div>
        {notice && <p className="notice">{notice}</p>}
      </section>

      {(searching || storeResults.length > 0) && <>
        <section className="summary" aria-live="polite">
          <span>판매처 {storeResults.length}/{total}</span>
          <span>상품 {relevantCount}</span>
          {hiddenCount > 0 && <span>관련 낮음 {hiddenCount}</span>}
          {failedCount > 0 && <span className="summary-warn">실패 {failedCount}</span>}
          <span>최저가 {minPrice === null ? "정보 없음" : formatPrice(minPrice)}</span>
          {searchedTerm && <span className="muted">"{searchedTerm}"</span>}
        </section>

        <section className="results">
          <div className="result-toolbar">
            <div className="chips">
              <button className={`chip ${filters.category === "all" ? "active" : ""}`} onClick={() => updateFilter("category", "all")}>
                전체 {counts.reduce((sum, [, n]) => sum + n, 0)}
              </button>
              {counts.map(([category, count]) => (
                <button key={category} className={`chip ${filters.category === category ? "active" : ""}`} onClick={() => updateFilter("category", category)}>
                  {categoryLabels[category]} {count}
                </button>
              ))}
            </div>
            <div className="result-options">
              <label><input type="checkbox" checked={filters.hideSoldOut} onChange={(e) => updateFilter("hideSoldOut", e.target.checked)} /> 품절 제외</label>
              <label><input type="checkbox" checked={filters.hideUsed} onChange={(e) => updateFilter("hideUsed", e.target.checked)} /> 중고 제외</label>
              <label><input type="checkbox" checked={filters.showHidden} onChange={(e) => updateFilter("showHidden", e.target.checked)} /> 관련 낮은 결과 보기</label>
              <select value={filters.sort} onChange={(e) => updateFilter("sort", e.target.value as SortKey)}>
                <option value="relevance">관련도순</option>
                <option value="price-asc">낮은 가격순</option>
                <option value="price-desc">높은 가격순</option>
                <option value="store">판매처순</option>
              </select>
            </div>
          </div>

          {visibleGroups.length > 0 ? <div className="result-list">{visibleGroups.map(({ key, best: p, offers }) => (
            <div className={`result-group ${offers.some((o) => o.relevant) ? "" : "dimmed"}`} key={key}>
              <article className="result-row" onClick={() => openUrl(p.url)}
                onKeyDown={(e) => { if (e.key === "Enter") openUrl(p.url); }} role={p.url ? "button" : undefined} tabIndex={p.url ? 0 : undefined}>
                {p.imageUrl ? <img src={p.imageUrl} alt="" loading="lazy" referrerPolicy="no-referrer" /> : <span className="image-placeholder" />}
                <div className="result-main">
                  <strong>{p.name}</strong>
                  <small>
                    <span>{offers.length > 1 ? `판매처 ${offers.length}곳 · 최저 ${p.store}` : p.store}</span>
                    <span className="category">{categoryLabels[p.category]}</span>
                    {p.tags.map((tag) => <span key={tag} className={`tag tag-${tag}`}>{tagLabels[tag]}</span>)}
                    {p.release && <span>{p.release}</span>}
                    <span className={p.availability === "sold_out" ? "soldout" : ""}>{availabilityLabel(p)}</span>
                    {!p.relevant && p.missing.length > 0 && <span className="missing">없는 단어: {p.missing.join(", ")}</span>}
                  </small>
                </div>
                <span className={`result-price ${p.availability === "sold_out" ? "soldout" : ""}`}>
                  {offers.length > 1 && p.price !== null && <em>최저</em>}{formatPrice(p.price)}
                </span>
              </article>
              {offers.length > 1 && <ul className="offer-list">{offers.map((o) => (
                <li key={o.id} className={o === p ? "cheapest" : ""} onClick={() => openUrl(o.url)}
                  onKeyDown={(e) => { if (e.key === "Enter") openUrl(o.url); }} role={o.url ? "button" : undefined} tabIndex={o.url ? 0 : undefined}>
                  <span className="offer-store">{o.store}</span>
                  <span className="offer-meta">
                    {o.tags.map((tag) => <span key={tag} className={`tag tag-${tag}`}>{tagLabels[tag]}</span>)}
                    <span className={o.availability === "sold_out" ? "soldout" : ""}>{availabilityLabel(o)}</span>
                  </span>
                  <span className={`result-price ${o.availability === "sold_out" ? "soldout" : ""}`}>{formatPrice(o.price)}</span>
                </li>
              ))}</ul>}
            </div>
          ))}</div> : !searching && <div className="empty">
            {hiddenCount > 0 ? "검색어와 정확히 맞는 상품이 없어요. '관련 낮은 결과 보기'로 비슷한 상품을 볼 수 있어요." : "조건에 맞는 상품이 없어요."}
          </div>}

          {orderedStores.length > 0 && <div className="store-status">
            <h3>판매처별 결과</h3>
            <div className="store-grid">{orderedStores.map((store) => (
              <button key={store.store} className={`store-chip status-${store.status}`} title={store.message ?? ""}
                disabled={!store.searchUrl} onClick={() => openUrl(store.searchUrl)}>
                <strong>{store.store}</strong>
                <span>{store.status === "results" ? `${store.relevant}개` : statusLabels[store.status]}</span>
              </button>
            ))}</div>
            <p className="muted">판매처를 누르면 그 판매처의 검색 결과를 브라우저로 열어요.</p>
          </div>}
        </section>
      </>}

      <section className="toolbar">
        <div><h2>검색할 판매처</h2><span className="muted">{allSites.length}개 사이트</span></div>
        <div className="controls"><input value={filter} onChange={(event) => setFilter(event.target.value)} placeholder="판매처 필터" /></div>
      </section>
      <div className="selection-actions">
        <label><input type="checkbox" checked={skipFailed} onChange={(event) => setSkipFailed(event.target.checked)} /> 응답 없는 판매처 제외</label>
        <button onClick={() => selectVisible(true)}>전체 선택</button><button onClick={() => selectVisible(false)}>전체 해제</button>
      </div>
      <section className="site-grid">{visibleSites.map((site) => {
        const [state, label] = checkLabel(site.name);
        return <label className={`site-card ${selected.has(site.name) ? "selected" : ""}`} key={site.name}>
          <input type="checkbox" checked={selected.has(site.name)} onChange={() => toggle(site.name)} />
          <span className="checkmark">✓</span>
          <span className="site-info"><strong>{site.name}</strong><small>{site.host}</small></span>
          <span className={`status ${state}`} title={checks[site.name]?.detail}>{label}</span>
        </label>;
      })}</section>
      {!visibleSites.length && <div className="empty">조건에 맞는 판매처가 없어요.</div>}
      <footer>검색 결과는 앱 안에 표시되고, 상품이나 판매처를 누를 때만 외부 브라우저가 열려요. 네이버 스마트스토어는 앱 안에서 검색할 수 없어 브라우저 링크로 제공해요.</footer>
    </main>
  );
}

export default App;
