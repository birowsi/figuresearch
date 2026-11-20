use search_core::{
    Availability, Category, PageStatus, Platform, Tag, extract::detect_platform, process_html, query::analyze,
};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

#[test]
fn cafe24_reads_only_search_results_and_filters_irrelevant() {
    let html = fixture("cafe24_search.html");
    let url = "https://figurepresso.com/product/search.html?banner_action=&keyword=%EB%84%A8";
    let platform = detect_platform(url);
    assert_eq!(platform, Platform::Cafe24);
    let analysis = analyze("넨도로이드 미쿠");
    let out = process_html("피규어프레소", platform, &analysis, "넨도로이드 미쿠", 200, url, &html);

    assert_eq!(out.status, PageStatus::Results);
    assert!(out.scoped, "should scope to xans-search-result");
    let names: Vec<&str> = out.products.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names.len(), 4, "header BEST and recommend widgets excluded: {names:?}");
    assert!(!names.iter().any(|n| n.contains("BEST") || n.contains("추천")));

    let relevant: Vec<_> = out.products.iter().filter(|p| p.relevant).collect();
    assert_eq!(relevant.len(), 2, "{names:?}");

    let first = relevant.iter().find(|p| p.name.contains("16th")).unwrap();
    assert_eq!(first.name, "[26년 3월 발매] 넨도로이드 하츠네 미쿠 16th Anniversary Ver.");
    assert_eq!(first.price, Some(58_500), "sale price, not consumer price or points");
    assert_eq!(first.url.as_deref(), Some("https://figurepresso.com/product/넨도로이드-하츠네-미쿠-16th/1201/category/1/display/1/"));
    assert_eq!(first.image_url.as_deref(), Some("https://figurepresso.com/web/product/medium/202601/miku16.jpg"));
    assert_eq!(first.category, Category::Nendoroid);
    assert!(first.tags.contains(&Tag::Preorder));
    assert_eq!(first.release.as_deref(), Some("2026년 3월"));
    assert_eq!(first.availability, Availability::InStock);

    let v4x = relevant.iter().find(|p| p.name.contains("V4X")).unwrap();
    assert_eq!(v4x.price, Some(49_000));
    assert!(v4x.tags.contains(&Tag::Reissue));

    let figma = out.products.iter().find(|p| p.name.starts_with("figma")).unwrap();
    assert!(!figma.relevant);
    assert_eq!(figma.availability, Availability::SoldOut);
    assert_eq!(figma.missing, vec!["넨도로이드".to_string()]);

    let rin = out.products.iter().find(|p| p.name.contains("린")).unwrap();
    assert!(!rin.relevant);
}

#[test]
fn godomall_scoped_list_with_prices_soldout_and_badges() {
    let html = fixture("godomall_search.html");
    let url = "https://www.comiczone.co.kr/goods/goods_search.php?keyword=x";
    let analysis = analyze("블루 아카이브 아리스 1/7");
    let out = process_html("코믹존", detect_platform(url), &analysis, "블루 아카이브 아리스 1/7", 200, url, &html);
    assert!(out.scoped);
    assert_eq!(out.products.len(), 2);
    let scale = &out.products[0];
    assert!(scale.relevant);
    assert_eq!(scale.price, Some(219_000));
    assert_eq!(scale.url.as_deref(), Some("https://www.comiczone.co.kr/goods/goods_view.php?goodsNo=1000012345"));
    assert_eq!(scale.category, Category::ScaleFigure);
    assert_eq!(scale.availability, Availability::InStock, "hidden soldout overlay must not count");
    assert!(scale.tags.contains(&Tag::Preorder));
    assert_eq!(scale.release.as_deref(), Some("2026년 5월"));

    let acryl = &out.products[1];
    assert!(!acryl.relevant, "1/7 is required");
    assert_eq!(acryl.availability, Availability::SoldOut);
    assert_eq!(acryl.category, Category::Goods);
}

#[test]
fn aladin_split_price_nodes_and_book_category() {
    let html = fixture("aladin_search.html");
    let url = "https://www.aladin.co.kr/search/wsearchresult.aspx?SearchTarget=All&SearchWord=x";
    let analysis = analyze("주술회전");
    let out = process_html("알라딘", detect_platform(url), &analysis, "주술회전", 200, url, &html);
    assert_eq!(out.products.len(), 2);
    let book = out.products.iter().find(|p| p.name == "주술회전 26").unwrap();
    assert_eq!(book.price, Some(5_400));
    assert_eq!(book.category, Category::Book);
    assert_eq!(book.image_url.as_deref(), Some("https://image.aladin.co.kr/product/111/cover200.jpg"));
    let nendo = out.products.iter().find(|p| p.name.contains("넨도로이드")).unwrap();
    assert_eq!(nendo.category, Category::Nendoroid);
    assert_eq!(nendo.price, Some(62_000));
}

#[test]
fn generic_page_ignores_menus_and_other_domains() {
    let html = fixture("generic_noise.html");
    let url = "https://www.comicct.com/shop/search_result.php?search_str=x";
    let analysis = analyze("원피스 루피");
    let out = process_html("코믹시티", detect_platform(url), &analysis, "원피스 루피", 200, url, &html);
    let names: Vec<&str> = out.products.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["원피스 루피 기어5 프라이즈", "드래곤볼 손오공 피규어"]);
    assert!(out.products[0].relevant && !out.products[1].relevant);
    assert_eq!(out.products[0].price, Some(23_000));
    assert_eq!(out.products[0].category, Category::PrizeFigure);
    assert_eq!(out.status, PageStatus::Results);
}

#[test]
fn page_statuses() {
    let analysis = analyze("미쿠");
    let url = "https://a.kr/product/search.html?keyword=x";
    let empty = process_html("A", Platform::Cafe24, &analysis, "미쿠", 200, url, &fixture("empty_search.html"));
    assert_eq!(empty.status, PageStatus::Empty);
    assert!(empty.products.is_empty(), "header product must not leak");

    let limited = process_html("A", Platform::Cafe24, &analysis, "미쿠", 429, url, "<a href='/product/x/1/'>미쿠</a>");
    assert_eq!(limited.status, PageStatus::RateLimited);

    let bot = process_html("A", Platform::Generic, &analysis, "미쿠", 200, url, "<p>자동입력 방지를 위해 보안문자를 입력해 주세요</p>");
    assert_eq!(bot.status, PageStatus::Blocked);

    let login = process_html("A", Platform::NaverStore, &analysis, "미쿠", 200, "https://nid.naver.com/nidlogin.login", "<a>로그인</a>");
    assert_eq!(login.status, PageStatus::LoginRequired);

    let odd = process_html("A", Platform::Generic, &analysis, "미쿠", 200, url, "<div>뭔가 다른 페이지</div>");
    assert_eq!(odd.status, PageStatus::Unparsed);
    assert!(odd.status.worth_retry());
}

#[test]
fn charset_sniffing() {
    assert_eq!(search_core::sniff_charset(Some("text/html; charset=EUC-KR"), b"").as_deref(), Some("euc-kr"));
    let head = br#"<html><head><meta http-equiv="Content-Type" content="text/html; charset=euc-kr">"#;
    assert_eq!(search_core::sniff_charset(Some("text/html"), head).as_deref(), Some("euc-kr"));
    assert_eq!(search_core::sniff_charset(None, b"<meta charset=\"utf-8\">").as_deref(), Some("utf-8"));
    assert_eq!(search_core::sniff_charset(None, b"<html>"), None);
}

#[test]
fn reads_products_linked_only_from_onclick() {
    let html = r#"<ul class="prd_list">
      <li><img src="/img/a.jpg"><dl>
        <dd class="name" onClick="javascript:location.href='https://www.1004gundam.co.kr/mall/Itemdetails.php?cate=&itemno=11'">[예약] 넨도로이드 하츠네 미쿠</dd>
        <dd class="price">77,500원</dd></dl></li>
      <li><img src="/img/b.jpg"><dl>
        <dd class="name" onclick="location.href='/mall/Itemdetails.php?itemno=12'">figma 하츠네 미쿠</dd>
        <dd class="price">85,800원</dd></dl></li>
    </ul>"#;
    let url = "https://www.1004gundam.co.kr/mall/search.php?q=x";
    let out = process_html("천사건담", detect_platform(url), &analyze("미쿠"), "미쿠", 200, url, html);
    assert_eq!(out.status, PageStatus::Results);
    let found: Vec<(&str, Option<u64>)> = out.products.iter().map(|p| (p.name.as_str(), p.price)).collect();
    assert!(found.contains(&("[예약] 넨도로이드 하츠네 미쿠", Some(77_500))), "{found:?}");
    assert!(found.contains(&("figma 하츠네 미쿠", Some(85_800))), "{found:?}");
}

#[test]
fn adult_verification_redirect_has_its_own_status() {
    let url = "https://figuresailer.com/intro/adult_i.html?returnUrl=%2Fproduct%2Fsearch.html";
    let out = process_html("피규어세일러", detect_platform(url), &analyze("미쿠"), "미쿠", 200, url, "<html><body>성인인증</body></html>");
    assert_eq!(out.status, PageStatus::AgeVerificationRequired);
}

#[test]
fn zero_result_count_is_empty_and_204_is_an_error() {
    let url = "https://www.animate-onlineshop.co.kr/goods/goods_search.php?keyword=x";
    let html = r#"<div class="search_tit"><span>&quot;유우카&quot;</span> 검색결과 0개</div>"#;
    let out = process_html("애니메이트코리아", detect_platform(url), &analyze("유우카"), "유우카", 200, url, html);
    assert_eq!(out.status, PageStatus::Empty);
    let out = process_html("논노21", detect_platform(url), &analyze("유우카"), "유우카", 204, url, "");
    assert_eq!(out.status, PageStatus::HttpError);
}

#[test]
fn table_layout_prefers_bold_sale_price_over_points_column() {
    let url = "http://nonno21.com/shop/shopbrand.html?search=x";
    let html = r#"<table><tr>
      <td><a href="/shop/shopdetail.html?branduid=45808"><img src="/a.jpg"></a></td>
      <td><a href="/shop/shopdetail.html?branduid=45808">하츠네 미쿠 누들 스토퍼</a></td>
      <td>1140원</td>
      <td><div style="color:gray"><strike>42,000원</strike></div><b style="color:#C94C00">38,000원</b></td>
    </tr></table>"#;
    let out = process_html("논노21", detect_platform(url), &analyze("미쿠"), "미쿠", 200, url, html);
    assert_eq!(out.products[0].price, Some(38_000));
}

#[test]
fn human_verification_pages_are_blocked() {
    let analysis = analyze("미쿠");
    let cafe24 = "https://veritas-hub.cafe24.com/challenge?auth=x&url=y";
    let page = "<body>컨텐츠 바로가기 안전한 이용을 위해 간단한 확인이 필요해요.</body>";
    assert_eq!(process_html("피규어프레소", Platform::Cafe24, &analysis, "미쿠", 200, cafe24, page).status, PageStatus::Blocked);
    let url = "https://www.1004gundam.co.kr/mall/search.php?q=x";
    let page = "<body>자동등록방지를 위해 보안절차를 거치고 있습니다. Please prove that you are human.</body>";
    assert_eq!(process_html("천사건담", detect_platform(url), &analysis, "미쿠", 200, url, page).status, PageStatus::Blocked);
}

#[test]
fn figure_search_keeps_standalone_merchandise_out_of_primary_results() {
    let page = r#"<div class="search-result"><ul class="prdList">
      <li><a href="/product/main/1/">넨도로이드 하츠네 미쿠 2301</a></li>
      <li><a href="/product/keychain/2/">하츠네 미쿠 넨도로이드 플러스 러버 키 체인 단품 판매</a></li>
      <li><a href="/product/bundle/3/">넨도로이드 하츠네 미쿠 2301 키링 특전 포함</a></li>
      <li><a href="/product/stand/4/">넨도로이드 하츠네 미쿠 한정 스탠드(블랙)</a></li>
      <li><a href="/product/car/5/">넨도로이드 플러스 하츠네 미쿠 큐트로디 풀백카 단품 판매</a></li>
      <li><a href="/product/plamo/6/">넨도로이드 프라모 하츠네 미쿠</a></li>
    </ul></div>"#;
    let url = "https://example.kr/product/search.html?keyword=x";
    let search = |term| process_html("A", Platform::Cafe24, &analyze(term), term, 200, url, page);
    let figures = search("넨도로이드 하츠네 미쿠");
    assert_eq!(figures.relevant_count(), 2);
    assert!(figures.products.iter().any(|p| p.category == Category::Accessory && !p.relevant));
    assert!(figures.products.iter().any(|p| p.name.contains("풀백카") && p.category == Category::Goods && !p.relevant));
    assert!(figures.products.iter().any(|p| p.category == Category::Plamo && !p.relevant));
    assert!(search("넨도로이드 미쿠 키체인").products.iter().any(|p| p.category == Category::Goods && p.relevant));
    assert_eq!(search("미쿠 풀백카").relevant_count(), 1);
}

#[test]
fn model_number_search_hides_standalone_decals_but_keeps_the_full_kit() {
    let page = r#"<ul class="prdList">
      <li><a href="/product/kit/1/">RG RX-78-2 건담 Ver.2.0</a></li>
      <li><a href="/product/kit-with-decals/2/">RG RX-78-2 건담 Ver.2.0 데칼 포함</a></li>
      <li><a href="/product/decals/3/">ガンダムデカ-ル GD84 RG RX-78-2 ガンダム用 (84)</a></li>
    </ul>"#;
    let url = "https://example.kr/product/search.html?keyword=x";
    let search = |term| process_html("A", Platform::Cafe24, &analyze(term), term, 200, url, page);
    let kits = search("RG RX-78-2");
    assert_eq!(kits.relevant_count(), 2);
    assert!(kits.products.iter().any(|p| p.category == Category::Accessory && !p.relevant));
    assert!(search("RG RX-78-2 데칼").products.iter().any(|p| p.category == Category::Accessory && p.relevant));
}

#[test]
fn explicit_listing_conditions_match_titles_and_badges() {
    let page = r#"<ul class="prdList">
      <li><a href="/product/new/1/">넨도로이드 하츠네 미쿠</a></li>
      <li><a href="/product/used/2/">넨도로이드 하츠네 미쿠 2301</a><span class="badge">중고</span></li>
      <li><a href="/product/preorder/3/">[예약] 넨도로이드 하츠네 미쿠</a></li>
      <li><a href="/product/reissue/4/">넨도로이드 하츠네 미쿠 (재판)</a></li>
    </ul>"#;
    let url = "https://example.kr/product/search.html?keyword=x";
    for (condition, expected) in [("중고", "/used/2/"), ("예약", "/preorder/3/"), ("재판", "/reissue/4/")] {
        let term = format!("미쿠 {condition}");
        let out = process_html("A", Platform::Cafe24, &analyze(&term), &term, 200, url, page);
        assert_eq!(out.relevant_count(), 1, "{condition}");
        assert!(out.products[0].url.as_ref().unwrap().ends_with(expected));
        assert!(out.products[1..].iter().all(|p| p.missing.contains(&condition.to_string())));
    }
}

#[test]
fn makeshop_listing_identity_is_stable_across_searches() {
    let url = "https://example.kr/shop/shopbrand.html?search=x";
    let analysis = analyze("미쿠");
    let listing = |keyword| {
        let html = format!(r#"<a href="/shop/shopdetail.html?branduid=99&amp;search={keyword}">하츠네 미쿠</a>"#);
        process_html("A", Platform::MakeShop, &analysis, keyword, 200, url, &html).products.remove(0)
    };
    let original = listing("original");
    let fallback = listing("fallback");
    assert_ne!(original.url, fallback.url);
    assert_eq!(original.id, fallback.id);
}
