import { describe, expect, it } from "vitest";
import { deduplicateResults, parseProviderResponse, parseProviderResults } from "./provider";
import authFixture from "./fixtures/naver-auth.html?raw";

const fixture = `<html><a href="/product/1"><img src="/one.jpg">하츠네 미쿠 넨도로이드 2301 ₩55,000</a><a href="/product/1">duplicate</a><a href="/product/2"><span>품절</span> 미쿠 figure</a></html>`;

describe("provider result normalization", () => {
  it("parses product links, prices, availability, images and absolute URLs", () => {
    const results = parseProviderResults("Fixture", "https://shop.test/search?q=x", "미쿠", fixture);
    expect(results[0]).toMatchObject({
      store: "Fixture", name: "하츠네 미쿠 넨도로이드 2301 ₩55,000",
      url: "https://shop.test/product/1", price: 55000, priceText: "55,000원",
      availability: "in-stock", imageUrl: "https://shop.test/one.jpg", query: "미쿠",
    });
    expect(results[1].availability).toBe("out-of-stock");
  });

  it("handles malformed and empty HTML and conservatively deduplicates", () => {
    expect(parseProviderResults("Empty", "https://shop.test", "x", "<broken")).toEqual([]);
    const results = parseProviderResults("Fixture", "https://shop.test", "x", fixture);
    expect(deduplicateResults([...results, ...results])).toHaveLength(results.length);
  });

  it("classifies auth/navigation pages and never emits those links as products", () => {
    const outcome = parseProviderResponse("megahousePW", "https://smartstore.naver.com/shop/search?q=x", "abc123", authFixture, 200, "https://nid.naver.com/login");
    expect(outcome.report.classification).toBe("login_required");
    expect(outcome.results).toEqual([]);
  });

  it("uses structured Product data before strict fallback", () => {
    const html = `<script type="application/ld+json">${JSON.stringify({
      "@type": "Product", name: "Nendoroid Miku", url: "/products/2301",
      image: "/miku.jpg", offers: { price: "55000", availability: "InStock" },
    })}</script>`;
    const outcome = parseProviderResponse("Fixture", "https://shop.test/search", "미쿠", html);
    expect(outcome.report.adapter).toBe("StrictGenericAdapter");
    expect(outcome.results[0]).toMatchObject({ name: "Nendoroid Miku", price: 55000, url: "https://shop.test/products/2301" });
  });

  it("classifies rate limits and bot pages without parsing anchors", () => {
    expect(parseProviderResponse("Limited", "https://shop.test", "x", "<a href='/p/1'>Product</a>", 429).report.classification).toBe("rate_limited");
    expect(parseProviderResponse("Bot", "https://shop.test", "x", "captcha robot check").report.classification).toBe("bot_challenge");
  });
});
