import { describe, expect, it } from "vitest";
import { applyFilters, categoryCounts, defaultFilters, formatPrice, lowestPrice, type Product } from "./results";

const product = (overrides: Partial<Product>): Product => ({
  id: overrides.name ?? "x", store: "A", name: "x", url: null, price: null, imageUrl: null,
  availability: "in_stock", category: "nendoroid", tags: [], release: null, relevance: 1,
  relevant: true, missing: [], query: "q", ...overrides,
});

const products = [
  product({ name: "넨도 미쿠", price: 55000 }),
  product({ name: "넨도 미쿠 품절", price: 40000, availability: "sold_out" }),
  product({ name: "중고 넨도", price: 30000, tags: ["used"], store: "번개장터" }),
  product({ name: "figma 미쿠", category: "figma", relevant: false, relevance: 0.5, price: 10 }),
  product({ name: "미쿠 스케일", category: "scale_figure", price: null, relevance: 0.8 }),
];

describe("result filtering", () => {
  it("hides irrelevant products unless asked", () => {
    expect(applyFilters(products, defaultFilters).map((p) => p.name)).not.toContain("figma 미쿠");
    const all = applyFilters(products, { ...defaultFilters, showHidden: true });
    expect(all.at(-1)?.name).toBe("figma 미쿠");
  });

  it("filters by category, sold-out and used", () => {
    const filters = { ...defaultFilters, category: "nendoroid" as const, hideSoldOut: true, hideUsed: true };
    expect(applyFilters(products, filters).map((p) => p.name)).toEqual(["넨도 미쿠"]);
  });

  it("sorts by price with unknown prices last", () => {
    const sorted = applyFilters(products, { ...defaultFilters, sort: "price-asc" }).map((p) => p.price);
    expect(sorted).toEqual([30000, 40000, 55000, null]);
  });

  it("counts categories and finds the lowest available relevant price", () => {
    expect(categoryCounts(products, defaultFilters)).toEqual([["nendoroid", 3], ["scale_figure", 1]]);
    expect(lowestPrice(products)).toBe(30000);
    expect(formatPrice(58500)).toBe("58,500원");
    expect(formatPrice(null)).toBe("가격 정보 없음");
  });
});
