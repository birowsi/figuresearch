export type SearchEncoding = "UTF-8" | "EUC-KR";
export type SearchSite = {
  name: string;
  url: string;
  encoding: SearchEncoding;
  profile?: { eucKrFallback?: "UTF-8" | "skip"; preferredLanguage?: "korean" | "japanese" | "english"; shortQuery?: boolean };
};

const INPUT_MARKER = "__FIGURE_SEARCH_INPUT__";

/**
 * Builds a search URL from a configured URL template.
 *
 * The template must contain exactly one `{input}` query value. Static query
 * parameters are parsed and serialized by URLSearchParams instead of being
 * assembled through string replacement.
 */
export function buildSearchUrl(site: SearchSite, term: string, encodedEucKrTerm?: string) {
  const matches = site.url.match(/\{input\}/g)?.length ?? 0;
  if (matches !== 1) {
    throw new Error(`${site.name}: URL must contain exactly one {input} placeholder`);
  }

  const parsed = new URL(site.url.replace("{input}", INPUT_MARKER));
  const entries = [...parsed.searchParams.entries()];
  const inputEntries = entries.filter(([, value]) => value === INPUT_MARKER);
  if (inputEntries.length !== 1) {
    throw new Error(`${site.name}: {input} must be used as a query parameter value`);
  }

  const inputKey = inputEntries[0][0];
  if (site.encoding === "UTF-8") {
    parsed.searchParams.set(inputKey, term);
    return parsed.toString();
  }

  if (site.profile?.eucKrFallback === "skip" && !encodedEucKrTerm) {
    throw new Error(`${site.name}: EUC-KR query requires an encoder result`);
  }
  const encodedTerm = encodedEucKrTerm ?? encodeURIComponent(term);
  const query = entries
    .map(([key, value]) => `${encodeURIComponent(key)}=${value === INPUT_MARKER ? encodedTerm : encodeURIComponent(value)}`)
    .join("&");
  parsed.search = query;
  return parsed.toString();
}
