import aliasData from "./data/search-aliases.json";

export type SearchLanguage = "korean" | "japanese" | "english";
export type SearchProfile = {
  preferredLanguage?: SearchLanguage;
  shortQuery?: boolean;
  eucKrFallback?: "UTF-8" | "skip";
};
export type SearchAlias = {
  canonical: string;
  korean: string[];
  japanese: string[];
  english: string[];
  abbreviations: string[];
};
export type SearchIdentifier = {
  value: string;
  kind: "JAN" | "EAN" | "product-number" | "scale" | "product-family";
  strong: boolean;
};
export type QueryCandidate = {
  value: string;
  reasons: string[];
  score: number;
  strongIdentifiers: string[];
  aliasAccuracy: number;
  unnecessaryTokens: number;
};
export type SearchAnalysis = {
  original: string;
  normalized: string;
  coreTerms: string[];
  identifiers: SearchIdentifier[];
  korean: string[];
  japanese: string[];
  english: string[];
};

export const searchAliases = aliasData as SearchAlias[];
const compact = (value: string) => value.normalize("NFKC").trim().replace(/\s+/gu, " ");
const lower = (value: string) => compact(value).toLocaleLowerCase();

function validJanEan(value: string) {
  if (![8, 12, 13, 14].includes(value.length)) return false;
  const body = value.slice(0, -1);
  const check = Number(value.at(-1));
  const sum = [...body].reverse().reduce((total, digit, index) =>
    total + Number(digit) * (index % 2 === 0 ? 3 : 1), 0);
  return (10 - (sum % 10)) % 10 === check;
}

function isTokenAlias(normalized: string, term: string) {
  const escaped = compact(term).split(" ").map((part) => part.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&")).join("\\s+");
  return new RegExp(`(?:^|\\s)${escaped}(?=\\s|$)`, "iu").test(normalized);
}

function findAliases(normalized: string) {
  return searchAliases.filter((alias) => [
    ...alias.korean, ...alias.japanese, ...alias.english, ...alias.abbreviations,
  ].some((term) => isTokenAlias(normalized, term)));
}

function findIdentifiers(normalized: string): SearchIdentifier[] {
  const identifiers: SearchIdentifier[] = [];
  for (const token of normalized.split(" ")) {
    if (/^\d{8,14}$/u.test(token) && validJanEan(token)) {
      identifiers.push({ value: token, kind: token.length === 13 ? "EAN" : "JAN", strong: true });
    } else if (/^1\/\d{1,4}$/u.test(token)) {
      identifiers.push({ value: token, kind: "scale", strong: true });
    } else if (/^(?:[A-Z]{2,5}[-_]\d{2,}[A-Z0-9-]*|[A-Z]{2,5}\d{3,}[A-Z0-9-]*)$/iu.test(token)) {
      identifiers.push({ value: token, kind: "product-number", strong: true });
    } else if (/^\d{2,6}$/u.test(token) && normalized.split(" ").some((part) => /넨도|nendo|figure|피규어|figma|scale|스케일/iu.test(part))) {
      identifiers.push({ value: token, kind: "product-number", strong: true });
    }
  }
  for (const alias of findAliases(normalized)) {
    identifiers.push({ value: alias.canonical, kind: "product-family", strong: false });
  }
  return [...new Map(identifiers.map((item) => [`${item.kind}:${item.value}`, item])).values()];
}

export function analyzeSearch(input: string): SearchAnalysis {
  const original = input;
  const normalized = compact(input);
  const aliases = findAliases(normalized);
  return {
    original,
    normalized,
    coreTerms: normalized ? normalized.split(" ") : [],
    identifiers: findIdentifiers(normalized),
    korean: [...new Set(aliases.flatMap((alias) => alias.korean))],
    japanese: [...new Set(aliases.flatMap((alias) => alias.japanese))],
    english: [...new Set(aliases.flatMap((alias) => alias.english))],
  };
}

function mergeTerms(base: string, extra: string) {
  const existing = new Set(base.split(" ").map(lower));
  return `${base} ${extra}`.trim().split(/\s+/u).filter((term, index, all) =>
    !index || !existing.has(lower(term)) || all.slice(0, index).every((previous) => lower(previous) !== lower(term))).join(" ");
}

function addCandidate(map: Map<string, QueryCandidate>, value: string, reasons: string[], analysis: SearchAnalysis, aliasAccuracy: number) {
  const normalized = compact(value);
  if (!normalized) return;
  const strongIdentifiers = analysis.identifiers.filter((item) => item.strong && normalized.includes(item.value)).map((item) => item.value);
  const unnecessaryTokens = Math.max(0, analysis.coreTerms.length - normalized.split(" ").length);
  const score = strongIdentifiers.length * 1000 + aliasAccuracy * 100 - unnecessaryTokens * 10 - normalized.length / 100;
  const candidate = { value: normalized, reasons, score, strongIdentifiers, aliasAccuracy, unnecessaryTokens };
  const previous = map.get(normalized);
  if (!previous || score > previous.score) map.set(normalized, candidate);
}

export function generateQueryCandidates(analysis: SearchAnalysis, profile: SearchProfile = {}) {
  const candidates = new Map<string, QueryCandidate>();
  const strong = analysis.identifiers.filter((item) => item.strong).map((item) => item.value);
  addCandidate(candidates, analysis.normalized, ["original"], analysis, 0);
  for (const language of ["korean", "japanese", "english"] as const) {
    const terms = analysis[language].join(" ");
    if (terms) addCandidate(candidates, mergeTerms(terms, strong.join(" ")), [language, "preserved identifiers"], analysis, 1);
  }
  if (profile.preferredLanguage) {
    const terms = analysis[profile.preferredLanguage].join(" ");
    if (terms) addCandidate(candidates, mergeTerms(terms, strong.join(" ")), ["preferred language", "preserved identifiers"], analysis, 2);
  }
  if (profile.shortQuery && strong.length) addCandidate(candidates, mergeTerms(strong.join(" "), analysis.identifiers.filter((item) => item.kind === "product-family").map((item) => item.value).join(" ")), ["short query"], analysis, 1);
  if (!candidates.size) addCandidate(candidates, analysis.normalized, ["original"], analysis, 0);
  return [...candidates.values()].sort((a, b) =>
    b.score - a.score || b.strongIdentifiers.length - a.strongIdentifiers.length ||
    b.aliasAccuracy - a.aliasAccuracy || a.unnecessaryTokens - b.unnecessaryTokens || a.value.length - b.value.length);
}

export function selectQueryCandidate(analysis: SearchAnalysis, profile?: SearchProfile) {
  return generateQueryCandidates(analysis, profile)[0]?.value ?? "";
}
