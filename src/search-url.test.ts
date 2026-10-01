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

  it("encodes Korean terms as EUC-KR bytes", () => {
    const result = buildSearchUrl({
      name: "Legacy",
      url: "https://example.com/search?skey=all&sword={input}",
      encoding: "EUC-KR",
    }, "미쿠");

    expect(result).toContain("skey=all");
    expect(result).toContain("sword=%B9%CC%C4%ED");
  });

  it("rejects placeholders outside a query value", () => {
    expect(() => buildSearchUrl({
      name: "Invalid",
      url: "https://example.com/{input}",
      encoding: "UTF-8",
    }, "figure")).toThrow(/query parameter/);
  });
});
