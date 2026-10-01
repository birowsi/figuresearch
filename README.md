# FigureSearch

수동으로 수집한 국내 피규어 판매처를 한 번에 검색하는 Windows 데스크톱 앱입니다. 검색어를 입력하고 판매처를 선택하면 각 사이트의 검색 결과가 외부 브라우저 탭으로 열립니다.

## 실행

Node.js와 Rust가 설치된 환경에서 다음 명령을 실행합니다.

```bash
npm install
npm run tauri dev
```

## 배포 빌드

```bash
npm run tauri build
```

설치 파일은 `src-tauri/target/release/bundle` 아래에 생성됩니다. Tauri는 ChromeDriver나 별도 Selenium 설치가 필요하지 않습니다.

## 판매처 데이터 수정

`sites.json`이 판매처 데이터의 원본입니다. 기존 형식을 유지하며 `UTF-8` 또는 `EUC-KR` 그룹 안에 `name`과 `{input}` 자리표시자가 포함된 `url`을 추가할 수 있습니다. 판매처 페이지의 검색 URL이 변경되면 이 파일만 수정하면 됩니다.

앱은 사이트 상품 목록을 수집하거나 계정 정보를 다루지 않습니다. 검색 결과는 각 판매처의 정책에 따라 외부 브라우저에서 표시됩니다.

## 기술 스택

- Tauri 2 + Rust: 가벼운 데스크톱 창과 외부 링크 열기
- React + TypeScript + Vite: 검색 UI
- `sites.json`: 사용자가 직접 관리하는 판매처 목록
