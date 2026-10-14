# 개발 계획

> 앱 이름: **Encela (엔셀라)** (2026-10-14 결정). 코드·패키지 이름은 아직 `ai-management`
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
| 앱 이름 | **Encela (엔셀라)** (2026-10-14). 코드·패키지 이름은 아직 `ai-management` | 이름에 "Claude"·"Anthropic" 사용 금지(Anthropic 정책) |
| 창 구성 | **메인 창 + 사이드바, 한 번에 하나만 표시** | 실행하면 메인 창. "사이드바로 전환"하면 메인 창을 닫고 사이드바가 터미널 옆에 붙음. 트레이·단축키로 다시 메인 창 |
| 디자인 | Claude 데스크탑 앱 같은 스타일(따뜻한 무채색, 왼쪽 사이드바, 강조색 하나). 테마는 시스템 따라감 + 설정에서 라이트/다크 | 로고·이름·전용 폰트는 쓰지 않고 무료 폰트(OFL)로 대체 |
| 도킹 방식 | **지금 쓰는 창 따라가기** | Windows Terminal / VS Code / Claude 데스크탑 / Codex(ChatGPT) 데스크탑 중 활성 창 옆에 자동으로 붙음 |
| 대상 AI 도구 | **Claude Code + Codex** (Phase 13, 2026-10-13). 이후 Copilot·Antigravity 추가 예정 | 도구 어댑터 구조로 도구를 추가할 수 있게 함 (3.9) |
| 문서 생성 | 템플릿 + 로컬 `claude` CLI headless 호출 | 사용자 구독 그대로 사용, 비용 0 |
| 메모 저장 | 로컬 SQLite가 원본 + 교체 가능한 동기화 어댑터 | 오프라인에서도 동작 |
| 동기화 | **무료: 로컬만 / 유료 고객: Firebase 동기화** | 중앙 프로젝트의 공유 한도·악용 문제를 유료 고객으로 한정해서 해결 (research.md 4장) |
| 동기화 로그인 | GitHub 로그인 → Firebase (`GithubAuthProvider.credential`) | GitHub 연동 로그인과 통합. Device Flow 토큰을 Firebase가 받는지는 PoC 필요 |
| git | 설치된 `git` CLI 실행 | worktree·push·인증(GCM)까지 레퍼런스 동작 |
| GitHub | GitHub App + Device Flow 로그인 | 클라이언트 시크릿 불필요(오픈소스 OK), 레포 단위 권한 |
| 배포 | GitHub Releases + Tauri updater, NSIS 설치 파일 | 무료 |
| 외부 블록을 사용자 파일에 써넣을 때의 저작자 표시(NOTICE) 의무 | 공개 전 법률 검토 |
| 설치 파일에 서드파티 라이선스 전문 동봉(블록 LICENSE, npm·Rust 의존성, 글꼴)과 앱 안 "오픈소스 라이선스" 화면. 지금은 저장소에만 있음 | 공개 배포 전 필수 |
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
- 자석 버튼(3단): 누를 때마다 **자동 감지 → 창에 고정 → 해제** 순으로 바뀐다. 메인 창에서 사이드바로 전환하면 항상 자동 감지부터 시작한다.
  - 자동 감지: 앞으로 나온 대상 창을 따라간다(터미널과 Claude 데스크탑을 오가면 계속 옮겨 붙음).
  - 창에 고정: 지금 붙은 창에만 붙어 있고 다른 대상 창은 무시한다. 붙은 창이 없거나 닫히면 다음에 앞으로 나오는 대상 창에 고정된다.
  - 해제: 사용자가 둔 자리에 그대로 있는다. 트레이의 도킹 체크는 해제 ↔ 자동 감지만 바꾼다.

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

**Phase 7 결정 (2026-10-11)**
- 앱이 하는 조작: 상태·변경 파일·diff 보기, 브랜치 전환·만들기·삭제, worktree 만들기·삭제·정리. 커밋·push·pull은 Claude Code나 터미널에 맡긴다(나중에 확장 가능).
- 변경 파일은 목록 + 읽기 전용 diff(HEAD 기준, 스테이징 여부 무관).
- 앱에서 만드는 worktree도 Claude Code 규칙을 따른다(`.claude/worktrees/<이름>`, 브랜치 `worktree-<이름>`). `.claude/worktrees/`는 `.git/info/exclude`에 넣는다.
- 되돌리기 어려운 조작은 확인창. 병합 안 된 브랜치 삭제·변경이 남은 worktree 삭제는 기본으로 막고, 경고를 두 번 확인해야 강제로 지운다. 기본·열린 worktree는 지울 수 없다.

### 3.6 GitHub
- ~~GitHub App(선택한 레포만 접근) + Device Flow~~ → **Phase 9 결정 (2026-10-11): OAuth App + Device Flow**, scope `repo`. 레포마다 App을 설치할 필요가 없는 대신 모든 비공개 레포에 접근할 수 있는 권한이다. OAuth App 토큰은 만료되지 않는다(사용자가 GitHub 설정에서 취소 가능).
- Client ID는 공개 값이라 코드에 넣는다(`src-tauri/src/github/mod.rs`의 `CLIENT_ID`, 개발 중에는 환경 변수 `AIM_GITHUB_CLIENT_ID`). Client secret은 쓰지 않는다.
- 9a 범위: 작업 탭 안 GitHub 섹션. 현재 브랜치 PR·CI 상태, 열린 PR·이슈 목록, 현재 브랜치로 PR 만들기. 리뷰·머지·댓글은 GitHub 웹으로 연결한다. GitHub 요청은 60초마다(화면이 보일 때)·창 포커스·브랜치 변경 때만.
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

**문서·라이브러리 동기화 결정 (2026-10-12)**
- 문서 작성기 문서(`compositions/*.json`)와 라이브러리(skill 폴더의 텍스트 파일, subagent .md)도 메모와 같은 계정·허용 목록으로 동기화한다. Firestore `users/{uid}/files/{kind-해시}` 한 문서에 한 항목씩 넣는다(최대 900KB).
- 로컬 파일이 기준이다. `sync-files.json`에 항목마다 마지막으로 서버와 맞춘 내용의 해시(base)를 둔다. 한쪽만 바뀌면 그쪽을 따르고, 양쪽 다 바뀌면 나중에 고친 쪽이 이긴다. 진 쪽은 "이름 (다른 기기)" 또는 "(이 기기)" 사본으로 남기고 경고를 띄운다. 한쪽에서 지우고 다른 쪽에서 고쳤으면 고친 쪽을 남긴다.
- 동기화하지 않는 것: `composed.json`, `applied.json`, `config.json`(프로젝트 경로·라이브러리 폴더 설정), skill 폴더의 바이너리·숨김·512KB 넘는 파일.
- 계정이나 라이브러리 폴더가 바뀌면 base를 비워서, 없는 항목을 "지운 것"으로 올리지 않는다.

**Phase 8b 결정 (2026-10-11)**
- 요금제(8c)가 정해질 때까지는 **허용 목록**에 있는 계정만 동기화한다. Firebase 콘솔에서 `allowed/{uid}` 문서를 직접 만든다. 보안 규칙이 강제한다(`firebase/firestore.rules`).
- 로그인은 작업 탭의 GitHub 로그인을 재사용한다. Rust가 Identity Toolkit REST(`accounts:signInWithIdp`, `providerId=github.com`)로 GitHub 토큰을 Firebase 세션으로 바꾼다. JS SDK는 데스크탑 웹뷰를 지원하지 않아서 쓰지 않는다.
- 동기화는 자동이다. 편집 후 3초 모아서 올리고, 1분마다와 창 포커스 때 받아온다.
- 경로는 `users/{uid}/items/{itemId}` 한 컬렉션이다. 받아오기는 서버 시각 `syncedAt`(+문서 이름) 커서로 해서 기기 시계 차이와 무관하다.
- 로컬 행의 `synced_at`이 `updated_at`과 다르면 올릴 대상이다. 충돌은 `updatedAt`이 큰 쪽(같으면 deviceId 큰 쪽)이 이기고, 앱과 보안 규칙이 같은 기준을 쓴다.
- 계정이 바뀌면 전부 다시 올린다. 저장소 키가 바뀌어 메모 id가 바뀌면 옛 id에 tombstone을 남긴다.
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

**Phase 12 결정 (2026-10-13): PLANS.md**
- Claude Code는 PLANS.md를 스스로 읽지 않는다. `@PLANS.md` import(매 세션 전체 로드) 대신 CLAUDE.md 관리 구역에 안내 한 줄을 넣어 작업 시작 때 읽게 한다. CLAUDE.md 출력이 없으면 경고.
- 단계 지침은 Claude가 그 단계에서 읽고 따르는 글이다. 강제로 실행되지 않는다(강제는 hooks뿐이고 hooks는 계획의 단계를 모른다).
- 겹침: CLAUDE.md(항상 적용)와 단계 지침(그 단계만)이 다르게 말하면 Claude가 헷갈린다. 예: CLAUDE.md "subagent 전에 물어봐" vs 단계 "여기서 subagent 실행". 단계에서 고른 subagent·skill이 이 문서의 SUBAGENTS·SKILLS 출력에 없으면 경고한다. 문장 의미 충돌은 자동 검사하지 않는다.
- Claude Code 자체 plan 모드의 계획, 앱의 브랜치 할 일과는 별개다(자동으로 맞추지 않음).
- 상태는 앱에서 정하고, Claude가 PLANS.md에서 바꾼 상태는 다음 '프로젝트에 쓰기' 때 관리 구역 수정으로 확인을 받는다. 파일에서 상태를 읽어 오는 기능은 아직 없다.
- 세부 목표는 항상 보조 목표 아래: 맨 위로 옮겨진 세부 목표는 보조 목표가 된다. 보조 목표를 옮기면 그 아래 세부 목표·단계 지침이 같이 옮겨진다.

### 3.9 여러 AI 도구 지원 (Phase 13, 2026-10-13 결정)
- 구조: Rust `tools/` 모듈이 도구별 어댑터를 가진다(`tools/claude`, `tools/codex`). 공통 타입(설정 지도 항목·범위·상태)과 도구별 파일 배치(`ToolId` 메서드: skill 폴더, agent 파일, 지침 파일, 규칙·worktree 폴더)를 한 곳에 둔다. 프론트는 `src/tools.ts`가 같은 내용을 미러링한다. 도구를 추가할 때 둘을 같이 고친다.
- **프로젝트마다 도구 하나**를 쓴다. 설정 지도·라이브러리·작성기·프로젝트 문서·MCP·worktree·커밋 초안이 그 도구 기준으로 동작한다. 설정 지도의 도구 선택에서 "자동" 또는 도구를 직접 고르고, 고른 값은 프로젝트별로 기억한다(localStorage `toolPrefs`, 두 창 공유).
- 자동 감지: ① 프로젝트(와 저장소 루트)의 파일 — 도구 전용 폴더·지침 파일은 2점(`.claude/`, `CLAUDE.md`, `CLAUDE.local.md`, `.codex/`, `AGENTS.override.md`), 여러 도구가 같이 쓰는 파일은 1점(`AGENTS.md`, `.agents/skills`, `.mcp.json`) ② 동점이면 설치된 CLI가 하나뿐일 때 그 도구 ③ 그래도 못 정하면 설정의 **기본 도구**(처음 값 Claude Code).
- Codex 파일 배치: 지침 `AGENTS.md`(**팀만**. Codex에는 개인용 지침 파일이 없고, AGENTS.md를 git에서 빼면 팀 파일과 부딪혀서 작성기의 '나만' 지침 출력은 경고만 내고 쓰지 않는다. hooks도 같다), skill `.agents/skills/<이름>/`(frontmatter에 `name`·`description`이 없으면 채운다), subagent `.codex/agents/<이름>.toml`(라이브러리의 Markdown을 TOML로 변환), 규칙 파일 없음(규칙 블록은 AGENTS.md로), hooks `.codex/hooks.json`(Stop은 stdout에 JSON 필수, PreToolUse는 deny만 지원한다는 경고), PLANS 안내는 계획의 범위와 상관없이 팀 AGENTS.md에.
- GitHub에 바로 쓰기는 열린 로컬 프로젝트가 아니라 **대상 저장소의 도구**를 따른다. 브랜치의 파일 목록에서 한 도구의 파일만 있으면 그 도구(`CLAUDE.md`·`.claude/` 대 `AGENTS.md`·`AGENTS.override.md`·`.codex/`), 아니면 기본 도구로 정하고, 쓰기 창에서 바꿀 수 있다.
- Codex 끄기: skill만, **나만**만 지원한다. Codex는 `[[skills.config]]`를 사용자 설정(`~/.codex/config.toml`)에서만 인정하므로, 프로젝트 복사본의 SKILL.md 절대 경로로 끈다. 팀 끄기·subagent 끄기는 Codex에 방법이 없어 버튼을 숨긴다.
- worktree: Codex 프로젝트는 `<레포>/.codex/worktrees/<이름>`에 브랜치 `worktree-<이름>`으로 만든다(git에서 제외). Codex 앱이 만든 worktree(`~/.codex/worktrees/…`)는 표시만 한다.
- MCP: Codex는 `config.toml`의 `[mcp_servers]`를 앱이 직접 읽고 고친다(read-merge-write, 사용자·프로젝트 범위). 연결 확인은 Codex에 없어서 표시하지 않는다.
- 커밋 메시지 초안: 프로젝트 도구의 CLI(`claude -p` 또는 `codex exec` 읽기 전용·세션 저장 안 함), 없으면 설치된 다른 CLI.
- Claude 전용으로 남기는 것: Claude Code mod(설정에서 "Claude Code 전용" 표시), 명령(commands) 화면(Codex 프로젝트에서는 skill로 대체됐다는 안내). Codex hook은 사용자가 Codex `/hooks`에서 신뢰해야 실행된다(앱이 대신 신뢰하지 않음).
- 도킹 대상에 Codex 데스크탑 앱(Windows는 ChatGPT 데스크탑 앱)을 추가한다. 프로세스 이름은 실기 확인 필요.

### 3.10 작업 트리 (Phase 14, 2026-10-14 결정)
- 작업 탭과 별도 기능이다. 사이드바에 새 탭 **트리**, 메인 창 왼쪽 메뉴에 **작업 트리** 페이지. 둘 다 **GitHub** / **프롬프트** 두 트리를 전환해 본다.
- 모양: 시간순 흐름 트리(git 그래프처럼 세로 줄기에 점, 브랜치는 가지로 갈라졌다 병합 때 합쳐짐). 최신이 위.
  - GitHub 트리: 커밋(GitHub에 없으면 빈 점) → push → PR 열림 → CI → 병합/닫힘. 브랜치마다 가지(lane), 기본 브랜치가 줄기.
  - 프롬프트 트리: 세션 하나가 가지 하나(도구 아이콘 Claude Code/Codex), 점은 프롬프트. 점에 그 프롬프트 동안 고친 파일 수, 만든 커밋(눌러서 GitHub 트리의 같은 커밋으로 이동), push 표시.
- 로그는 둘로 나눠 앱 데이터 `activity.db`에 남긴다. 저장소 키는 메모와 같다(같은 저장소의 clone·worktree는 로그를 공유). 원본(세션 파일·브랜치)이 지워져도 남는다. 보관 기간은 설정(기본 90일, 무기한 가능).
- GitHub 로그 수집: 로컬 `git`(모든 브랜치 커밋·부모, 원격 추적 브랜치로 push 여부, 병합 커밋)과 GitHub API(PR 상태·병합·닫힘, CI 결과; 기존 GitHub 로그인 재사용). 화면이 보일 때·창 포커스 때만 갱신.
- 프롬프트 로그 수집: 각 도구가 이미 남기는 **세션 기록 파일을 읽기만** 한다(Claude Code `~/.claude/projects/*/*.jsonl`, Codex `$CODEX_HOME/sessions/**/rollout-*.jsonl`). 세션의 작업 폴더(cwd)가 이 저장소인 것만. 사람이 보낸 프롬프트만(자동 알림·도구 결과·주입된 지침 제외). 로그인 정보 파일은 읽지 않는다. 형식이 공개 규격이 아니라서 모르는 줄은 건너뛰고, 형식이 바뀌면 그 세션만 빠진다.
- **동기화**: 메모처럼 계정·허용 목록으로 Firestore `users/{uid}/activity/{id}`에 올린다(보안 규칙 다시 게시 필요). 프롬프트는 **앞 500자만, 토큰·키처럼 보이는 문자열은 가려서** 저장·동기화한다(원문 전체는 원본 세션 파일에만 있음). 설정에서 프롬프트 로그 동기화만 끌 수 있다. 가림 대상: 키·토큰 모양 문자열, `password`·`secret`·`token`·`api_key` 등이 들어간 이름의 `=`/`:` 값(JSON·YAML 따옴표 키 포함), URL의 `user:pass@`, `Authorization:` 헤더. 보관 기간이 지나 지워진 항목은 내용(글·제목·파일·커밋·주소·작성자·부모·브랜치)을 비운 묘비만 남고 서버로도 내용 없이 올라간다. 더 오래된 항목 불러오기는 (시각, id) 커서(`before` + `beforeId`)로 같은 시각의 항목을 놓치지 않는다.

### 3.11 설정 저장 · 업데이트 표시 (2026-10-14 결정)
- 업데이트 후 글자 크기 등 설정이 초기화되는 문제: 설정을 웹뷰 localStorage 대신 앱 데이터 `prefs.db`에 저장한다. 처음 실행 때 기존 localStorage 값을 한 번 옮긴다(이후 localStorage는 대비용 복사본).
- 테마·언어·글자 크기·알림·기본 도구(`theme`, `language`, `fontSize`, `notifyEnabled`, `defaultTool`)만 계정으로 동기화한다(`users/{uid}/prefs/{key}`, 보안 규칙 다시 게시 필요). 최근 프로젝트·패널 너비 등 기기마다 다른 값은 동기화하지 않는다.
- 업데이트 알림의 "나중에"는 카드만 닫고, 메인 창 왼쪽 메뉴 아래(사이드바는 상태줄)에 작은 "업데이트" 표시를 남겨 언제든 눌러 설치한다. 확인 주기는 그대로(실행 5초 뒤, 이후 6시간마다).
- 설정 › 정보의 라이선스 표시는 "Apache-2.0 오픈소스"만.

## 4. 로드맵

- [x] **Phase 0. 초기 세팅**: Tauri + React 스캐폴드, 린트, CI(Windows 설치 파일 빌드), 문서
- [x] **Phase 1. 사이드바 셸 + 도킹 PoC** (구현 완료, Windows 실기 테스트 대기: `docs/testing/phase1-windows.md`): 활성 창 따라가기, 가장자리 고정 대체 동작, 트레이, 전역 단축키
- [x] **Phase 2. 설정 지도(읽기 전용)**: 프로젝트 선택, Claude Code 설정 스캔·표시
- [x] **Phase 3. 라이브러리 적용/제외** (구현·리뷰 완료, Windows 테스트 후 완료 처리)
- [x] **Phase 4. 메인 창 + 디자인 시스템** (구현·리뷰 완료, Windows 테스트 대기: `docs/testing/phase4-windows.md`): 창 2개 전환, 테마(라이트/다크/시스템), 폰트, 메인 레이아웃. 기존 화면(설정 지도·라이브러리)을 메인 창에도 배치
- [x] **Phase 5. 문서 작성기** (구현·리뷰 완료, Linux UI 확인. Windows 테스트 대기: `docs/testing/phase4-windows.md` E): 블록 그래프 편집, 미리보기, 저장·불러오기, 실행 취소, 프로젝트에 쓰기
- [x] **Phase 6. 블록 카탈로그** (외부 13개 포함·출처 표시·최신 받아오기 구현. 목록 자체 갱신(새 인기 항목 추가)은 저장소 index가 main에 올라간 뒤): 내장 블록, 외부 인기 항목(라이선스 확인), 최신 받아오기, 내 라이브러리 연동. 이후 `claude -p` 다듬기·스택 감지
- [x] **Phase 7. Git 작업 관리** (구현 완료, Windows 테스트 대기: `docs/testing/phase4-windows.md` G): 브랜치·worktree·상태·diff·조작
- [x] **Phase 8a. 브랜치 메모/TODO (로컬)**: 저장소·브랜치별 메모와 할 일, 브랜치 따라가기, 두 창 동기화
- [x] **Phase 8b. 메모 동기화 (데이터베이스 연동)** (구현 완료, Firebase 프로젝트 설정 후 Windows 테스트: `docs/sync-setup.md`, `docs/testing/phase4-windows.md` I): Firestore 연동, GitHub 계정으로 로그인, 허용 목록 계정만 사용
- [ ] **Phase 8c. 요금제** (미결정 · 보류): 결제 수단·가격·유료 범위. 정해지면 결제 웹훅이 허용 목록(`allowed/{uid}`)을 채우게 바꾼다
- [x] **Phase 9a. GitHub 연동** (구현 완료, OAuth App 등록·Client ID 입력 후 Windows 테스트: `docs/testing/phase4-windows.md` H): 로그인, PR·CI·이슈 보기, PR 만들기
- [x] **Phase 9b. GitHub 저장소 선택** (구현 완료, Windows 테스트 대기: `docs/testing/phase4-windows.md` J): 로그인 안 했으면 프로젝트와 상관없이 로그인 버튼, 내 저장소 목록에서 고르기(clone 있으면 로컬로 열기, 없으면 GitHub 보기: 브랜치·커밋·CI·PR·이슈, 브랜치 만들기/삭제, PR 만들기, clone해서 작업하기)
- [x] **Phase 9c. 프로젝트 문서 편집** (구현 완료, Windows 테스트 대기: `docs/testing/phase4-windows.md` K): 연결한 폴더의 CLAUDE.md(하위 폴더 포함)·rules·skills·agents·commands·설정 파일 자동 감지, 앱 안 편집기로 읽고 고쳐서 바로 저장, 밖에서 바뀌면 알림, 새 문서·삭제
- [x] **Phase 9d. GitHub 직접 수정 · 커밋/push** (구현 완료, Windows 테스트 대기: `docs/testing/phase4-windows.md` L): clone 없이 GitHub 파일을 열어 고치고 여러 파일을 한 커밋으로(브랜치 또는 새 브랜치), 로컬 작업 탭에서 커밋·push·pull
- [x] **Phase 11. Claude Code mod 연동** (구현 완료, Windows + Claude Code 2.1.287+ 테스트 대기: `docs/testing/phase4-windows.md` M): 브랜치 메모 mod(입력창 위 한 줄, /notes 패널, /todo · /memo, Claude에게 메모 전달), 설정에서 설치/제거, 추천 mods 설치
- [x] **Phase 9e. 작업 화면 개편** (2026-10-12): 흐름 카드(커밋→올리기→PR→CI, 다음 할 일 버튼), 변경·기록·브랜치·Worktree·GitHub 탭, 커밋 타임라인, PR 색(열림 초록·병합 보라), 고른 GitHub 저장소·브랜치 기억
- [x] **Phase 9f. 작업 화면 보강** (2026-10-13): ① 파일별 커밋에 담기/빼기(체크박스) ② 기록에서 커밋 눌러 변경 보기 ③ `claude -p`로 커밋 메시지 초안(haiku, 도구 끔, 세션 저장 안 함) ④ clone 없는 저장소 화면에도 흐름 카드·탭·타임라인 ⑤ 브랜치 PR 병합·닫힘, CI 끝남 데스크톱 알림(설정에서 끄기) ⑥ 충돌 해결 도우미(블록마다 현재/들어오는/둘 다, 계속하기·중단하기)
- [x] **Phase 9g. 앱 계정** (2026-10-13, Firebase 콘솔 설정 후 Windows 테스트: `docs/sync-setup.md` 2-1): 이메일·비밀번호(가입·로그인·재설정·인증 메일), Google(브라우저 + localhost 리디렉션, 비밀 값 없음), GitHub(작업 탭 Device Flow 그대로, 로그인하면 브랜치·PR도 바로 불러옴). 모두 한 Firebase 계정에 연결해 어느 방법으로든 같은 데이터. 갱신 토큰은 OS 자격 증명 저장소. 예전 GitHub 동기화 기기는 같은 계정으로 자동 전환, 허용 목록 유지
- [x] **Phase 12. PLANS.md** (2026-10-13): 문서 작성기에 계획 블록. 주요 목표(큰 블록) 안에 보조 목표·세부 목표를 끌어 놓고, 그 사이에 단계 지침(지침 글 / subagent에게 맡기기 / skill 쓰기 / 확인 질문)을 넣는다. 상태 `[y]` 완료 · `[p]` 진행 중 · `[n]` 예정. PLANS 출력이 `PLANS.md`(관리 구역)로 쓰고 CLAUDE.md에 "계획은 PLANS.md" 안내 한 줄. 출력·블록 이름은 대문자(RULES, SKILLS, SUBAGENTS, HOOKS, PLANS)
- [x] **Phase 13. 여러 AI 도구 지원 (Codex)** (구현·리뷰 완료, Windows + Codex 테스트 대기: `docs/testing/phase4-windows.md` Q): 도구 어댑터, 자동 감지·선택, Codex 설정 지도·라이브러리·작성기·프로젝트 문서·MCP·worktree·도킹·커밋 초안
- [x] **Phase 14. 작업 트리** (구현·리뷰 완료, Windows 테스트 대기: `docs/testing/phase4-windows.md` R, 보안 규칙 다시 게시 필요): GitHub 트리·프롬프트 트리, 로그 저장·동기화, 사이드바 탭·메인 페이지
- [ ] **Phase 10. 배포**: 자동 업데이트, 코드 서명, winget

## 5. 열린 질문 / 리스크

| 항목 | 상태 |
|---|---|
| 유료 플랜: 결제 수단(Paddle·Lemon Squeezy·Stripe 등), 가격, 사업자 등록·세금, 유료 기능 범위 | Phase 8c (보류) |
| 유료 사용자가 늘면 Firebase Blaze(종량제) 전환 필요. Firestore·Auth에는 지출 상한이 없음 | Phase 8c (보류) |
| GitHub Device Flow 토큰 → Firebase 로그인 동작 여부 | PoC 필요 |
| 도킹 PoC: owner 지정, 포커스, DPI, 관리자 권한 터미널 | 실제 Windows에서 테스트 필요 |
| 정식 앱 이름 | Encela로 결정 (2026-10-14). 앱 안 표시 이름·설치 파일·패키지 이름 변경은 아직 |
| UI 다국어(영어) | 오픈소스 공개 전에 추가 |
| Claude Code "mods"(터미널 안 패널) 연동 | Phase 11에서 구현 |
| 외부 블록을 사용자 파일에 써넣을 때의 저작자 표시(NOTICE) 의무 | 공개 전 법률 검토 |
| React Flow의 WebKitGTK(Linux)·WebView2 실제 조작(드래그, 연결 거부) | 실기 테스트 필요 |
| 작업 중 사용량 한도에 걸리면 | 풀리는 시각에 바로 이어서 하도록 예약(send_later) |
| 코드 서명 | SignPath는 이미 공개된 릴리스가 있어야 신청 가능 |

**Phase 9d · 11 결정 (2026-10-12)**
- 커밋·push·pull을 작업 탭에서 한다(이전 결정 "커밋·push는 Claude Code나 터미널"을 바꿈). GitHub 로그인이 있으면 그 명령 한 번에만 토큰을 git 환경 변수로 넘기고(설정 파일에 저장 안 함), 없으면 사용자 git 자격 증명을 쓴다. pull은 fast-forward만.
- clone 없는 GitHub 수정은 Git Data API로 한다: 파일을 불러온 커밋 위에 트리·커밋을 만들고 브랜치를 force 없이 옮긴다. 그사이 브랜치가 움직였으면 거절(새 브랜치로는 커밋 가능).
- mod와 앱은 파일로만 주고받는다: 앱이 `<앱 데이터>/bridge/state.json`에 메모·할 일·PR 요약을 쓰고(30초마다 heartbeat), mod는 `bridge/inbox/*.json`에 요청을 남긴다. DB에 쓰는 건 앱 하나라 동기화(8b)와 꼬이지 않는다. 앱이 꺼져 있으면 다음 실행 때 반영.
- mod 설치는 사용자 `claude` 명령으로: 앱이 mod 파일을 `<앱 데이터>/claude-mod`(로컬 마켓플레이스 `ai-management`)에 쓰고 `claude plugin marketplace add` + `install branch-notes@ai-management --scope user`. Claude Code 설정·로그인 정보는 앱이 직접 읽거나 쓰지 않는다.
- 추천 mods는 저장소와 LICENSE를 확인한 것만 넣는다(`docs/research.md` 7).

**나중에 추가할 후보 (2026-10-12, 상태는 2026-10-12 기준)**
- 비슷한 앱 조사(`docs/research.md` 10)에서 나온 후보. 효과가 크고 기존 기능과 잘 맞는 순서:
  1. [ ] **권한 편집기**: settings.json의 `permissions` allow/ask/deny를 범위별로 보고 고치기(read-merge-write). 설정 지도와 연결. (지금은 설정 지도가 deny로 꺼진 항목을 표시만 함)
  2. [ ] **사용량·비용 보기**: `~/.claude`의 세션 기록(JSONL)을 읽어 모델·프로젝트별 토큰, 5시간 창. 로그인 정보 파일은 읽지 않는다.
  3. [x] **MCP 관리**: 2026-10-12 완료(MCP 서버 페이지, `docs/research.md` 11).
  4. [ ] **프로필**: skills·agents·hooks·MCP 묶음을 이름 붙여 저장하고 프로젝트에 한 번에 적용.
  5. [~] **설정 점검**: 깨진 설정 JSON·가려진 항목·꺼진 항목 표시는 설정 지도에 있음(완료). 없는 hook 경로 검사는 아직.
  6. 세션 기록 탐색 [ ], 슬래시 명령 편집기 [~](프로젝트 문서 화면에서 commands 파일 편집은 완료, 전용 편집기는 아직), 상태줄 빌더 [ ], 가져오기·내보내기 [ ].
- [ ] 문서 작성기 블록끼리 연결(체인 4종, 선으로 연결 + 블록에 표시. 결정 완료, 구현 전).
- [ ] 문서 작성기 `claude -p` "내 프로젝트에 맞게 다듬기", 기술 스택 감지로 블록 추천.
- [ ] anthropics/skills `skill-creator` (Apache-2.0): skill을 만들고 평가·개선하는 skill. Python 스크립트(평가 루프, 설명 개선, 패키징)가 있어야 제대로 동작해서 Markdown만 가져오는 지금 카탈로그 방식으로는 넣지 않았다. 스크립트까지 설치하는 방식(예: mods처럼 마켓플레이스로 설치)을 지원할 때 넣는다. 출처: https://github.com/anthropics/skills/tree/main/skills/skill-creator
- [ ] 초보자 모드: Claude Code 내장 출력 스타일 Explanatory·Learning을 설정 화면에서 켜는 기능(`.claude/settings.local.json`의 `outputStyle`, read-merge-write), 초보자용 지침 블록(`docs/research.md` 9).
- [x] 카탈로그 추가(27개 블록·동반 파일·큰 블록 경고)와 추천 MCP 8개: 2026-10-12 완료(`docs/research.md` 12).
- [x] 문서 작성기에서 clone 없이 GitHub 저장소에 바로 쓰기(새 브랜치+PR 또는 브랜치에 직접): 2026-10-12 완료.
