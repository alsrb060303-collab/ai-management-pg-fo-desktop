# 동기화 · 앱 계정 설정 (Phase 8b, 9g)

동기화는 Firebase(Firestore + Authentication)를 쓴다. 사용자는 앱 계정(이메일·Google·GitHub, 모두 같은 Firebase 계정)으로 로그인하고, 데이터는 그 계정(`users/{uid}`)에 저장된다. 프로젝트는 앱 개발자가 한 번 만들고, 앱에는 공개 설정값(API 키, 프로젝트 ID)만 들어간다. 비밀 값은 앱에 넣지 않는다.

## 1. Firebase 프로젝트 만들기
1. https://console.firebase.google.com → **프로젝트 추가**. 이름은 자유(예: `ai-management-sync`). Google 애널리틱스는 꺼도 된다.
2. 요금제는 **Spark(무료)** 그대로 둔다. 결제 계정을 연결하지 않으면 과금되지 않는다.

## 2. GitHub 로그인 켜기
1. 왼쪽 **빌드 › Authentication › 시작하기 › 로그인 방법 › GitHub**.
2. **사용 설정**을 켜고 Client ID / Client secret을 넣는다.
   - Client ID: 앱이 쓰는 GitHub OAuth App의 Client ID(`Ov23li...`).
   - Client secret: https://github.com/settings/developers 의 그 OAuth App에서 **Generate a new client secret**으로 만든 값. **Firebase에만** 넣고 앱이나 저장소에는 넣지 않는다.
3. 저장.

## 2-1. 이메일 · Google 로그인 켜기 (앱 계정, 2026-10-13)
1. **Authentication › 로그인 방법 › 새 제공업체 추가 › 이메일/비밀번호** → 첫 번째 스위치(이메일/비밀번호)만 켜고 저장. 이메일 링크(비밀번호 없는 로그인)는 끈다.
2. **새 제공업체 추가 › Google** → 사용 설정, 프로젝트 지원 이메일 고르고 저장. Firebase가 Google OAuth 클라이언트("Web client (auto created by Google Service)")를 알아서 만든다. **클라이언트 ID·보안 비밀은 앱에 넣지 않는다**(Firebase 서버가 교환한다).
3. Google이 앱으로 돌아올 주소를 등록한다: https://console.cloud.google.com/apis/credentials (위쪽 프로젝트 선택에서 **Firebase 프로젝트와 같은 프로젝트**(`ai-manager-7d33c`)를 고른다. 다른 Google Cloud 프로젝트의 클라이언트를 고치면 `redirect_uri_mismatch`가 그대로 난다) › **OAuth 2.0 클라이언트 ID › Web client (auto created by Google Service)** › **승인된 리디렉션 URI**에 `http://localhost:47823/auth` 추가 › 저장. (반영까지 몇 분 걸릴 수 있다.)
   - 맞는 클라이언트인지 확인: 클라이언트 ID가 앱이 쓰는 것(`332336151846-t5ktm1go…`, 앞 숫자가 프로젝트 번호)과 같아야 한다.
   - 로그인 때 Google이 `redirect_uri_mismatch`를 보여주면 오류 화면의 "요청 세부정보"에 나온 주소를 그대로 추가한다.
4. **Authentication › 설정 › 승인된 도메인**에 `localhost`가 있는지 확인(기본으로 있다).
5. **Authentication › 설정 › 사용자 계정 연결**은 기본값 **"동일한 이메일 주소에 계정 연결"**(이메일당 계정 하나)을 그대로 둔다. 같은 이메일로 다른 방법을 쓰면 앱이 "원래 방법으로 로그인한 뒤 계정 설정에서 연결"하라고 안내한다.

### 기존 GitHub 동기화 사용자
- 예전 버전은 작업 탭의 GitHub 로그인으로 동기화했다. 그 기기를 업데이트하면 앱이 같은 GitHub 계정으로 앱 계정에 자동 로그인한다(계정 ID 그대로라 데이터·허용 목록 유지).
- 다른 컴퓨터에서 이메일이나 Google로 쓰고 싶으면, 먼저 GitHub로 로그인한 상태에서 **계정 › 로그인 방법**에서 비밀번호 설정 또는 Google 연결을 한다. 이후엔 어느 방법으로 로그인해도 같은 계정이다.

## 3. Firestore 만들기
1. **빌드 › Firestore Database › 데이터베이스 만들기**.
2. 위치는 `asia-northeast3 (서울)` 추천(나중에 못 바꾼다). **프로덕션 모드**로 시작.
3. **규칙** 탭에 이 저장소의 `firebase/firestore.rules` 내용을 그대로 붙여 넣고 **게시**.

## 4. 앱 설정값 넘기기
1. 프로젝트 개요 옆 톱니 › **프로젝트 설정 › 내 앱 › 웹 앱 추가(</>)**. 이름 자유, Hosting은 체크하지 않는다.
2. 나오는 설정에서 **`apiKey`**와 **`projectId`** 두 값만 개발에 넘긴다(`src-tauri/src/sync/mod.rs`의 `API_KEY`, `PROJECT_ID`). 둘 다 공개해도 되는 값이다.
   - 개발 중에는 환경 변수 `AIM_FIREBASE_API_KEY`, `AIM_FIREBASE_PROJECT`로도 넣을 수 있다.

## 5. 허용 목록에 계정 넣기 (요금제 전까지)
1. 앱 왼쪽 아래 **로그인**(또는 **설정 › 동기화 › 로그인**)으로 앱 계정에 로그인 → **설정 › 동기화**에 "아직 허용되지 않은 계정"과 **계정 ID**가 보인다. 복사.
2. Firestore **데이터** 탭 › **컬렉션 시작** › 컬렉션 ID `allowed` › 문서 ID에 그 계정 ID를 붙여 넣고 필드 없이(또는 아무 필드나) 저장.
3. 앱에서 **지금 동기화**를 누르면 동기화가 시작된다. 다른 기기에서도 같은 앱 계정(연결된 어느 방법이든)으로 로그인하면 같은 계정 ID라 따로 추가할 필요 없다.

## 6. 규칙 다시 게시 (문서·라이브러리 동기화 추가, 2026-10-12)
문서 작성기 문서와 라이브러리가 새 컬렉션 `users/{uid}/files`로 동기화된다. 앱을 업데이트한 뒤 Firestore **규칙** 탭에 `firebase/firestore.rules`를 다시 붙여 넣고 **게시**해야 한다. 게시 전에는 메모만 동기화되고, 문서·라이브러리는 "동기화 오류 (403)"가 난다.

## 7. 규칙 다시 게시 (작업 트리 기록 동기화 추가, Phase 14)
작업 트리의 GitHub 로그와 프롬프트 로그가 새 컬렉션 `users/{uid}/activity`로 동기화된다(이벤트 하나가 문서 하나, 문서 ID는 이벤트 ID의 해시). 앱을 업데이트한 뒤 Firestore **규칙** 탭에 `firebase/firestore.rules`를 **다시 붙여 넣고 게시**해야 한다. 게시 전에는 메모·문서·라이브러리는 동기화되지만 작업 트리 기록은 "동기화 오류 (403)"가 난다.
- 프롬프트 로그는 앞 500자만, 토큰·키처럼 보이는 문자열은 `[가림]`으로 가린 채 올라간다. 설정에서 프롬프트 로그 동기화만 끌 수 있다(끄면 올리지도, 다른 기기에서 올린 것을 받지도 않는다).
- 보관 기간(기본 90일)이 지난 이벤트는 삭제 표시(tombstone)로 바뀌어 다른 기기에도 전해진다. 문서는 지워지지 않고 `deleted: true`로 남는다.

## 8. 규칙 다시 게시 (설정 동기화 추가)
테마·언어·글자 크기·알림·기본 도구 같은 이식 가능한 설정이 새 컬렉션 `users/{uid}/prefs`로 동기화된다(키 하나가 문서 하나, 문서 ID는 키 이름). 앱을 업데이트한 뒤 Firestore **규칙** 탭에 `firebase/firestore.rules`를 **다시 붙여 넣고 게시**해야 한다. 게시 전에는 나머지는 동기화되지만 설정은 "동기화 오류 (403)"가 난다.
- 동기화되는 키는 앱에 고정된 허용 목록(`src-tauri/src/prefs/mod.rs`의 `SYNCED_KEYS`)뿐이다. 최근 프로젝트·경로별 도구·패널 너비·탭 같은 기기별 값은 이 기기에만 남는다.
- 설정은 앱 데이터의 `prefs.db`에 저장돼 앱을 업데이트해도 남는다. 값은 4096자까지만 올라간다.

## 무료 한도 (Spark)
- 하루 쓰기 2만, 읽기 5만, 저장 1GiB. 프로젝트 전체 합계다.
- 앱은 열려 있는 동안 1분마다 변경분만 조회한다(변경이 없으면 조회 1회 = 읽기 1). 기기 한 대가 하루 종일 켜져 있으면 약 1,440 읽기.
- 넘으면 미국 태평양 시간 자정(한국 오후 4~5시)까지 동기화가 멈추고, 메모는 로컬에 그대로 남는다.
