import { describe, expect, it } from "vitest";
import { analyzeSearch, generateQueryCandidates, selectQueryCandidate } from "./search-query";

describe("search query analysis", () => {
  it("keeps original input and normalizes Unicode and whitespace", () => {
    const result = analyzeSearch("  ﾊﾂﾈ　ﾐｸ   01 ");
    expect(result.original).toBe("  ﾊﾂﾈ　ﾐｸ   01 ");
    expect(result.normalized).toBe("ハツネ ミク 01");
  });

  it("only treats valid JAN/EAN values as strong identifiers", () => {
    expect(analyzeSearch("4904810912345").identifiers.some((item) => item.strong)).toBe(false);
    expect(analyzeSearch("8806091234567").identifiers.some((item) => item.strong)).toBe(false);
    expect(analyzeSearch("4006381333931").identifiers.some((item) => item.strong)).toBe(true);
  });

  it("expands known aliases without requiring an AI service", () => {
    const result = analyzeSearch("넨도 하츠네 미쿠");
    expect(result.japanese).toContain("初音ミク");
    expect(result.english).toContain("Nendoroid");
    expect(selectQueryCandidate(result, { preferredLanguage: "japanese" })).toContain("初音ミク");
  });

  it("keeps strong identifiers ahead of shorter generic candidates", () => {
    const result = analyzeSearch("하츠네 미쿠 4006381333931");
    const candidates = generateQueryCandidates(result);
    expect(candidates[0].value).toContain("4006381333931");
  });

  it("preserves contextual numeric product numbers and scales through aliases", () => {
    expect(selectQueryCandidate(analyzeSearch("하츠네 미쿠 넨도로이드 2301"))).toContain("2301");
    expect(selectQueryCandidate(analyzeSearch("1/7 初音ミク"))).toContain("1/7");
  });

  it("does not match aliases inside unrelated words and deduplicates expansions", () => {
    expect(analyzeSearch("Mikuni").english).toEqual([]);
    expect(analyzeSearch("미쿠라").korean).toEqual([]);
    const result = analyzeSearch("GSC 굿스마일");
    expect(new Set(result.english).size).toBe(result.english.length);
  });

  it("does not classify arbitrary four digit numbers without product context", () => {
    expect(analyzeSearch("2024").identifiers).toEqual([]);
    expect(analyzeSearch("넨도로이드 2301").identifiers.some((item) => item.value === "2301")).toBe(true);
  });

  it("requires a meaningful product-number shape", () => {
    expect(analyzeSearch("AB12 A123").identifiers.filter((item) => item.kind === "product-number")).toEqual([]);
    expect(analyzeSearch("RX-78").identifiers.some((item) => item.value === "RX-78")).toBe(true);
  });
});
