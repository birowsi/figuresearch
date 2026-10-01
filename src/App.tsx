import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-shell";
import { invoke } from "@tauri-apps/api/core";
import sitesData from "../sites.json";
import { buildSearchUrl, type SearchEncoding } from "./search-url";

type Site = { id: string; name: string; url: string; encoding: SearchEncoding; host: string };
type SiteCheck = { ok: boolean; status: number | null; detail: string };

const allSites: Site[] = Object.entries(sitesData).flatMap(([encoding, sites]) =>
  sites.map((site, index) => ({
    id: `${encoding}-${index}-${site.name}`,
    name: site.name,
    url: site.url,
    encoding: encoding as SearchEncoding,
    host: new URL(site.url).hostname.replace(/^www\./, ""),
  })),
);

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
  const [checks, setChecks] = useState<Record<string, SiteCheck>>({});
  const [checking, setChecking] = useState(false);
  const [showFailedOnly, setShowFailedOnly] = useState(false);
  const [skipFailed, setSkipFailed] = useState(true);

  useEffect(() => {
    localStorage.setItem("figure-search-selected", JSON.stringify([...selected]));
  }, [selected]);

  const visibleSites = useMemo(() => allSites.filter((site) =>
    (encoding === "all" || site.encoding === encoding) &&
    (!showFailedOnly || checks[site.id]?.ok === false) &&
    `${site.name} ${site.host}`.toLowerCase().includes(filter.toLowerCase()),
  ), [checks, encoding, filter, showFailedOnly]);
  const selectedSites = allSites.filter((site) => selected.has(site.id) && (!skipFailed || checks[site.id]?.ok !== false));

  const checkSites = async () => {
    setChecking(true);
    setNotice("판매처 상태를 확인하는 중...");
    const results = await Promise.all(allSites.map(async (site) => {
      try {
        const result = await invoke<SiteCheck>("check_site", { url: buildSearchUrl(site, "figure") });
        return [site.id, result] as const;
      } catch (error) {
        return [site.id, { ok: false, status: null, detail: String(error) }] as const;
      }
    }));
    setChecks(Object.fromEntries(results));
    setChecking(false);
    const failed = results.filter(([, result]) => !result.ok).length;
    setNotice(failed ? `${failed}곳은 응답이 없거나 주소가 변경되었을 수 있습니다.` : "모든 판매처가 응답했습니다.");
  };

  useEffect(() => { void checkSites(); }, []);

  const toggle = (id: string) => setSelected((current) => {
    const next = new Set(current);
    next.has(id) ? next.delete(id) : next.add(id);
    return next;
  });

  const runSearch = async () => {
    const normalized = term.trim();
    if (!normalized) {
      setNotice("검색어를 입력해 주세요.");
      return;
    }
    if (!selectedSites.length) {
      setNotice("검색할 사이트를 하나 이상 선택해 주세요.");
      return;
    }
    setQuery(normalized);
    const skipped = allSites.filter((site) => selected.has(site.id) && checks[site.id]?.ok === false).length;
    setNotice(`${selectedSites.length}곳의 검색 결과를 여는 중...${skipped ? ` (${skipped}곳 제외)` : ""}`);
    try {
      await Promise.all(selectedSites.map((site) => open(buildSearchUrl(site, normalized))));
      setNotice(`${selectedSites.length}곳의 검색 결과를 열었습니다.`);
    } catch {
      setNotice("일부 검색 결과를 열지 못했습니다. 외부 브라우저 설정을 확인해 주세요.");
    }
  };

  const selectVisible = (value: boolean) => setSelected((current) => {
    const next = new Set(current);
    visibleSites.forEach((site) => value ? next.add(site.id) : next.delete(site.id));
    return next;
  });

  return (
    <main className="shell">
      <header className="hero">
        <div>
          <span className="eyebrow">FIGURE SEARCH / DESKTOP</span>
          <h1>찾고 싶은 피규어를<br /><em>한 번에</em> 찾아보세요.</h1>
          <p>수동으로 모은 국내 판매처의 검색 결과를 한 화면에서 실행합니다.</p>
        </div>
        <div className="count-badge"><strong>{selectedSites.length}</strong><span>selected stores</span></div>
      </header>

      <section className="search-panel">
        <label htmlFor="search">무엇을 찾고 있나요?</label>
        <div className="search-row">
          <input id="search" autoFocus value={term} onChange={(event) => setTerm(event.target.value)}
            onKeyDown={(event) => event.key === "Enter" && void runSearch()} placeholder="예: 넨도로이드 하츠네 미쿠" />
          <button className="primary" onClick={() => void runSearch()}>검색하기 <span>↗</span></button>
        </div>
        {notice && <p className="notice">{notice}</p>}
      </section>

      <section className="toolbar">
        <div><h2>검색할 판매처</h2><span className="muted">{allSites.length}개 사이트 · {query ? `"${query}"` : "원하는 곳을 선택하세요"}</span></div>
        <div className="controls">
          <input value={filter} onChange={(event) => setFilter(event.target.value)} placeholder="판매처 검색..." />
          <select value={encoding} onChange={(event) => setEncoding(event.target.value as "all" | SearchEncoding)}>
            <option value="all">전체 인코딩</option><option value="UTF-8">UTF-8</option><option value="EUC-KR">EUC-KR</option>
          </select>
          <button className="refresh" disabled={checking} onClick={() => void checkSites()}>{checking ? "확인 중..." : "상태 새로고침"}</button>
        </div>
      </section>

      <div className="selection-actions">
        <label><input type="checkbox" checked={skipFailed} onChange={(event) => setSkipFailed(event.target.checked)} /> 실패 사이트 검색에서 제외</label>
        <label><input type="checkbox" checked={showFailedOnly} onChange={(event) => setShowFailedOnly(event.target.checked)} /> 실패 사이트만 보기</label>
        <button onClick={() => selectVisible(true)}>보이는 판매처 모두 선택</button>
        <button onClick={() => selectVisible(false)}>보이는 판매처 해제</button>
      </div>

      <section className="site-grid">
        {visibleSites.map((site) => (
          <label className={`site-card ${selected.has(site.id) ? "selected" : ""}`} key={site.id}>
            <input type="checkbox" checked={selected.has(site.id)} onChange={() => toggle(site.id)} />
            <span className="checkmark">✓</span>
            <span className="site-info"><strong>{site.name}</strong><small>{site.host}</small></span>
            <span className={`status ${checks[site.id] ? checks[site.id].ok ? "ok" : "failed" : "unknown"}`}>
              {checks[site.id] ? checks[site.id].status === 429 ? "제한" : checks[site.id].ok ? "정상" : "실패" : "미확인"}
            </span>
            <span className="encoding">{site.encoding}</span>
          </label>
        ))}
      </section>
      {!visibleSites.length && <div className="empty">조건에 맞는 판매처가 없습니다.</div>}
      <footer>검색 결과는 각 판매처의 새 창에서 열립니다. 사이트 주소와 검색 정책은 판매처 사정에 따라 바뀔 수 있습니다.</footer>
    </main>
  );
}

export default App;
