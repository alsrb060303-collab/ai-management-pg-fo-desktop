# 메모 동기화 설정 (Phase 8b)

메모 동기화는 Firebase(Firestore + Authentication)를 쓴다. 프로젝트는 앱 개발자가 한 번 만들고, 앱에는 공개 설정값(API 키, 프로젝트 ID)만 들어간다. 비밀 값은 앱에 넣지 않는다.

## 1. Firebase 프로젝트 만들기
1. https://console.firebase.google.com → **프로젝트 추가**. 이름은 자유(예: `ai-management-sync`). Google 애널리틱스는 꺼도 된다.
2. 요금제는 **Spark(무료)** 그대로 둔다. 결제 계정을 연결하지 않으면 과금되지 않는다.

## 2. GitHub 로그인 켜기
1. 왼쪽 **빌드 › Authentication › 시작하기 › 로그인 방법 › GitHub**.
2. **사용 설정**을 켜고 Client ID / Client secret을 넣는다.
   - Client ID: 앱이 쓰는 GitHub OAuth App의 Client ID(`Ov23li...`).
   - Client secret: https://github.com/settings/developers 의 그 OAuth App에서 **Generate a new client secret**으로 만든 값. **Firebase에만** 넣고 앱이나 저장소에는 넣지 않는다.
3. 저장.

## 3. Firestore 만들기
1. **빌드 › Firestore Database › 데이터베이스 만들기**.
2. 위치는 `asia-northeast3 (서울)` 추천(나중에 못 바꾼다). **프로덕션 모드**로 시작.
3. **규칙** 탭에 이 저장소의 `firebase/firestore.rules` 내용을 그대로 붙여 넣고 **게시**.

## 4. 앱 설정값 넘기기
1. 프로젝트 개요 옆 톱니 › **프로젝트 설정 › 내 앱 › 웹 앱 추가(</>)**. 이름 자유, Hosting은 체크하지 않는다.
2. 나오는 설정에서 **`apiKey`**와 **`projectId`** 두 값만 개발에 넘긴다(`src-tauri/src/sync/mod.rs`의 `API_KEY`, `PROJECT_ID`). 둘 다 공개해도 되는 값이다.
   - 개발 중에는 환경 변수 `AIM_FIREBASE_API_KEY`, `AIM_FIREBASE_PROJECT`로도 넣을 수 있다.

## 5. 허용 목록에 계정 넣기 (요금제 전까지)
1. 앱에서 작업 탭으로 GitHub 로그인 → **설정 › 동기화**에 "아직 허용되지 않은 계정"과 **계정 ID**가 보인다. 복사.
2. Firestore **데이터** 탭 › **컬렉션 시작** › 컬렉션 ID `allowed` › 문서 ID에 그 계정 ID를 붙여 넣고 필드 없이(또는 아무 필드나) 저장.
3. 앱에서 **지금 동기화**를 누르면 동기화가 시작된다. 다른 기기에서도 같은 GitHub 계정으로 로그인하면 같은 계정 ID라 따로 추가할 필요 없다.

## 6. 규칙 다시 게시 (문서·라이브러리 동기화 추가, 2026-10-12)
문서 작성기 문서와 라이브러리가 새 컬렉션 `users/{uid}/files`로 동기화된다. 앱을 업데이트한 뒤 Firestore **규칙** 탭에 `firebase/firestore.rules`를 다시 붙여 넣고 **게시**해야 한다. 게시 전에는 메모만 동기화되고, 문서·라이브러리는 "동기화 오류 (403)"가 난다.

## 무료 한도 (Spark)
- 하루 쓰기 2만, 읽기 5만, 저장 1GiB. 프로젝트 전체 합계다.
- 앱은 열려 있는 동안 1분마다 변경분만 조회한다(변경이 없으면 조회 1회 = 읽기 1). 기기 한 대가 하루 종일 켜져 있으면 약 1,440 읽기.
- 넘으면 미국 태평양 시간 자정(한국 오후 4~5시)까지 동기화가 멈추고, 메모는 로컬에 그대로 남는다.
