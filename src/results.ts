// Types and pure helpers for search results produced by the Rust backend.

export type Category =
  | "nendoroid" | "nendoroid_doll" | "figma" | "action_figure" | "scale_figure" | "pop_up_parade"
  | "prize_figure" | "kuji" | "plamo" | "gacha" | "plush" | "goods" | "accessory" | "book" | "figure" | "other";
export type Tag = "preorder" | "used" | "bonus" | "reissue" | "bootleg";
export type Availability = "in_stock" | "sold_out" | "unknown";
export type PageStatus =
  | "results" | "no_relevant" | "empty" | "unparsed" | "login_required" | "rate_limited"
  | "blocked" | "http_error" | "network_error" | "link_only" | "skipped";

export type Product = {
  id: string;
  store: string;
  name: string;
  url: string | null;
  price: number | null;
  imageUrl: string | null;
  availability: Availability;
  category: Category;
  tags: Tag[];
  release: string | null;
  relevance: number;
  relevant: boolean;
  missing: string[];
  query: string;
  /** Same for listings of the same product across stores (computed in Rust); empty if unknown. */
  groupKey: string;
};

export type StoreResult = {
  searchId: string;
  store: string;
  platform: string;
  status: PageStatus;
  searchUrl: string | null;
  products: Product[];
  relevant: number;
  hidden: number;
  message: string | null;
  attempts: { query: string; url: string; status: PageStatus; httpStatus: number | null; products: number; relevant: number }[];
  durationMs: number;
};

export const categoryLabels: Record<Category, string> = {
  nendoroid: "넨도로이드",
  nendoroid_doll: "넨도로이드 돌",
  figma: "figma",
  action_figure: "액션 피규어",
  scale_figure: "스케일",
  pop_up_parade: "팝업 퍼레이드",
  prize_figure: "프라이즈",
  kuji: "이치방쿠지",
  plamo: "프라모델",
  gacha: "가챠·식완",
  plush: "인형",
  goods: "굿즈",
  accessory: "부속·케이스",
  book: "도서",
  figure: "기타 피규어",
  other: "기타",
};

export const tagLabels: Record<Tag, string> = {
  preorder: "예약",
  used: "중고",
  bonus: "특전·한정",
  reissue: "재판",
  bootleg: "비정품 의심",
};

export const statusLabels: Record<PageStatus, string> = {
  results: "결과",
  no_relevant: "맞는 상품 없음",
  empty: "결과 없음",
  unparsed: "읽기 실패",
  login_required: "로그인 필요",
  rate_limited: "요청 제한",
  blocked: "차단됨",
  http_error: "오류",
  network_error: "연결 실패",
  link_only: "브라우저로 열기",
  skipped: "건너뜀",
};

export type SortKey = "relevance" | "price-asc" | "price-desc" | "store";
export type ResultFilters = {
  category: Category | "all";
  hideSoldOut: boolean;
  hideUsed: boolean;
  showHidden: boolean;
  sort: SortKey;
};

export const defaultFilters: ResultFilters = {
  category: "all",
  hideSoldOut: false,
  hideUsed: false,
  showHidden: false,
  sort: "price-asc",
};

export const formatPrice = (price: number | null) => (price === null ? "가격 정보 없음" : `${price.toLocaleString("ko-KR")}원`);

const availabilityRank: Record<Availability, number> = { in_stock: 0, unknown: 1, sold_out: 2 };

const byPrice = (a: Product, b: Product, direction: 1 | -1) =>
  a.price === null ? (b.price === null ? 0 : 1) : b.price === null ? -1 : (a.price - b.price) * direction;

function compareProducts(a: Product, b: Product, sort: SortKey) {
  if (a.relevant !== b.relevant) return a.relevant ? -1 : 1;
  switch (sort) {
    case "price-asc": return byPrice(a, b, 1);
    case "price-desc": return byPrice(a, b, -1);
    case "store": return a.store.localeCompare(b.store, "ko") || b.relevance - a.relevance;
    default:
      return b.relevance - a.relevance ||
        availabilityRank[a.availability] - availabilityRank[b.availability] ||
        byPrice(a, b, 1);
  }
}

export function sortProducts(products: Product[], sort: SortKey) {
  return [...products].sort((a, b) => compareProducts(a, b, sort));
}

/** Listings with the same key are the same product; used and bootleg listings stay apart. */
export const productKey = (p: Product) => p.groupKey || `id:${p.id}`;

export type ProductGroup = {
  key: string;
  /** Cheapest available listing first, sold-out and unknown prices last. */
  offers: Product[];
  best: Product;
};

const offerOrder = (a: Product, b: Product) =>
  Number(a.availability === "sold_out") - Number(b.availability === "sold_out") || byPrice(a, b, 1);

export function groupProducts(products: Product[], sort: SortKey): ProductGroup[] {
  const groups = new Map<string, Product[]>();
  for (const p of products) {
    const key = productKey(p);
    groups.set(key, [...(groups.get(key) ?? []), p]);
  }
  const result = [...groups.entries()].map(([key, offers]) => {
    offers.sort(offerOrder);
    return { key, offers, best: offers[0] };
  });
  // Rank a group by its cheapest offer, but with the best relevance any offer has.
  const representative = (g: ProductGroup): Product => ({
    ...g.best,
    relevant: g.offers.some((p) => p.relevant),
    relevance: Math.max(...g.offers.map((p) => p.relevance)),
  });
  return result.sort((a, b) => compareProducts(representative(a), representative(b), sort));
}

/** Products passing every filter except the category (used for chip counts). */
function baseFilter(products: Product[], filters: ResultFilters) {
  return products.filter((p) =>
    (filters.showHidden || p.relevant) &&
    !(filters.hideSoldOut && p.availability === "sold_out") &&
    !(filters.hideUsed && p.tags.includes("used")));
}

export function applyFilters(products: Product[], filters: ResultFilters) {
  const base = baseFilter(products, filters);
  const visible = filters.category === "all" ? base : base.filter((p) => p.category === filters.category);
  return sortProducts(visible, filters.sort);
}

export const applyFiltersGrouped = (products: Product[], filters: ResultFilters) =>
  groupProducts(applyFilters(products, filters), filters.sort);

export function categoryCounts(products: Product[], filters: ResultFilters) {
  const counts = new Map<Category, number>();
  for (const p of baseFilter(products, filters)) counts.set(p.category, (counts.get(p.category) ?? 0) + 1);
  return [...counts.entries()].sort((a, b) => b[1] - a[1]);
}

export function lowestPrice(products: Product[]) {
  const prices = products.filter((p) => p.relevant && p.price !== null && p.availability !== "sold_out").map((p) => p.price!);
  return prices.length ? Math.min(...prices) : null;
}

export const isFailure = (status: PageStatus) =>
  ["unparsed", "login_required", "rate_limited", "blocked", "http_error", "network_error"].includes(status);
