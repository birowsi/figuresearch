import json
import time
from selenium import webdriver
from selenium.webdriver.chrome.options import Options
from urllib.parse import quote

# JSON 파일 로드 및 사이트 정렬 함수
def load_sites_config(filename="sites.json"):
    # sites.json 파일을 읽어와서 사이트 정보 목록을 정렬해서 반환
    with open(filename, "r", encoding="utf-8") as f:
        return sorted([
            {"name": site["name"], "url": site["url"], "encoding": encoding}
            for encoding, sites in json.load(f).items()  # 각 인코딩별로 사이트 정보 읽기
            for site in sites  # 각 사이트의 이름, URL, 인코딩 정보 저장
        ], key=lambda site: site["name"])  # 사이트 이름을 기준으로 정렬

# 검색어 인코딩 함수
def encode_search_term(search_term, encoding):
    # 검색어를 주어진 인코딩 방식에 맞게 URL 인코딩
    return quote(search_term.encode("euc-kr")) if encoding == "EUC-KR" else quote(search_term)
    # EUC-KR이면 'euc-kr'로 인코딩하고, 그 외는 기본 URL 인코딩 방식 사용

# 사이트 탭 열기 함수
def open_search_tabs(search_term):
    sites = load_sites_config()  # 사이트 정보 로드
    if not sites:
        return  # 사이트 정보가 없으면 종료

    driver = webdriver.Chrome(options=Options())  # 크롬 브라우저 실행

    for idx, site in enumerate(sites):
        # 각 사이트마다 인코딩된 검색어를 넣어 URL을 만듬
        encoded_term = encode_search_term(search_term, site["encoding"])
        url = site["url"].replace("{input}", encoded_term)  # URL에서 {input}을 검색어로 대체

        # 사이트 열기 메시지 출력
        print(f"사이트 열기: {site['name']}")

        if idx == 0:
            driver.get(url)  # 첫 번째 탭은 바로 열기
        else:
            driver.execute_script(f"window.open('{url}');")  # 나머지 탭은 새 창으로 열기

    driver.maximize_window()  # 브라우저 최대화

    # 첫 번째 탭으로 돌아가기
    driver.switch_to.window(driver.window_handles[0])  # 첫 번째 탭으로 포커스 이동

    # 브라우저 종료 후 세션 접근 방지 (무한 대기 방지)
    try:
        # 모든 창이 닫힐 때까지 대기
        while driver.window_handles:
            time.sleep(1)  # 창이 열려있으면 대기
    except Exception:
        pass  # 이미 세션이 종료된 경우 예외 처리

    driver.quit()  # 모든 작업 끝나면 브라우저 종료

# 실행 부분
if __name__ == "__main__":
    search_term = input("검색할 캐릭터 이름을 입력하세요: ")  # 사용자로부터 검색어 입력 받기
    open_search_tabs(search_term)  # 입력받은 검색어로 사이트 탭 열기
