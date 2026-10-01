import { describe, expect, it } from "vitest";
import { buildSearchUrl } from "./search-url";

describe("buildSearchUrl", () => {
  it("sets UTF-8 query values without corrupting static parameters", () => {
    const result = buildSearchUrl({
      name: "Example",
      url: "https://example.com/search?fixed=1&q={input}",
      encoding: "UTF-8",
    }, "미쿠 01");

    const parsed = new URL(result);
    expect(parsed.searchParams.get("fixed")).toBe("1");
    expect(parsed.searchParams.get("q")).toBe("미쿠 01");
  });

  it("encodes legacy query terms without Node-only runtime dependencies", () => {
    const result = buildSearchUrl({
      name: "Legacy",
      url: "https://example.com/search?skey=all&sword={input}",
      encoding: "EUC-KR",
    }, "미쿠");

    expect(result).toContain("skey=all");
    expect(result).toContain("sword=%EB%AF%B8%EC%BF%A0");
  });

  it("accepts a complete EUC-KR encoding result without mixing encodings", () => {
    const result = buildSearchUrl({
      name: "Legacy",
      url: "https://example.com/search?sword={input}",
      encoding: "EUC-KR",
    }, "미쿠", "%B9%CC%C4%ED");

    expect(result).toContain("sword=%B9%CC%C4%ED");
    expect(result).not.toContain("%EB%AF");
    expect(result).not.toContain("%25");
  });

  it("requires an encoder result when an EUC-KR profile explicitly skips fallback", () => {
    expect(() => buildSearchUrl({
      name: "Strict legacy",
      url: "https://example.com/search?q={input}",
      encoding: "EUC-KR",
      profile: { eucKrFallback: "skip" },
    }, "ミク")).toThrow(/encoder result/);
  });

  it("rejects placeholders outside a query value", () => {
    expect(() => buildSearchUrl({
      name: "Invalid",
      url: "https://example.com/{input}",
      encoding: "UTF-8",
    }, "figure")).toThrow(/query parameter/);
  });
});
