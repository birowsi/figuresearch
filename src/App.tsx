import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { checkSites as fetchChecks, isDesktop, openExternal, searchStores, type SiteCheck } from "./backend";
import sitesData from "../sites.json";
import {
  allMatchingProductsSoldOut, applyFiltersGrouped, categoryCounts, categoryLabels, defaultFilters, formatPrice, isFailure, lowestOffer,
  shippingLabel, statusLabels, tagLabels, totalPrice, withShipping, type Product, type ShippingPolicy,
  type ResultFilters, type SortKey, type StoreResult,
} from "./results";

type SiteConfig = { name: string; url: string; shipping?: ShippingPolicy };
type Site = { name: string; host: string; direct: boolean };

const allSites: Site[] = Object.values(sitesData).flatMap((sites) =>
  (sites as SiteConfig[]).map((site) => {
    const host = new URL(site.url.replace("{input}", "x")).hostname.replace(/^www\./, "");
    return { name: site.name, host, direct: ["smartstore.naver.com", "brand.naver.com", "shopping.naver.com"].includes(host) };
  }),
);
const shippingPolicies = Object.fromEntries(
  Object.values(sitesData).flatMap((sites) => (sites as SiteConfig[]).map((site) => [site.name, site.shipping])),
);
const siteNames = new Set(allSites.map((site) => site.name));
const SELECTED_KEY = "figure-search-selected-v2";
const RESULTS_PER_PAGE = 40;

function loadSelected() {
  try {
    const saved = JSON.parse(localStorage.getItem(SELECTED_KEY) ?? "null") as string[] | null;
    if (Array.isArray(saved)) return new Set(saved.filter((name) => siteNames.has(name)));
  } catch { /* ignore broken storage */ }
  return new Set(siteNames);
}

const availabilityLabel = (p: Product) =>
  p.availability === "in_stock" ? "판매 중" : p.availability === "sold_out" ? "품절" : "재고 미확인";

function ExternalLink({ url, className, children }: { url: string | null; className?: string; children: ReactNode }) {
  if (!url) return <span className={className}>{children}</span>;
  return (
    <a className={className} href={url} target="_blank" rel="noopener noreferrer" onClick={(event) => {
      if (isDesktop) {
        event.preventDefault();
        void openExternal(url);
      }
    }}>{children}</a>
  );
}

function ProductPrice({ product }: { product: Product }) {
  return <span className={`result-price ${product.availability === "sold_out" ? "soldout" : ""}`}>
    {formatPrice(totalPrice(product))}
    {product.price !== null && <small>{shippingLabel(product)}</small>}
  </span>;
}

function App() {
  const [term, setTerm] = useState("");
  const [searchedTerm, setSearchedTerm] = useState("");
  const [selected, setSelected] = useState(loadSelected);
  const [filter, setFilter] = useState("");
  const [notice, setNotice] = useState("");
  const [checks, setChecks] = useState<Record<string, SiteCheck>>({});
  const [checking, setChecking] = useState(false);
  const [stores, setStores] = useState<Record<string, StoreResult>>({});
  const [searching, setSearching] = useState(false);
  const [total, setTotal] = useState(0);
  const [visibleLimit, setVisibleLimit] = useState(RESULTS_PER_PAGE);
  const [filters, setFilters] = useState<ResultFilters>(defaultFilters);
  const searchId = useRef("");
  const activeTerm = useRef("");

  useEffect(() => {
    try { localStorage.setItem(SELECTED_KEY, JSON.stringify([...selected])); } catch { /* ignore */ }
  }, [selected]);

  const checkSites = async () => {
    setChecking(true);
    try {
      setChecks(await fetchChecks());
    } catch {
      setNotice("판매처 상태를 확인하지 못했습니다.");
    } finally {
      setChecking(false);
    }
  };
  const visibleSites = useMemo(() => {
    const needle = filter.toLocaleLowerCase();
    return allSites.filter((site) => `${site.name} ${site.host}`.toLocaleLowerCase().includes(needle));
  }, [filter]);
  const selectedSites = allSites.filter((site) => selected.has(site.name));
  const shoppingTerm = term.trim();
  const shoppingUrl = shoppingTerm
    ? `https://search.shopping.naver.com/search/all?query=${encodeURIComponent(shoppingTerm)}`
    : "https://search.shopping.naver.com/home";

  const runSearch = async () => {
    const query = term.trim();
    if (query === activeTerm.current && activeTerm.current) return;
    if (!query) return setNotice("검색어를 입력하세요.");
    if ([...query].length > 100) return setNotice("검색어는 100자까지 입력할 수 있습니다.");
    if (!selectedSites.length) return setNotice("검색할 판매처를 선택하세요.");
    const id = crypto.getRandomValues(new Uint32Array(4)).join("-");
    searchId.current = id;
    activeTerm.current = query;
    setStores({});
    setTotal(selectedSites.length);
    setSearchedTerm(query);
    setVisibleLimit(RESULTS_PER_PAGE);
    setFilters((current) => ({ ...current, category: "all" }));
    setSearching(true);
    setNotice("");
    try {
      await searchStores(id, query, selectedSites.map((site) => site.name), (result) => {
        if (searchId.current === id) setStores((current) => ({ ...current, [result.store]: result }));
      });
      if (searchId.current === id) setNotice("");
    } catch (error) {
      if (searchId.current === id) setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      if (searchId.current === id) {
        activeTerm.current = "";
        setSearching(false);
      }
    }
  };

  const storeResults = Object.values(stores);
  const products = useMemo(() => withShipping(Object.values(stores).flatMap((store) => store.products), shippingPolicies), [stores]);
  const visibleGroups = useMemo(() => applyFiltersGrouped(products, filters), [products, filters]);
  const counts = useMemo(() => categoryCounts(products, filters), [products, filters]);
  const hiddenCount = storeResults.reduce((sum, store) => sum + store.hidden, 0);
  const unavailableStores = storeResults.filter((store) => isFailure(store.status) || store.status === "skipped");
  const directStores = storeResults.filter((store) => store.status === "link_only" || store.status === "age_verification_required");
  const searchedStores = storeResults.filter((store) => !unavailableStores.includes(store) && !directStores.includes(store));
  const minOffer = lowestOffer(visibleGroups.map((group) => group.best));
  const allSoldOut = !searching && storeResults.length === total && allMatchingProductsSoldOut(products);

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
  const updateFilter = <K extends keyof ResultFilters>(key: K, value: ResultFilters[K]) => {
    setVisibleLimit(RESULTS_PER_PAGE);
    setFilters((current) => ({ ...current, [key]: value }));
  };
  const checkLabel = (site: Site) => {
    if (site.direct) return ["unknown", "직접 검색"];
    const check = checks[site.name];
    if (!check) return ["unknown", ""];
    if (check.status === 429) return ["limited", "제한"];
    if (check.status === 401 || check.status === 403) return ["limited", "접근 제한"];
    return check.ok ? ["ok", "응답 수신"] : ["failed", check.detail];
  };

  return (
    <main className="shell">
      <header className="app-header">
        <h1>FigureSearch</h1>
      </header>

      <form className="search-panel" role="search" onSubmit={(event) => { event.preventDefault(); void runSearch(); }}>
        <label className="sr-only" htmlFor="search">피규어 검색</label>
        <div className="search-row">
          <input id="search" type="search" enterKeyHint="search" autoFocus={isDesktop} value={term}
            onChange={(event) => setTerm(event.target.value)}
            onKeyDown={(event) => { if (event.key === "Enter" && event.nativeEvent.isComposing) event.preventDefault(); }}
            placeholder="상품명, 캐릭터, 모델 번호" />
          <button className="primary" disabled={searching && term.trim() === searchedTerm}>검색</button>
        </div>
        <div className="search-links">
          <ExternalLink url={shoppingUrl} className="shopping-link">
            네이버쇼핑에서 검색 <span aria-hidden="true">↗</span>
          </ExternalLink>
        </div>
        {notice && <p className="notice" role="status">{notice}</p>}
      </form>

      <details className="site-settings">
        <summary>검색할 판매처 <span className="muted">{selectedSites.length}/{allSites.length}곳</span></summary>
        <div className="selection-actions">
          <input aria-label="판매처 이름 또는 주소" value={filter} onChange={(event) => setFilter(event.target.value)} placeholder="판매처 찾기" />
          <button type="button" onClick={() => selectVisible(true)}>표시된 곳 선택</button>
          <button type="button" onClick={() => selectVisible(false)}>표시된 곳 해제</button>
          <button type="button" disabled={checking} onClick={() => void checkSites()}>{checking ? "확인 중…" : "접속 확인"}</button>
        </div>
        <div className="site-grid">{visibleSites.map((site) => {
          const [state, label] = checkLabel(site);
          return <label className="site-card" key={site.name}>
            <input type="checkbox" checked={selected.has(site.name)} onChange={() => toggle(site.name)} />
            <span className="site-info"><strong>{site.name}</strong><small>{site.host}</small></span>
            {label && <span className={`status ${state}`} title={checks[site.name]?.detail}>{label}</span>}
          </label>;
        })}</div>
        {!visibleSites.length && <p className="empty">일치하는 판매처가 없습니다.</p>}
      </details>

      {searchedTerm && <>
        <section className="results">
          <div className="results-heading">
            <h2>{searchedTerm} <span>{visibleGroups.length}개</span></h2>
            {minOffer && <div className="summary-price">
              <span>최저 표시가</span><ProductPrice product={minOffer} />
            </div>}
          </div>
          {allSoldOut && <p className="search-progress summary-warn" role="status">판매 중인 상품이 없습니다.</p>}
          {searching && <p className="search-progress" role="status">검색 중 · 판매처 {storeResults.length}/{total}곳</p>}
          <div className="result-toolbar">
            <div className="result-options">
              <select aria-label="상품 종류" value={filters.category} onChange={(e) => updateFilter("category", e.target.value as ResultFilters["category"])}>
                <option value="all">전체 종류 ({counts.total})</option>
                {counts.categories.map(([category, count]) => <option key={category} value={category}>{categoryLabels[category]} ({count})</option>)}
                {filters.category !== "all" && !counts.categories.some(([category]) => category === filters.category)
                  && <option value={filters.category}>{categoryLabels[filters.category]} (0)</option>}
              </select>
              <select aria-label="정렬 기준" value={filters.sort} onChange={(e) => updateFilter("sort", e.target.value as SortKey)}>
                <option value="relevance">관련도순</option>
                <option value="price-asc">낮은 가격순</option>
                <option value="price-desc">높은 가격순</option>
                <option value="store">판매처순</option>
              </select>
            </div>
            <div className="result-options">
              <label><input type="checkbox" checked={filters.hideSoldOut} onChange={(e) => updateFilter("hideSoldOut", e.target.checked)} /> 품절 제외</label>
              <label><input type="checkbox" checked={filters.hideUsed} onChange={(e) => updateFilter("hideUsed", e.target.checked)} /> 중고 제외</label>
              {hiddenCount > 0 && <label><input type="checkbox" checked={filters.showHidden} onChange={(e) => updateFilter("showHidden", e.target.checked)} /> 관련도 낮은 상품 포함</label>}
            </div>
          </div>

          {storeResults.length > 0 && <details className="store-status">
            <summary>판매처별 결과
              {unavailableStores.length > 0 && <span className="summary-warn">검색 불가 {unavailableStores.length}곳</span>}
              {directStores.length > 0 && <span className="muted">직접 검색 {directStores.length}곳</span>}
            </summary>
            <div className="store-grid">{[...unavailableStores, ...directStores, ...searchedStores].map((store) => (
              <ExternalLink key={store.store} url={store.searchUrl} className={`store-chip status-${store.status}`}>
                <strong>{store.store}</strong>
                <span>{store.status === "results" ? `${store.relevant}건` : store.message || statusLabels[store.status]}</span>
              </ExternalLink>
            ))}</div>
          </details>}

          {visibleGroups.length > 0 ? <div className="result-list">{visibleGroups.slice(0, visibleLimit).map(({ key, best: p, offers }) => (
            <article className="result-group" key={key}>
              <ExternalLink url={p.url} className="result-row">
                {p.imageUrl ? <img src={p.imageUrl} alt="" loading="lazy" referrerPolicy="no-referrer" /> : <span className="image-placeholder" />}
                <span className="result-main">
                  <strong>{p.name}</strong>
                  <small>
                    <span>{p.store}</span>
                    <span className="category">{categoryLabels[p.category]}</span>
                    {p.tags.map((tag) => <span key={tag} className={`tag tag-${tag}`}>{tagLabels[tag]}</span>)}
                    {p.release && <span>{p.release}</span>}
                    <span className={p.availability === "sold_out" ? "soldout" : ""}>{availabilityLabel(p)}</span>
                  </small>
                  {!p.relevant && <small className="missing">
                    {p.missing.length ? `일치하지 않은 조건 · ${p.missing.join(", ")}` : "검색한 상품 종류와 다름"}
                  </small>}
                </span>
                <ProductPrice product={p} />
              </ExternalLink>
              {offers.length > 1 && <details className="offer-details">
                <summary>판매 목록 비교 <span className="muted">{offers.length}건</span></summary>
                <ul className="offer-list">{offers.map((offer) => (
                  <li key={offer.id}>
                    <ExternalLink url={offer.url} className="offer-row">
                      <span>
                        <strong>{offer.store}</strong>
                        <small className={offer.availability === "sold_out" ? "soldout" : "muted"}>
                          {[availabilityLabel(offer), ...offer.tags.map((tag) => tagLabels[tag])].join(" · ")}
                        </small>
                      </span>
                      <ProductPrice product={offer} />
                    </ExternalLink>
                  </li>
                ))}</ul>
              </details>}
            </article>
          ))}</div> : !searching && <p className="empty">
            {storeResults.length > 0 && !searchedStores.length ? "통합 검색 결과를 받지 못했습니다. 판매처별 링크에서 직접 검색할 수 있습니다."
              : "현재 조건에 맞는 상품이 없습니다."}
          </p>}
          {visibleGroups.length > visibleLimit && <button className="more-results" type="button"
            onClick={() => setVisibleLimit((limit) => limit + RESULTS_PER_PAGE)}>
            상품 더 보기 <span className="muted">{visibleLimit}/{visibleGroups.length}</span>
          </button>}
        </section>
      </>}
    </main>
  );
}

export default App;
