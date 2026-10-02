# FigureSearch

국내 피규어 판매처 여러 곳을 한 번에 검색해서, 결과를 앱 안에 모아 보여 주는 Windows 데스크톱 앱입니다. 상품이나 판매처를 누를 때만 외부 브라우저가 열립니다.

## 실행

Node.js와 Rust가 설치된 환경에서 다음 명령을 실행합니다.

```bash
npm install
npm run tauri dev
```

## 테스트

```bash
npm test            # sites.json 검증 + 프론트 필터/정렬 테스트
npm run test:core   # Rust: HTML 추출 · 관련도 · 상품 분류 · 백엔드 테스트
npm run test:all    # 둘 다
```

## 구조

```
src/                      React UI (결과 표시, 분류 필터, 판매처 선택)
src-tauri/src/lib.rs      Tauri 명령: 판매처 요청, 문자셋 디코딩, 번개장터 API, 결과 스트리밍
src-tauri/search-core/    검색 핵심 로직 (외부 의존성 없음)
  src/html.rs             망가진 HTML도 읽는 관대한 파서
  src/extract.rs          쇼핑몰 플랫폼별 상품 카드 추출 (Cafe24, 고도몰, 메이크샵, 영카트, 아임웹, 알라딘, 예스24, 범용)
  src/query.rs            검색어 분석, 판매처별 검색어 후보, 관련도 판정
  src/group.rs            판매처가 달라도 같은 상품인지 판단하는 묶음 키
  src/classify.rs         상품 분류(넨도로이드, figma, 스케일, 프라이즈 …)와 예약·중고·특전 태그
  data/aliases.txt        검색 별칭(한·일·영 표기, 약칭) — 직접 추가할 수 있어요
sites.json                판매처 목록
```

### 검색 흐름

1. 검색어를 분석해 단어 묶음을 만듭니다. `하츠네 미쿠`·`初音ミク`·`Hatsune Miku`·`미쿠`는 같은 묶음, `2301`·`1/7`·JAN 코드는 꼭 맞아야 하는 식별자, `피규어`·`정품`·제조사 이름은 선택 단어로 다룹니다.
2. 판매처마다 원래 검색어로 먼저 검색하고, 맞는 상품이 없으면 다른 후보(한국어 표기, 선택 단어를 뺀 검색어)로 한 번 더 시도합니다.
3. 페이지에서 검색 결과 영역만 골라 상품 카드를 읽습니다. 헤더·메뉴·추천상품·배너 링크는 제외하고, 소비자가·적립금 대신 실제 판매가를 읽습니다.
4. 상품명에 검색어가 실제로 들어 있는지 판정합니다. 맞지 않는 상품은 숨기고, "관련 낮은 결과 보기"로 확인할 수 있습니다(없는 단어 표시).
5. 상품을 종류별로 분류하고 예약·중고·특전·재판·비정품 의심 태그와 발매월을 붙입니다.
6. 여러 판매처의 같은 상품을 하나로 묶고, 판매처를 배송비를 포함한 낮은 가격순으로 보여 줍니다. 상품명에서 예약·정발·발매월 같은 판매처 문구, 띄어쓰기, 제조사 이름은 무시하고 `aliases.txt`의 표기(`하츠네 미쿠`·`初音ミク`·`Hatsune Miku`)를 같은 이름으로 보며, 중고와 비정품 의심 상품은 새 상품과 따로 묶습니다.

같은 서버에는 동시에 요청하지 않고, 판매처 하나가 실패해도 다른 판매처 결과는 계속 표시됩니다.

## 판매처 데이터 수정

`sites.json`이 판매처 목록의 원본입니다. `UTF-8` 또는 `EUC-KR` 그룹 안에 `name`과 `{input}` 자리표시자가 들어간 `url`을 추가합니다. 쇼핑몰 플랫폼은 URL에서 자동으로 판별하며, 필요하면 직접 지정할 수 있습니다.

```json
{"name": "어떤샵", "url": "https://example.com/product/search.html?keyword={input}", "platform": "cafe24",
 "profile": {"preferredLanguage": "japanese", "eucKrFallback": "UTF-8"}}
```

- `platform`: `cafe24`, `godomall`, `makeshop`, `youngcart`, `imweb`, `aladin`, `yes24`, `naver_store`, `bunjang`, `generic`
- `preferredLanguage`: 이 판매처에 먼저 보낼 검색어 표기(`korean`, `japanese`, `english`)
- `eucKrFallback`: EUC-KR로 표현할 수 없는 글자가 있을 때 `UTF-8`로 보내거나 `skip`(건너뛰기)
- `shipping`: 배송비 정책. `{"fee": 3000, "freeOver": 50000}`은 기본 배송비 3,000원, 50,000원 이상이면 무료라는 뜻이에요. 가격 비교와 최저가는 배송비를 더한 금액으로 계산하고, 정책이 없는 판매처는 "배송비 별도"로 표시해요. 상품명이나 배지에 "무료배송"이 있으면 그 상품은 0원으로 봐요.

## 판매처별 참고

- **네이버 스마트스토어·브랜드스토어**: 화면이 스크립트로 그려지고 자동 요청을 막아서 앱 안에서 검색하지 않습니다. 결과 화면의 판매처 버튼으로 브라우저에서 엽니다.
- **번개장터**: 공개 검색 API로 결과를 가져오며, 모든 상품에 `중고` 태그가 붙습니다.
- `429`(요청 제한)는 판매처가 자동 요청을 잠시 막은 것으로, 폐쇄나 URL 변경으로 판단하지 않습니다.

## 판매처 페이지 샘플 저장 (파서 개선용)

개발 모드(`npm run tauri dev`)나 환경 변수 `FIGURESEARCH_DEBUG=1`로 실행하면 각 판매처의 응답 HTML과 분석 결과가 저장됩니다.

```
%LOCALAPPDATA%\com.birowsi.figuresearch\logs\search-<검색 ID>\<판매처>-<검색어>.html / .json
```

결과가 이상한 판매처가 있으면 이 파일을 `src-tauri/search-core/tests/fixtures`에 넣고 테스트를 추가하면 됩니다.

## 배포 빌드

```bash
npm run tauri build
```

설치 파일은 `src-tauri/target/release/bundle` 아래에 생성됩니다.
