# 개발 계획

> 임시 코드네임: `ai-management` (정식 이름 미정)
> 최종 갱신: 2026-10-10
> 근거 자료와 출처: [`docs/research.md`](./research.md)

Claude Code 같은 AI 코딩 도구로 작업할 때, 터미널 옆에 붙어서 다음 세 가지를 돕는 데스크탑 사이드바 앱.

1. **문서 관리**: CLAUDE.md, skill, subagent를 만들고, 어디에 뭐가 적용되는지 보여주고, 프로젝트별로 적용·제외
2. **작업 관리**: 브랜치, worktree, 변경 상태, git 조작, GitHub PR·이슈
3. **메모/TODO**: 브랜치별 메모를 여러 기기에서 동기화

---

## 1. 확정된 결정

| 항목 | 결정 | 이유 |
|---|---|---|
| 지원 OS | Windows 먼저 → Linux(X11) → macOS(미정) | 사용자의 주 OS가 Windows |
| 앱 프레임워크 | **Tauri 2.12** (Rust ≥ 1.90) | MIT/Apache-2.0, 가볍고(상주형 사이드바에 중요) Win32 API를 Rust에서 바로 호출 가능 |
| 프론트엔드 | React 19 + TypeScript + Vite + Tailwind CSS v4 + Zustand | 보편적이고 생태계가 큼 |
| 린트/포맷 | Biome (TS), rustfmt + clippy (Rust) | |
| 라이선스 | **Apache-2.0** | OSI 승인 라이선스. 무료 코드 서명(SignPath) 조건도 충족 |
| 앱 이름 | 임시 코드네임 `ai-management` | 이름에 "Claude"·"Anthropic" 사용 금지(Anthropic 정책) |
| 창 구성 | **메인 창 + 사이드바, 한 번에 하나만 표시** | 실행하면 메인 창. "사이드바로 전환"하면 메인 창을 닫고 사이드바가 터미널 옆에 붙음. 트레이·단축키로 다시 메인 창 |
| 디자인 | Claude 데스크탑 앱 같은 스타일(따뜻한 무채색, 왼쪽 사이드바, 강조색 하나). 테마는 시스템 따라감 + 설정에서 라이트/다크 | 로고·이름·전용 폰트는 쓰지 않고 무료 폰트(OFL)로 대체 |
| 도킹 방식 | **지금 쓰는 창 따라가기** | Windows Terminal / VS Code / Claude 데스크탑 중 활성 창 옆에 자동으로 붙음 |
| 대상 AI 도구 | Claude Code만 (다른 도구는 나중에) | |
| 문서 생성 | 템플릿 + 로컬 `claude` CLI headless 호출 | 사용자 구독 그대로 사용, 비용 0 |
| 메모 저장 | 로컬 SQLite가 원본 + 교체 가능한 동기화 어댑터 | 오프라인에서도 동작 |
| 동기화 | **무료: 로컬만 / 유료 고객: Firebase 동기화** | 중앙 프로젝트의 공유 한도·악용 문제를 유료 고객으로 한정해서 해결 (research.md 4장) |
| 동기화 로그인 | GitHub 로그인 → Firebase (`GithubAuthProvider.credential`) | GitHub 연동 로그인과 통합. Device Flow 토큰을 Firebase가 받는지는 PoC 필요 |
| git | 설치된 `git` CLI 실행 | worktree·push·인증(GCM)까지 레퍼런스 동작 |
| GitHub | GitHub App + Device Flow 로그인 | 클라이언트 시크릿 불필요(오픈소스 OK), 레포 단위 권한 |
| 배포 | GitHub Releases + Tauri updater, NSIS 설치 파일 | 무료 |
| 외부 블록을 사용자 파일에 써넣을 때의 저작자 표시(NOTICE) 의무 | 공개 전 법률 검토 |
| React Flow의 WebKitGTK(Linux)·WebView2 실제 조작(드래그, 연결 거부) | 실기 테스트 필요 |
| 작업 중 사용량 한도에 걸리면 | 풀리는 시각에 바로 이어서 하도록 예약(send_later) |
| 코드 서명 | 처음엔 미서명 → 공개 릴리스 후 SignPath Foundation(OSS 무료) 신청 | 비용 0 |

## 2. 아키텍처

```
┌─────────────────────── WebView (React) ───────────────────────┐
│  TitleBar · TabNav                                            │
│  [문서] 설정 지도 · 라이브러리 · 생성     [작업] git · GitHub   │
│  [메모] 브랜치별 메모/TODO                                     │
└──────────────── invoke / event (Tauri IPC) ───────────────────┘
┌────────────────────────── Rust core ──────────────────────────┐
│ dock          Win32 이벤트 훅으로 대상 창 추적 (cfg(windows))   │
│ claude_config 설정 파일 스캔 · 적용/제외 (settings.local.json)  │
│ generator     `claude -p` 실행 → 결과 미리보기 → 저장           │
│ git           `git` CLI 래퍼                                    │
│ github        Device Flow 로그인 · PR/이슈 (octocrab)          │
│ store         SQLite + sync adapter                             │
└───────────────────────────────────────────────────────────────┘
```

원칙
- OS 종속 코드는 `cfg(windows)` 등으로 분리하고, 다른 OS에는 대체 동작(화면 가장자리 고정)을 둔다.
- Claude Code 설정 파일을 쓸 때는 **읽기 → 병합 → 쓰기**로 처리하고, 모르는 키는 보존한다.
- Claude 로그인 토큰(`~/.claude/.credentials.json` 등)은 **절대 읽거나 저장하지 않는다.** 사용자가 설치한 `claude` 바이너리를 수정 없이 실행만 한다.

## 3. 기능별 설계

### 3.1 사이드바 도킹 (Windows)
- 전용 스레드에서 `SetWinEventHook`(out-of-context)과 메시지 루프를 돌린다.
  - 전역: `EVENT_SYSTEM_FOREGROUND`로 활성 창이 바뀌는 걸 감지 → 대상 앱이면 그 창으로 전환
  - 대상 창: `EVENT_OBJECT_LOCATIONCHANGE`, `MINIMIZESTART/END`, `MOVESIZESTART/END`, `CLOAKED/UNCLOAKED`(가상 데스크톱), `DESTROY`
- 대상 창 판별
  - Windows Terminal: 창 클래스 `CASCADIA_HOSTING_WINDOW_CLASS` + 프로세스 이름 대비책
  - VS Code / Claude 데스크탑(Electron): 클래스 접두사 `Chrome_WidgetWin_` + 프로세스 이름 + 창 제목
- 위치 계산: `DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS)`로 보이는 테두리 기준 좌표 → `PhysicalPosition`으로 이동 (DPI 배율 문제 회피)
- z-order: 대상 창을 owner로 지정하면 대상 바로 위에만 뜨고 최소화도 같이 됨. 단 owner가 닫히면 사이드바도 파괴되므로 재생성 처리 필요 → **PoC로 검증**
- 포커스: `focusable:false`면 클릭해도 입력이 안 됨 → 메모 입력 시에만 활성화하고, 끝나면 터미널로 포커스 반환
- 관리자 권한 터미널(UIPI): 이벤트 수신이 막힐 수 있음 → 감지해서 안내
- Linux: X11만 도킹 가능, Wayland는 화면 가장자리 고정으로 대체

### 3.2 Claude Code 설정 지도
보여줄 범위(우선순위 높은 순):
- managed: `C:\Program Files\ClaudeCode\` (`CLAUDE.md`, `managed-settings.json`)
- user: `%USERPROFILE%\.claude\` (`CLAUDE.md`, `settings.json`, `rules/`, `skills/`, `agents/`, `commands/`)
- project: `CLAUDE.md` 또는 `.claude/CLAUDE.md`, `.claude/settings.json`, `.claude/rules/`, `.claude/skills/`, `.claude/agents/`, `.mcp.json`
- local: `CLAUDE.local.md`, `.claude/settings.local.json`
- 주의: `AGENTS.md`는 CLAUDE.md 계열이 하나도 없을 때만 읽힘. 기능이 버전마다 다르므로 `claude --version`으로 기능을 켜고 끈다.

### 3.3 라이브러리 적용 / 제외
- 라이브러리 위치: **앱 전용 데이터 폴더**. `~/.claude/skills`에 두면 모든 프로젝트에 자동 적용되므로 사용하지 않는다.
- 적용: 프로젝트 `.claude/skills/<name>/`, `.claude/agents/<name>.md`로 **복사**(Windows 심볼릭 링크는 관리자/개발자 모드 필요). 앱이 넣은 파일은 해시로 기록해서 추적.
  - 팀 공유용(커밋) / 개인용(`.git/info/exclude`에 추가) 중 선택
- 제외 (`.claude/settings.local.json`):
  - skill: `"skillOverrides": { "<name>": "off" }`
  - subagent: `"permissions": { "deny": ["Agent(<name>)"] }`
  - plugin: `"enabledPlugins": { "<name>@<marketplace>": false }` 또는 `claude plugin disable --scope local`
- 주의
  - Claude Code는 이 파일을 **자기가 처음 만들 때만** git 전역 제외에 추가한다. 앱이 만들면 직접 `.git/info/exclude`에 추가해야 한다.
  - 이미 git에 커밋된 `settings.local.json`은 건드리지 않는다(저장소 제공 파일로 취급됨).
  - plugin 안의 skill은 `skillOverrides`로 개별 제외가 안 된다.

**Phase 3 결정 (2026-10-10)**
- 라이브러리 위치: 사용자가 고른 폴더(git 레포로 관리하면 여러 PC에서 공유 가능). 고르지 않으면 앱 데이터 폴더의 `library/`.
  - 구조: `<라이브러리>/skills/<이름>/SKILL.md`(+부속 파일), `<라이브러리>/agents/<이름>.md`
- 적용·끄기 모두 항목마다 **나만**(개인) / **팀**(공유)을 고른다.
  - 적용: 둘 다 `.claude/skills|agents/`로 복사. 나만이면 `.git/info/exclude`에 추가.
  - 끄기: 나만이면 `.claude/settings.local.json`, 팀이면 `.claude/settings.json`에 기록.
- 앱이 넣은 파일은 앱 데이터 폴더의 `applied.json`에 해시와 함께 기록한다(프로젝트에 관리 파일을 추가하지 않음). 사용자가 수정한 복사본은 묻지 않고 지우거나 덮어쓰지 않는다.

### 3.4 문서 생성
1. 기술 스택 감지: `package.json`, `Cargo.toml`, `pyproject.toml`, `go.mod` 등
2. 템플릿으로 즉시 초안 생성 (오프라인)
3. (선택) `claude -p` 호출로 맞춤 생성
   - `--output-format json --json-schema <schema> --permission-mode dontAsk --allowedTools "Read,Glob,Grep" --max-turns <n>`
   - 읽기 전용으로 실행 → 앱에서 diff 미리보기 → 사용자가 확인하면 앱이 파일 저장
4. 보안: `--bare`가 아닌 `-p`는 프로젝트의 hooks와 `.mcp.json`을 **신뢰 확인 없이 실행**한다. 모르는 레포에서는 경고하고 `--settings '{"disableAllHooks":true}'` 등을 쓴다.

### 3.5 Git
- `git` CLI 실행. Git for Windows가 없을 수 있으므로(Claude Code에선 선택 사항) 감지해서 안내한다.
- Claude Code가 만드는 worktree(`.claude/worktrees/<name>`, 브랜치 `worktree-<name>`)도 함께 표시한다.

### 3.6 GitHub
- GitHub App(선택한 레포만 접근) + Device Flow. 토큰은 8시간, refresh token은 6개월(사용할 때마다 교체).
- 토큰은 OS 자격 증명 저장소(`keyring` 4.x)에 보관한다. Tauri Stronghold 플러그인은 v3에서 제거 예정이라 쓰지 않는다.
- API 한도: 사용자당 시간당 5,000회(사용자의 다른 앱과 공유).

### 3.7 메모 & 동기화
- 로컬 SQLite가 원본(`<앱 데이터>/memo.db`). 프로젝트는 정규화한 git remote URL로 식별(없으면 공통 git 폴더, git 밖이면 폴더 경로). 같은 저장소의 clone·worktree는 메모를 공유한다.
- 브랜치마다 메모 1개 + 할 일 목록. 터미널에서 브랜치를 바꾸면 4초 안에 따라 바뀐다(창이 보일 때만 확인). 삭제된 브랜치의 메모도 남아 있다.
- 동기화 대비: 모든 행에 `updated_at`·`device_id`, 삭제는 tombstone(`deleted = 1`).
- 동기화는 어댑터 인터페이스로 분리해서 백엔드를 갈아끼울 수 있게 한다.
- 무료 사용자는 로컬만, **유료 고객만 Firebase 동기화**를 쓴다.
  - Firestore 경로: `users/{uid}/repos/{repoKey}/branches/{branchKey}/items/{itemId}` (repoKey = 정규화한 remote URL의 SHA-256)
  - 보안 규칙: 본인 uid + **유료 권한 확인** + 크기·필드 검증. `match /users/{userId}/{document=**}` 사용.
  - 충돌: `updatedAt`(+deviceId) 비교를 규칙에서 강제한다. Firestore 자체 LWW는 도착 순서 기준이라 그대로 쓰면 안 됨.
  - 읽기·쓰기 절약: `updatedAt > lastSync` 증분 조회, 쓰기는 모아서 전송.
- 로그인: GitHub(Device Flow) 토큰 → `GithubAuthProvider.credential(token)` → `signInWithCredential`. 앱 안 팝업 로그인은 안 되므로 쓰지 않는다.
- 유료 권한 부여에는 결제 웹훅을 받을 서버 코드가 필요하다(Cloud Functions는 Blaze 요금제 필요, 또는 다른 호스팅).

### 3.8 메인 창 & 문서 작성기 (2026-10-10 결정)
- 창: `sidebar`(기존 창, 라벨만 변경) + `main`(새 창, 작업 표시줄에 표시, 크기 조절 가능). 같은 Vite 앱에서 창 라벨로 화면을 나누고, 작성기 코드는 메인 창에서만 지연 로드.
  - 창 생성은 항상 비동기 작업에서 한다(Windows에서 동기 핸들러 안 생성 시 교착).
  - 권한은 창별 capability 파일로 나눈다.
- 디자인: 왼쪽 사이드바(새 문서, 메뉴, 최근 문서) + 본문. 폰트는 UI Pretendard, 영문 Hanken Grotesk, 본문 미리보기 Source Serif 4, 코드 JetBrains Mono (모두 OFL, 앱에 포함). 아이콘 Lucide.
- 작성기(유니티 셰이더 그래프 방식)
  - 화면: 왼쪽 블록 목록(검색·분류) / 가운데 캔버스(@xyflow/react, MIT) / 오른쪽 실제 파일 미리보기(줄 수 표시, CLAUDE.md는 200줄 권장선)
  - 블록 종류: 지침(CLAUDE.md용), 규칙(`.claude/rules`, 경로 지정 가능), skill, subagent
  - 출력 노드: CLAUDE.md(나만이면 CLAUDE.local.md) / 규칙 / skills / agents. 연결 가능한 종류만 연결된다.
  - CLAUDE.md 안의 블록 순서는 출력 노드 안 목록에서 드래그로 정한다.
  - 파일 쓰기: 종류별 위치로 나눠서 쓴다. CLAUDE.md는 앱이 쓴 구역만 HTML 주석 표시로 감싸서 관리하고(Claude Code는 이 주석을 읽지 않음), 그 밖의 내용은 건드리지 않는다. 쓰기 전에 변경 미리보기를 보여주고, 앱이 쓰지 않았거나 사용자가 수정한 파일은 묻지 않고 덮어쓰지 않는다.
  - 실행 취소/다시 실행은 직접 구현한다(React Flow의 Pro 예제 코드는 라이선스상 쓰지 않음).
- 블록 출처
  - 내장 블록: 직접 작성(본문 영어, 이름·설명 한국어). 공식 문서는 인용하지 않고 요약해서 출처 링크만 단다.
  - 외부 인기 항목: Apache-2.0/MIT 항목만, 커밋 SHA 고정 + 라이선스 표시(`src-tauri/catalog/external/`, `THIRD_PARTY_NOTICES.md`). hooks·scripts·MCP 설정 등 실행되는 부분은 가져오지 않는다.
  - "최신 받아오기": 같은 저장소의 같은 파일을 최신 커밋(git `info/refs`, API 토큰·한도 없음)으로 받고, 라이선스 파일이 그대로인지 확인한 뒤 앱 데이터 `catalog-cache/`에 저장. subagent의 `model`은 빼서 세션 모델을 쓴다.
  - 쓴 파일에는 출처를 남긴다: SKILL.md는 `license`·`metadata`, 그 밖의 frontmatter는 `# Source:` 주석, CLAUDE.md 구역은 HTML 주석.
  - 내 라이브러리(Phase 3) 항목도 블록으로 쓴다.
- 나중에: `claude -p`로 "내 프로젝트에 맞게 다듬기"(미리보기 후 저장), 기술 스택 감지로 블록 추천.

## 4. 로드맵

- [x] **Phase 0. 초기 세팅**: Tauri + React 스캐폴드, 린트, CI(Windows 설치 파일 빌드), 문서
- [~] **Phase 1. 사이드바 셸 + 도킹 PoC** (구현 완료, Windows 실기 테스트 대기: `docs/testing/phase1-windows.md`): 활성 창 따라가기, 가장자리 고정 대체 동작, 트레이, 전역 단축키
- [x] **Phase 2. 설정 지도(읽기 전용)**: 프로젝트 선택, Claude Code 설정 스캔·표시
- [~] **Phase 3. 라이브러리 적용/제외** (구현·리뷰 완료, Windows 테스트 후 완료 처리)
- [~] **Phase 4. 메인 창 + 디자인 시스템** (구현·리뷰 완료, Windows 테스트 대기: `docs/testing/phase4-windows.md`): 창 2개 전환, 테마(라이트/다크/시스템), 폰트, 메인 레이아웃. 기존 화면(설정 지도·라이브러리)을 메인 창에도 배치
- [~] **Phase 5. 문서 작성기** (구현·리뷰 완료, Linux UI 확인. Windows 테스트 대기: `docs/testing/phase4-windows.md` E): 블록 그래프 편집, 미리보기, 저장·불러오기, 실행 취소, 프로젝트에 쓰기
- [~] **Phase 6. 블록 카탈로그** (외부 13개 포함·출처 표시·최신 받아오기 구현. 목록 자체 갱신(새 인기 항목 추가)은 저장소 index가 main에 올라간 뒤): 내장 블록, 외부 인기 항목(라이선스 확인), 최신 받아오기, 내 라이브러리 연동. 이후 `claude -p` 다듬기·스택 감지
- [ ] **Phase 7. Git 작업 관리**: 브랜치·worktree·상태·조작
- [x] **Phase 8a. 브랜치 메모/TODO (로컬)**: 저장소·브랜치별 메모와 할 일, 브랜치 따라가기, 두 창 동기화
- [ ] **Phase 8b. 메모 동기화** (유료 고객 Firebase)
- [ ] **Phase 9. GitHub 연동**: 로그인, PR·이슈
- [ ] **Phase 10. 배포**: 자동 업데이트, 코드 서명, winget

## 5. 열린 질문 / 리스크

| 항목 | 상태 |
|---|---|
| 유료 플랜: 결제 수단(Paddle·Lemon Squeezy·Stripe 등), 가격, 사업자 등록·세금, 유료 기능 범위 | Phase 8 전에 결정 |
| 유료 사용자가 늘면 Firebase Blaze(종량제) 전환 필요. Firestore·Auth에는 지출 상한이 없음 | Phase 8 전에 결정 |
| GitHub Device Flow 토큰 → Firebase 로그인 동작 여부 | PoC 필요 |
| 도킹 PoC: owner 지정, 포커스, DPI, 관리자 권한 터미널 | 실제 Windows에서 테스트 필요 |
| 정식 앱 이름 | 미정 |
| UI 다국어(영어) | 오픈소스 공개 전에 추가 |
| Claude Code "mods"(터미널 안 패널) 연동 | 나중에 확장으로 검토 |
| 외부 블록을 사용자 파일에 써넣을 때의 저작자 표시(NOTICE) 의무 | 공개 전 법률 검토 |
| React Flow의 WebKitGTK(Linux)·WebView2 실제 조작(드래그, 연결 거부) | 실기 테스트 필요 |
| 작업 중 사용량 한도에 걸리면 | 풀리는 시각에 바로 이어서 하도록 예약(send_later) |
| 코드 서명 | SignPath는 이미 공개된 릴리스가 있어야 신청 가능 |
