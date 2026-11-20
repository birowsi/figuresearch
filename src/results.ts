// Types and pure helpers for search results produced by the Rust backend.

export type Category =
  | "nendoroid" | "nendoroid_doll" | "figma" | "action_figure" | "scale_figure" | "pop_up_parade"
  | "prize_figure" | "kuji" | "plamo" | "gacha" | "plush" | "goods" | "accessory" | "book" | "figure" | "other";
export type Tag = "preorder" | "used" | "bonus" | "reissue" | "bootleg";
export type Availability = "in_stock" | "sold_out" | "unknown";
export type PageStatus =
  | "results" | "no_relevant" | "empty" | "unparsed" | "login_required" | "rate_limited"
  | "blocked" | "http_error" | "network_error" | "link_only" | "age_verification_required" | "skipped";

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
  groupTitleKey?: string;
  /** The listing itself says shipping is free. */
  freeShipping: boolean;
  /** Shipping fee for this listing (filled in from the store policy); null when unknown. */
  shipping?: number | null;
};

/** Store shipping policy from sites.json: flat fee, free from `freeOver` won. */
export type ShippingPolicy = { fee: number; freeOver?: number };

export function shippingFee(p: Product, policy: ShippingPolicy | undefined): number | null {
  if (p.freeShipping) return 0;
  if (!policy) return null;
  if (policy.freeOver !== undefined && p.price !== null && p.price >= policy.freeOver) return 0;
  return policy.fee;
}

export const withShipping = (products: Product[], policies: Record<string, ShippingPolicy | undefined>) =>
  products.map((p) => ({ ...p, shipping: shippingFee(p, policies[p.store]) }));

/** What the buyer pays for this listing alone; unknown shipping counts as 0. */
export const totalPrice = (p: Product) => (p.price === null ? null : p.price + (p.shipping ?? 0));

export const shippingLabel = (p: Product) =>
  p.price === null ? "" : p.shipping === 0 ? "무료배송" : p.shipping ? `배송비 ${p.shipping.toLocaleString("ko-KR")}원 포함` : "배송비 별도";

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
  unparsed: "목록 읽기 오류",
  login_required: "로그인 필요",
  age_verification_required: "성인 인증 후 직접 검색",
  rate_limited: "요청 제한",
  blocked: "자동 검색 차단",
  http_error: "응답 오류",
  network_error: "연결 오류",
  link_only: "직접 검색",
  skipped: "검색어 전송 불가",
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
  sort: "relevance",
};

export const formatPrice = (price: number | null) => (price === null ? "가격 정보 없음" : `${price.toLocaleString("ko-KR")}원`);

const availabilityRank: Record<Availability, number> = { in_stock: 0, unknown: 1, sold_out: 2 };

function byPrice(a: Product, b: Product, direction: 1 | -1) {
  const x = totalPrice(a), y = totalPrice(b);
  return x === null ? (y === null ? 0 : 1) : y === null ? -1 : (x - y) * direction;
}

function compareProducts(a: Product, b: Product, sort: SortKey) {
  if (a.relevant !== b.relevant) return a.relevant ? -1 : 1;
  switch (sort) {
    case "price-asc": return byPrice(a, b, 1) || b.relevance - a.relevance;
    case "price-desc": return byPrice(a, b, -1) || b.relevance - a.relevance;
    case "store": return a.store.localeCompare(b.store, "ko") || b.relevance - a.relevance;
    default:
      return b.relevance - a.relevance ||
        availabilityRank[a.availability] - availabilityRank[b.availability] ||
        byPrice(a, b, 1) || a.name.localeCompare(b.name, "ko") || a.id.localeCompare(b.id);
  }
}

export function sortProducts(products: Product[], sort: SortKey) {
  return [...products].sort((a, b) => compareProducts(a, b, sort));
}

/** Listings with the same key are the same product; used and bootleg listings stay apart. */
export const productKey = (p: Product) => p.groupKey || `id:${p.id}`;

function comparisonKey(products: Product[]) {
  const numberedEditions = new Map<string, string | null>();
  for (const product of products) {
    const { groupTitleKey, groupKey } = product;
    if (!groupTitleKey || !groupKey || groupKey === groupTitleKey) continue;
    const existing = numberedEditions.get(groupTitleKey);
    numberedEditions.set(groupTitleKey, existing === undefined || existing === groupKey ? groupKey : null);
  }
  return (product: Product) => {
    // A title without a number can join one known edition. Conflicting
    // numbers stay separate, including when a result filter hides one of them.
    const { groupTitleKey, groupKey } = product;
    const match = groupTitleKey && groupTitleKey === groupKey ? numberedEditions.get(groupTitleKey) : null;
    return match ?? productKey(product);
  };
}

export type ProductGroup = {
  key: string;
  /** Cheapest available listing first, sold-out and unknown prices last. */
  offers: Product[];
  best: Product;
};

const offerOrder = (a: Product, b: Product) =>
  Number(a.availability === "sold_out") - Number(b.availability === "sold_out") ||
  byPrice(a, b, 1) || a.store.localeCompare(b.store, "ko") || a.id.localeCompare(b.id);

export function groupProducts(products: Product[], sort: SortKey, keyFor = comparisonKey(products)): ProductGroup[] {
  const groups = new Map<string, Product[]>();
  for (const p of products) {
    const key = keyFor(p);
    const offers = groups.get(key);
    if (offers) offers.push(p); else groups.set(key, [p]);
  }
  const result = [...groups.entries()].map(([key, offers]) => {
    offers.sort(offerOrder);
    return { key, offers, best: offers.find((offer) => offer.relevant) ?? offers[0] };
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

export function applyFiltersGrouped(products: Product[], filters: ResultFilters) {
  const base = baseFilter(products, filters);
  const visible = filters.category === "all" ? base : base.filter((p) => p.category === filters.category);
  return groupProducts(visible, filters.sort, comparisonKey(products));
}

export function categoryCounts(products: Product[], filters: ResultFilters) {
  const groups = new Map<Category, Set<string>>();
  const keys = new Set<string>();
  const keyFor = comparisonKey(products);
  for (const p of baseFilter(products, filters)) {
    const key = keyFor(p);
    keys.add(key);
    const category = groups.get(p.category) ?? new Set<string>();
    category.add(key);
    groups.set(p.category, category);
  }
  const counts = [...groups.entries()].map(([category, items]) => [category, items.size] as const);
  return { total: keys.size, categories: counts.sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])) };
}

export function lowestOffer(products: Product[]) {
  return products.reduce<Product | null>((best, p) => {
    if (!p.relevant || p.price === null || p.availability === "sold_out") return best;
    return !best || offerOrder(p, best) < 0 ? p : best;
  }, null);
}

export function allMatchingProductsSoldOut(products: Product[]) {
  const matches = products.filter((product) => product.relevant);
  return matches.length > 0 && matches.every((product) => product.availability === "sold_out");
}

export const isFailure = (status: PageStatus) =>
  ["unparsed", "login_required", "rate_limited", "blocked", "http_error", "network_error"].includes(status);
