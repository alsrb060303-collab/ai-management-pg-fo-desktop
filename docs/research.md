# 기술 조사 노트

> 조사일: 2026-10-10. 조사는 haiku 에이전트가, 검증은 sonnet 에이전트가 1차 출처(공식 문서, 저장소, 가격 페이지)로 다시 확인했다.
> 버전과 무료 한도는 자주 바뀌므로 구현 직전에 다시 확인한다.

## 1. Claude Code 설정 구조

출처: [memory](https://code.claude.com/docs/en/memory), [skills](https://code.claude.com/docs/en/skills), [sub-agents](https://code.claude.com/docs/en/sub-agents), [settings](https://code.claude.com/docs/en/settings), [permissions](https://code.claude.com/docs/en/permissions), [plugins](https://code.claude.com/docs/en/plugins/loading), [headless](https://code.claude.com/docs/en/headless), [legal](https://code.claude.com/docs/en/legal-and-compliance)

- 조사 시점 버전: npm `latest` 2.1.296, `stable` 2.1.287. 기능이 버전마다 다르다.
- **CLAUDE.md**: managed → user(`~/.claude/CLAUDE.md`) → project(`./CLAUDE.md` 또는 `./.claude/CLAUDE.md`) → local(`./CLAUDE.local.md`). 상위 폴더 것까지 **이어 붙여서** 읽는다(덮어쓰지 않음). 하위 폴더 것은 그 폴더 파일을 다룰 때 로드된다. `@경로`로 가져오기 가능(최대 4단계).
- **`.claude/rules/*.md`**: frontmatter의 `paths`만 읽는다. `paths`가 없으면 항상 로드, 있으면 해당 파일을 다룰 때 로드.
- **AGENTS.md**: CLAUDE.md / CLAUDE.local.md / `.claude/CLAUDE.md`가 하나도 없을 때만 읽는다(v2.1.277+).
- **Skill**: `<scope>/.claude/skills/<name>/SKILL.md`. 이름이 겹치면 enterprise > personal > project. 상위 폴더부터 레포 루트까지 탐색한다. frontmatter: `name`, `description`, `when_to_use`, `disable-model-invocation`, `user-invocable`, `allowed-tools`, `model`, `effort`, `context`, `paths` 등.
- **Subagent**: `.claude/agents/*.md`(하위 폴더 포함), 정체성은 `name` 필드로만 판단. 우선순위는 managed > `--agents` > project > user > plugin.
- **settings 우선순위**: managed > CLI `--settings` > `.claude/settings.local.json` > `.claude/settings.json` > `~/.claude/settings.json`. 값은 덮어쓰고, 배열(permissions 등)은 합친다.
- **프로젝트별 끄기**
  - skill: `"skillOverrides": {"<name>": "off"}` (`on` / `name-only` / `user-invocable-only` / `off`). `/skills` 메뉴도 `settings.local.json`에 이 값을 쓴다. plugin skill에는 적용 안 됨.
  - subagent: `"permissions": {"deny": ["Agent(<name>)"]}` (예전 이름 `Task(...)`도 동작)
  - plugin: `"enabledPlugins": {"<name>@<marketplace>": false}` 또는 `claude plugin disable <id> --scope local`
  - 파일 단위 설정의 병합 규칙은 문서에 명시되지 않은 부분이 있어서 실제로 확인한 뒤 써야 한다.
- **`settings.local.json` 주의**: Claude Code는 **자기가 처음 만들 때만** git 전역 제외 목록에 추가한다. 다른 프로그램이 만들면 git에 잡힌다. git에 커밋된 local 파일은 저장소가 제공한 파일로 취급된다.
- **headless 생성**: `claude -p --output-format json --json-schema ... --permission-mode dontAsk --allowedTools ... --max-turns N --max-budget-usd X`
  - `--bare`는 구독 로그인을 쓰지 않고 API 키가 필요하다. 구독 사용자는 `--bare` 없이 실행해야 한다.
  - `--bare` 없는 `-p`는 프로젝트 hooks와 `.mcp.json` 서버를 **신뢰 확인 없이 실행**한다.
  - 로그인 상태 확인: `claude auth status`(기본 출력이 JSON).
- **정책**: 제3자 앱은 Claude.ai 자격 증명을 수집·저장·중계하면 안 되고, 바이너리를 수정하면 안 된다. 제품 이름과 로고에 "Claude Code"·"Anthropic"을 쓰면 안 된다. 사용자가 수정 안 된 바이너리에 자기 구독으로 로그인하는 건 허용된다.
- **Phase 2 검토에서 추가로 확인한 규칙** (설정 지도에 반영됨)
  - `managed-mcp.json`이 있으면 관리 정책 MCP 서버만 쓴다(독점). 서버 목록이 비어 있어도 마찬가지다.
  - 관리 설정의 `strictPluginOnlyCustomization`(`true` 또는 `["skills","agents","hooks","mcp"]` 중 일부)은 해당 항목을 플러그인으로 배포된 것만 허용한다.
  - `claudeMdExcludes`(glob 목록, 모든 계층의 값을 합침)로 CLAUDE.md와 rules 파일을 제외할 수 있다. 관리 정책 CLAUDE.md는 제외되지 않는다.
  - `pluginConfigs["cc-plugin-agents-md@builtin"].options.instructionFiles`는 AGENTS.md 읽기 방식을 정한다. 사용자·관리 설정에서만 적용된다.
  - skill 폴더와 `.claude/rules` 하위 폴더는 symlink여도 된다.
  - claude.ai에서 동기화된 skill은 `~/.claude/skills/synced/<이름>`에 있고 `anthropic-skills:<이름>`으로 호출한다. `.trash`는 읽지 않는다.
  - `Skill(deploy *)`처럼 접두사로 막는 deny 규칙이 있다.
  - subagent `name`에 `:`가 있거나 256자를 넘으면 인식되지 않는다.
- **Windows**: 설정은 `%USERPROFILE%\.claude\`, 실행 파일은 `%USERPROFILE%\.local\bin\claude.exe`(네이티브 설치 기준). Git for Windows는 선택 사항이다(없으면 PowerShell 도구 사용).

## 2. Tauri 2 + Windows 도킹

출처: [Tauri releases](https://github.com/tauri-apps/tauri/releases), [config schema](https://schema.tauri.app/config/2), [WebviewWindowBuilder](https://docs.rs/tauri/latest/x86_64-pc-windows-msvc/tauri/webview/struct.WebviewWindowBuilder.html), [SetWinEventHook](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwineventhook), [event constants](https://learn.microsoft.com/en-us/windows/win32/winauto/event-constants), [DWMWINDOWATTRIBUTE](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute), [Window features](https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features)

- 버전: tauri 2.12.2(최소 Rust 1.90), tao 0.37, wry 0.57, `windows` crate 0.62. Tauri 3은 알파(3.0.0-alpha.4) 단계다.
- WebView2: Windows 11에는 기본 포함, Windows 10은 대부분 포함. 설치 파일은 기본값 `downloadBootstrapper`(용량 증가 없음)를 쓴다.
- 창 옵션: `alwaysOnTop`, `decorations`, `transparent`, `skipTaskbar`, `shadow`, `focus`, `focusable`. 테두리 없는 창에 `shadow:true`면 1px 테두리가 생기고 Windows 11에서는 모서리가 둥글어진다.
- `focusable:false`는 `WS_EX_NOACTIVATE`가 된다. 클릭해도 활성화되지 않아서 **키보드 입력이 안 된다**.
- **owner 지정**: 다른 프로세스의 창(HWND)에는 `owner_raw(HWND)`(Windows 전용)를 쓴다. `parent_raw`는 자식 창이 되므로 쓰면 안 된다. owned 창은 owner 바로 위에 뜨고, owner가 최소화되면 숨고, owner가 닫히면 함께 파괴된다. 생성 후 owner를 바꾸는 것(`GWLP_HWNDPARENT`)은 문서끼리 설명이 엇갈려서 실험이 필요하다.
- DPI: Tauri는 실행 중에 PerMonitorV2를 설정한다. 위치는 `PhysicalPosition`으로 지정해야 한다(Logical 값은 현재 모니터 배율로 환산됨). `GetWindowRect`에는 보이지 않는 테두리가 포함되므로 `DWMWA_EXTENDED_FRAME_BOUNDS`를 쓴다.
- 이벤트 훅: `WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS`로 걸고, 등록한 스레드에서 메시지 루프를 돌려야 한다. 콜백은 재진입할 수 있다.
- Windows Terminal 창 클래스: `CASCADIA_HOSTING_WINDOW_CLASS`(소스 코드 상수라 바뀔 수 있음). Electron 창 클래스는 `Chrome_WidgetWin_` 뒤에 숫자가 붙는다.
- 관리자 권한 창: UIPI 때문에 낮은 권한 앱의 훅이나 메시지가 막힐 수 있다(이벤트 수신 여부는 미확인).
- 참고 구현: komorebi(`SetWinEventHook` 사용, 라이선스가 OSS가 아니라 코드 복사 금지), GlazeWM(GPL-3.0, 참고만). MIT 래퍼 crate: `wineventhook`, `win_event_hook`.
- 플러그인: global-shortcut, positioner, single-instance, sql, store, updater, window-state, autostart 모두 v2에 있다. 트레이는 core의 `tray-icon` feature다.
- 업데이트: GitHub Releases에 `latest.json`을 올리는 방식. 서명 개인키를 잃어버리면 기존 사용자에게 업데이트를 보낼 수 없다.
- 코드 서명: 서명이 없으면 SmartScreen 경고가 뜬다. SignPath Foundation은 OSS에 무료지만 OSI 라이선스, CI 빌드, 이미 공개된 릴리스가 조건이다. Azure Artifact Signing은 유료이고 개인은 미국·캐나다만 가능하다.
- Linux/Wayland: 앱이 자기 창 위치를 정할 수 없다. 도킹은 X11/XWayland에서만 가능하다.

## 3. 동기화 · Git · GitHub

출처: [Supabase pricing](https://supabase.com/pricing), [Turso pricing](https://turso.tech/pricing), [D1 pricing](https://developers.cloudflare.com/d1/platform/pricing/), [Firebase pricing](https://firebase.google.com/pricing), [GitHub device flow](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps), [GitHub App user tokens](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/refreshing-user-access-tokens), [REST rate limits](https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api), [Stronghold deprecation](https://github.com/tauri-apps/plugins-workspace/issues/3494)

| 백엔드 | 무료 한도 | 사용자 설정 부담 | 비고 |
|---|---|---|---|
| GitHub 비공개 레포 | 무제한 private repo | GitHub 로그인만 | 실시간 아님, 쓰기 요청 분당 약 80회 제한 |
| Supabase | DB 500MB, 프로젝트 2개 | 사용자가 프로젝트 생성 | **1주일 미사용 시 일시정지** |
| Turso | DB 100개, 5GB | 계정·DB·토큰 | 2026-10-02 Supabase 인수 발표로 무료 정책 불확실, 10일 미사용 시 보관 처리 |
| Cloudflare D1 | 5GB, 하루 쓰기 10만 행 | 계정·DB·API 토큰 | Worker 없이 REST로 사용 가능, 오프라인 큐 직접 구현 |
| Firebase (Spark) | Firestore 1GiB, 하루 쓰기 2만 / 읽기 5만 | 중앙 프로젝트면 앱에서 가입만 | 검증 진행 중 |
| PocketBase | – | 서버 호스팅 필요 | 0원 조건 불충족 |

- Git: CLI가 worktree·push·인증까지 레퍼런스 동작이다. `gix`는 push·checkout이 없고, `git2`는 worktree API가 있지만 인증 동작이 CLI와 다르다. Git 최신 2.56.0.
- GitHub 로그인: Device Flow는 client secret이 필요 없다. GitHub App은 레포 단위 권한이라 OAuth App의 `repo` 범위(모든 private 레포 접근)보다 안전하다. App이 해당 레포에 설치돼 있어야 접근할 수 있다.
- 토큰 저장: `keyring` 4.x(Windows 자격 증명 관리자). Tauri Stronghold는 v3에서 제거 예정.
- octocrab 0.54: `authenticate_as_device`로 Device Flow를 지원한다.

## 4. Firebase 동기화 검토

출처: [Firestore quotas](https://firebase.google.com/docs/firestore/quotas), [error codes](https://docs.cloud.google.com/firestore/docs/understand-error-codes), [pricing plans](https://firebase.google.com/docs/projects/billing/firebase-pricing-plans), [Auth limits](https://firebase.google.com/docs/auth/limits), [Auth users](https://firebase.google.com/docs/auth/users), [API keys](https://firebase.google.com/docs/projects/api-keys), [App Check](https://firebase.google.com/docs/app-check), [reCAPTCHA billing](https://docs.cloud.google.com/recaptcha/docs/billing-information), [JS SDK environments](https://firebase.google.com/docs/web/environments-js-sdk), [offline persistence](https://firebase.google.com/docs/firestore/manage-data/enable-offline), [Tauri discussion #4805](https://github.com/tauri-apps/tauri/discussions/4805), [Google Cloud Terms](https://cloud.google.com/terms)

**무료 한도와 초과 시 동작 (Spark 요금제)**
- Firestore: 저장 1GiB, 하루 쓰기 2만, 읽기 5만, 삭제 2만 회. 프로젝트 단위라서 **모든 사용자가 같이 쓴다**.
- 초과하면 `RESOURCE_EXHAUSTED` 오류가 나고, 미국 태평양 시간 자정(한국 시간 오후 4~5시)까지 **프로젝트 전체가 동기화 불가**다.
- 결제 계정을 연결하지 않으면 과금은 없다. 결제 계정을 연결하거나 Google Cloud 서비스를 쓰면 자동으로 유료(Blaze)로 바뀐다. Firestore와 Auth에는 지출 상한 기능이 없다.
- Auth: 기본 Auth는 사용자 수 제한이 없다. Identity Platform으로 업그레이드하면 하루 3,000명 제한이 생기므로 업그레이드하지 않는다. 비밀번호 재설정 메일은 하루 150통, 이메일 링크 로그인은 하루 5통, 계정 생성은 IP당 시간당 100개다. SMS 로그인은 유료 플랜 전용이다.
- 2026-02부터 Spark 요금제에서는 Cloud Storage를 쓸 수 없다(첨부 파일 기능을 만들면 걸림).

**중앙 프로젝트에서 공개 가입을 받을 때 문제**
- 웹 설정 키는 공개해도 된다(Google 공식). 다만 할당량은 보호되지 않는다.
- 0원으로 쓸 수 있는 악용 방어 수단이 없다.
  - App Check는 데스크탑(Windows/Tauri)용 공급자가 없다.
  - reCAPTCHA 무료 한도는 월 1만 회인데, 상시 실행 기기 7대 정도면 소진된다.
  - Cloud Functions는 유료 플랜이 필요하다.
- 회원가입 차단 설정은 있다(Authentication 설정에서 사용자 작업 비활성화). 이걸 켜면 개발자 혼자만 쓰는 프로젝트로 만들 수 있다.
- 다른 사람 데이터를 보관하면 개인정보 고지·동의 책임이 프로젝트 소유자에게 있다(Google Cloud Terms 3.2). Auth 데이터는 미국에만 저장되고, 소유자는 콘솔에서 데이터를 볼 수 있다.
- 여러 무료 프로젝트로 사용자를 나눠서 한도를 피하는 건 약관 위반이다(3.3(d)(iii)). 각 사용자가 **자기 프로젝트를 쓰는 것(BYO)**은 문제없다.

**Tauri 웹뷰 안에서 Firebase 쓰기**
- 공식 지원 환경 목록에 Tauri/WebView2는 없다. 브라우저가 아닌 환경에서는 팝업/리다이렉트 로그인이 지원되지 않는다.
- Tauri(wry 0.53+)는 Windows에서 `window.open` 팝업을 기본으로 막는다. Google 정책도 앱이 제어하는 내장 웹뷰에서 OAuth 하는 걸 금지한다. 결국 **앱 안에서 Google/GitHub 팝업 로그인은 안 된다.**
- 이메일/비밀번호가 가장 안전한 선택지다. `getAuth()` 대신 `initializeAuth(app, { persistence: indexedDBLocalPersistence })`를 쓴다(팝업 리졸버 제외). 실제 Windows 빌드에서 테스트가 필요하다.
- Google 로그인: 시스템 브라우저 + loopback(`127.0.0.1:port`) + PKCE → `signInWithCredential`. `tauri-plugin-oauth` 2.1.0이 이 방식을 지원한다.
- GitHub 로그인: Firebase 문서에 "GitHub 토큰 → `GithubAuthProvider.credential(token)`" 경로가 있다. Device Flow 토큰도 받아주는지는 미검증이다.
- API 키에 HTTP referrer 제한을 걸면 `tauri.localhost`에서 차단될 수 있다. API 대상 제한만 쓴다.
- Windows `useHttpsScheme`을 바꾸면 IndexedDB 위치가 바뀌어서 로그인이 풀린다. 첫 출시 전에 값을 고정한다.
- CSP를 켜면 `identitytoolkit.googleapis.com`, `securetoken.googleapis.com`, `firestore.googleapis.com`를 허용해야 한다.
- firebase JS SDK 13.0.0이 2026-10-07에 나왔다. 버전은 정확히 고정한다.

**동기화 설계 시 주의**
- Firestore의 "마지막 쓰기 우선"은 **서버 도착 순서** 기준이다. 오프라인이던 기기가 나중에 옛 수정을 올리면 새 수정을 덮는다. `updatedAt` 비교를 보안 규칙이나 트랜잭션에서 직접 해야 한다.
- 오프라인 캐시 없이 리스너를 쓰면 재연결할 때마다 전체를 다시 읽어서 읽기 한도를 빨리 쓴다. `updatedAt > lastSync` 증분 조회로 하고, 쓰기는 모아서(디바운스) 보낸다.
- 보안 규칙: 하위 컬렉션까지 덮으려면 `match /users/{userId}/{document=**}`를 쓴다. 공식 예시는 create에 uid 검사가 빠져 있으니 추가한다. 문서 크기와 필드도 검증한다.
- 오프라인 상태에서 `setDoc`을 `await`하면 서버가 응답할 때까지 안 끝난다. SQLite outbox 방식으로 처리한다.

## 5. 경쟁 도구

| 도구 | 스택 · 라이선스 | 겹치는 부분 | 없는 부분 |
|---|---|---|---|
| cc-switch (14만★) | Tauri 2 · MIT | 여러 AI CLI 설정 전환, MCP·skill 동기화 | 프로젝트별 적용, git, 메모 |
| opcode (2.2만★) | Tauri 2 · AGPL | 세션·agent·CLAUDE.md 편집 | skill, 프로젝트별 토글, git |
| claude-code-tool-manager | Tauri 2 + Svelte · MIT | 프로젝트별 도구 on/off | git, CLAUDE.md, 메모 |
| xingkongliang/skills-manager (5.9천★) | Tauri 2 · MIT | 프로젝트 단위 skill 폴더 | subagent, git, 메모 |
| Nimbalyst | Electron · MIT | worktree, 작업 보드 | skill/agent 관리, 도킹 |
| Claudette | Tauri 2 · MIT | worktree, PR·CI 상태 | 설정 관리, 메모 |
| claude-squad, ccmanager | Go/Node TUI | worktree 세션 관리 | GUI, 설정 관리 |
| Claude 데스크탑 Code 탭 | 공식 | 세션 사이드바, worktree, PR 모니터 | 브랜치 메모, 외부 도킹 |

**우리 차별점**: 터미널에 붙는 사이드바 + 브랜치별 동기화 메모 + 프로젝트별 skill/subagent 적용 지도를 한 앱에서 제공한다.

**피할 이름**: Claude, Claudia, CC Switch, Crystal, Conductor, Vibe, Skills Manager, ccmanager, Claudette, Clyde, Wotch, Nimbalyst

**참고할 점**
- Claude Code **mods**(v2.1.287+): plugin이 터미널과 데스크탑 Code 탭 안에 패널을 그릴 수 있다. 나중에 연동 확장으로 검토한다.
- AGPL 프로젝트(opcode 등)의 코드는 가져오지 않는다. MIT 프로젝트는 참고할 수 있다.

## 6. 메인 창 · 문서 작성기 (2026-10-10 조사, sonnet 검증)

**노드 그래프 라이브러리**: `@xyflow/react` 12.12.0 (MIT, 2026-09-24 릴리스, React 19 호환, 타입 포함)
- 핸들 여러 개, `isValidConnection`으로 연결 검사(포트 타입 시스템은 없어서 핸들 id로 직접 구현), MiniMap·Controls·Background, `colorMode`, `toObject()`. https://reactflow.dev/api-reference/react-flow
- 실행 취소는 없음. 공식 예제는 Pro 라이선스라 코드 복사 금지. https://reactflow.dev/examples/interaction/undo-and-redo/
- 오른쪽 아래 "React Flow" 표시는 라이선스 의무가 아니라 요청 사항. https://reactflow.dev/remove-attribution
- WebKitGTK에서 HTML5 드래그 앤 드롭 버그(bug 265857): `dataTransfer.setData()`를 꼭 호출해야 동작. 블록 목록 → 캔버스 끌어놓기는 포인터 이벤트로 구현하는 게 안전. Tauri의 `dragDropEnabled`(기본 true)는 Windows에서 HTML5 DnD를 막는다. https://bugs.webkit.org/show_bug.cgi?id=265857 · https://v2.tauri.app/reference/config/
- 대안: Rete.js v2 (MIT, history 플러그인 있음). 제외: litegraph(아카이브), Flume(React 18 전용), Reaflow(2025-04 이후 멈춤).

**Tauri 창 2개**
- `tauri.conf.json` 창에 `"create": false`를 두고 `WebviewWindowBuilder::from_config`로 필요할 때 생성. 동기 command·이벤트 핸들러 안에서 창을 만들면 Windows에서 교착 → `async_runtime::spawn` 또는 async command에서 생성. https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindowBuilder.html
- capability의 `windows`는 라벨로 매칭. 앱 command를 창별로 제한하려면 `build.rs`에 `AppManifest`가 필요. https://v2.tauri.app/security/capabilities/
- 숨긴 창은 작업 표시줄에서 빠지고 WebView가 살아 있으므로 메인 창은 닫으면 파괴, 다시 열 때 생성.
- `parent()`(owner)는 쓰지 않는다(owner와 함께 파괴·항상 위). 도킹 추적기는 자기 프로세스 창을 무시하므로 메인 창이 도킹 대상이 되지 않는다.
- window-state 플러그인은 앱 종료 시에만 디스크에 저장(강제 종료 시 유실).

**Claude Code 파일 규칙** (https://code.claude.com/docs/en/memory, /skills, /sub-agents, /output-styles)
- CLAUDE.md는 200줄 이하 권장. `@path` import는 문서 정리용일 뿐 컨텍스트를 줄이지 않음(최대 4단계, 작업 폴더 밖이면 승인 창). 코드 블록·코드 스팬 안의 `@`는 import 아님 → 블록 본문의 `@`는 이스케이프 필요.
- 블록 수준 HTML 주석은 CLAUDE.md를 컨텍스트에 넣을 때 제거됨 → 관리 구역 표시용으로 적합(다른 파일 종류는 명시 없음).
- `.claude/rules/*.md`: 하위 폴더까지 읽음. `paths` 없으면 항상 로드, 있으면 맞는 파일을 다룰 때만. `paths`가 유일한 frontmatter.
- SKILL.md: 이식 가능한 frontmatter는 name·description·license·compatibility·metadata·allowed-tools. name은 소문자·숫자·하이픈 1~64자, 폴더명과 같게. description ≤1024자. 본문 500줄 이하 권장. skill도 `paths`로 범위 지정 가능(Claude Code 전용).
- subagent: name·description 필수. 본문이 시스템 프롬프트. `skills` 필드로 skill 미리 로드. 설명 합계 15,000토큰 넘으면 경고.
- AGENTS.md는 CLAUDE.md가 없을 때만 읽힘. 같이 쓰려면 CLAUDE.md에 `@AGENTS.md` (심볼릭 링크 비권장).
- `claude -p`(--bare 아님)는 신뢰 확인 없이 프로젝트 hooks·MCP를 실행 → 다듬기 기능은 `--tools ""`, `--setting-sources user` 등으로 제한.

**블록 출처 라이선스** (각 LICENSE 파일 확인)
- 사용 가능(Apache-2.0/MIT): anthropics/skills의 14개 폴더(폰트·스크립트 제외, Markdown만), anthropics/claude-plugins-official의 개별 plugin(각 LICENSE 확인, claude-security 등 예외 있음), obra/superpowers, wshobson/agents, VoltAgent/awesome-claude-code-subagents, davila7/claude-code-templates(단 Anthropic 문서 skill 복사본 제외), mattpocock/skills, shanraisshan/claude-code-best-practice, PatrickJS/awesome-cursorrules(CC0).
- 사용 불가: anthropics/skills의 docx·pdf·pptx·xlsx(독점), anthropics/claude-code 저장소(All Rights Reserved), Claude Code 문서 본문(인용 대신 요약 + 링크), hesreallyhim/awesome-claude-code(CC BY-NC-ND), 라이선스 없는 항목.
- 라이선스는 저장소가 아니라 **항목(파일) 단위**로 확인. 앱이 넣는 항목엔 출처·라이선스·커밋 SHA를 기록하고 THIRD_PARTY_NOTICES에 포함.
- 보안: plugin은 사용자 권한으로 임의 코드를 실행할 수 있음(https://code.claude.com/docs/en/plugins/security). 블록은 Markdown 본문만 가져오고 hooks·scripts·`.mcp.json`·실행 관련 frontmatter는 뺀다.

**비슷한 도구**: 블록을 골라 CLAUDE.md를 조립하는 GUI는 찾지 못함. CC Workflow Studio(React Flow, agent·command 출력, 확장은 AGPL), Ruler·rulesync(규칙 파일 변환·분배, MIT), claude-code-templates(카탈로그·설치, MIT), cc-switch(프롬프트 통째 전환). 노드 편집기 주의점: 문서는 순서가 중요하므로 순서를 명시적으로 보여주고, 200줄 초과·서로 모순되는 블록을 경고.

**디자인 참고** (https://claude.com/docs/connectors/building/mcp-apps/design-guidelines)
- 공식 색 토큰 예: 배경 #FFFFFF / #F5F4ED / #FAF9F5, 다크 #30302E / #262624 / #141413, 글자 #141413 / #3D3D3A / #73726C, 테두리는 반투명. 모서리 4~12px, 기본 버튼은 거의 검정.
- 폰트(Anthropic Sans/Serif, Styrene, Tiempos)는 독점 → OFL 대체: Hanken Grotesk·Inter, Source Serif 4, JetBrains Mono. 한글은 Pretendard(OFL).
- 로고·"Claude" 이름·전용 폰트·공식 색 이름(Clay, Ivory 등)은 쓰지 않고, 강조색은 직접 정한다. https://www.anthropic.com/legal/trademark-guidelines

## 7. Claude Code mods · 추천 mods (2026-10-12 조사: haiku, 검증: sonnet + 직접 설치 확인)

- 공식 문서: https://code.claude.com/docs/en/plugins/mods/overview (터미널은 v2.1.287+, 데스크탑 앱은 v2.1.286+). 마켓플레이스 형식: https://code.claude.com/docs/en/plugin-marketplaces
- CLI: `claude plugin marketplace add <owner/repo | 로컬 경로 | git URL>`, `claude plugin install <name>@<marketplace> --scope user`(기본 user), `claude plugin uninstall <id> --scope user`, `claude plugin list --json`(각 항목 `id`). 로컬 폴더 마켓플레이스는 설치 후에도 그 폴더에서 바로 읽는다(파일을 바꾸면 다음 세션이나 `/reload-plugins`에 반영). 같은 마켓플레이스를 다시 add해도 성공으로 끝난다.
- plugin 이름은 `claude-`, `anthropic-`로 시작하면 `claude plugin validate`가 오류를 낸다 → 우리 mod 이름은 `branch-notes`, 마켓플레이스 `ai-management`.

| mod | 하는 일 | 출처 · 설치 | 라이선스 |
|---|---|---|---|
| token-weather | 입력창 위 컨텍스트 사용량 예보 | `anthropics/claude-code-playground`의 `claude-code/mods` (마켓플레이스가 하위 폴더라 clone 후 로컬 add, 이름 `claude-code-playground-mods`) | Apache-2.0 (LICENSE 확인) |
| blast-radius | 위험한 명령(rm -rf, force push) 보류 + 진행/취소 | 위와 같음 | Apache-2.0 |
| replay-theater | `/replay`로 마지막 턴의 파일 수정 다시 보기 | 위와 같음 | Apache-2.0 |
| claude-hud | 상태줄(컨텍스트·비용·한도). mod가 아닌 statusLine plugin, 설치 후 `/claude-hud:setup` | `jarrodwatts/claude-hud` (마켓플레이스 `claude-hud`) | MIT (LICENSE 확인) |

- 제외: Git Sidebar(`davila7/claude-code-templates`, MIT)는 마켓플레이스가 아니라 `npx`로 프로젝트 `.claude/skills`에 복사하는 방식이라 넣지 않았다. 공식 `claude-plugins-official`(Apache-2.0)의 plugin들은 mod가 아니다.
- 네 개 모두 이 저장소 작업 환경에서 격리된 설정(`CLAUDE_CONFIG_DIR`)으로 실제 add·install·list를 확인했다.

## 8. 카탈로그 추가 후보 (2026-10-12 조사: haiku 3개, 검증: sonnet + 저장소 clone 후 직접 확인)

기준: 공식 출처만이 아니라 **커뮤니티에서 널리 쓰이는 것**(GitHub 별·포크·이슈·최근 커밋)도 넣는다. 단 라이선스 파일이 있고(MIT/Apache-2.0/CC0), 가져올 파일 본문에 광고·추천 링크·텔레메트리·원격 스크립트가 없어야 한다. 스크립트가 있어야 동작하는 항목은 Markdown만 가져오는 원칙상 뺀다.

**인기 지표** (2026-10-12 GitHub 페이지 기준, 별/포크): obra/superpowers 297.8k/26.6k · mattpocock/skills 286.3k/24.0k · anthropics/skills 180.5k/21.4k · shanraisshan/claude-code-best-practice 67.4k/6.7k · PatrickJS/awesome-cursorrules 40.9k/3.5k · wshobson/agents 40.4k/4.3k · davila7/claude-code-templates 32.6k/3.7k · VoltAgent/awesome-claude-code-subagents 25.7k/3.0k. 별:포크 비율(8~12:1)과 이슈·PR 수가 정상 범위라 부풀린 흔적은 보이지 않음. 최근 커밋(clone 확인): superpowers·mattpocock·anthropics 2026-10-09, VoltAgent 2026-10-11, cursorrules 2026-05-30.

**넣은 항목** (2026-10-12 카탈로그에 추가, 31개)
| 종류 | 항목 | 출처 · 라이선스 | 비고 |
|---|---|---|---|
| skill | test-driven-development (+ writing-good-tests.md), writing-plans, requesting-code-review (+ code-reviewer.md), systematic-debugging (Markdown 참고 문서만) | obra/superpowers · MIT | 본문에 외부 링크 없음 |
| skill | grill-me, to-spec, to-tickets, handoff, domain-modeling | mattpocock/skills · MIT | 본문 깨끗. 광고(뉴스레터)는 README에만 있음. TDD는 superpowers 것 하나만 |
| subagent | test-automator, architect-reviewer, accessibility-tester, api-designer, sql-pro, refactoring-specialist, technical-writer, devops-engineer, java-architect, kotlin-specialist, swift-expert, csharp-developer, php-pro | VoltAgent/awesome-claude-code-subagents · MIT | 본문의 JSON "context manager" 통신 규약(Claude Code 기능 아님) 삭제, 고정된 `model` 제거. code-reviewer·debugger는 내장 블록이 있어 제외 |
| rule | Next.js App Router, FastAPI, Docker, Flutter Riverpod, Spring Boot JPA, Kotlin Ktor, Laravel, HTMX+Django | PatrickJS/awesome-cursorrules `rules/*.mdc` · CC0-1.0 | 규칙 본문 깨끗(스폰서 광고는 README에만). Cursor frontmatter(`globs`)를 `paths`로 변환 |

**제외 (직접 확인한 이유)**
- contains-studio/agents (별 12.4k): LICENSE 파일 없음, 커밋 10개 → 사용 불가.
- obra/superpowers `brainstorming`: 선택 기능(visual companion) 서버가 primeradiant.com 로고를 불러오는 텔레메트리 포함(환경 변수로 끔). Markdown만 가져오면 기능이 깨져서 제외.
- anthropics/skills `skill-creator`, `mcp-builder`, `webapp-testing`: Python 스크립트가 있어야 동작. `doc-coauthoring`: LICENSE.txt 없음. `brand-guidelines`: Anthropic 브랜드라 금지.
- mattpocock `improve-codebase-architecture`(CDN 스크립트를 불러오는 HTML 보고서), `diagnosing-bugs`(셸 스크립트 템플릿).
- centminmod/my-claude-code-setup: Z.AI 추천 코드(할인 링크)와 원격 스크립트 실행 설정.
- TheDecipherist/claude-code-mastery: 저자가 스스로 구버전이라 밝힘, 홍보 배너, `~/.claude/settings.json` 덮어쓰기.
- lst97/claude-code-sub-agents: 2025-08 이후 업데이트 없음, VoltAgent와 겹침. iannuttall/claude-agents: 보관(archived).
- wshobson/commands: 보관됨. anthropics/claude-code의 `/commit` 등: All Rights Reserved → 요약·링크만 가능. 명령(slash command)은 신뢰할 만한 공개 출처가 없어 직접 만드는 쪽이 낫다.
- davila7/claude-code-templates: 스폰서(Bright Data, Z.AI UTM)와 분석 기능 → 이전처럼 개별 파일만 골라 쓰고 새로 늘리지 않음. github/awesome-copilot: Copilot 전용 형식.
- 제3자 카탈로그 사이트(skillselion, vibeindex 등)는 숫자가 서로 달라 출처로 쓰지 않음.

## 9. 초보자용 자료 (2026-10-12 조사: haiku)

**공식 기능** (https://code.claude.com/docs/en/output-styles)
- 출력 스타일 **Explanatory**: 구현 선택과 코드베이스 패턴을 짧은 "Insight"로 설명해 준다. **Learning**: 같은 설명에 더해, 작은 부분(오류 처리, 비즈니스 로직 등)은 `TODO(human)` 표시를 남겨 사용자가 직접 짜게 한다.
- 켜는 법: `/output-style <이름>` 또는 `/config`. `.claude/settings.local.json`에 `"outputStyle": "Explanatory"`(대소문자 구분, 오타면 조용히 기본값)로 저장된다. 사용자 스타일은 `~/.claude/output-styles` 또는 `.claude/output-styles`의 Markdown(`name`, `description`, `keep-coding-instructions`). `keep-coding-instructions: true`가 없으면 Claude Code 기본 코딩 지침이 빠진다.
- `/init`: 프로젝트를 보고 CLAUDE.md 초안을 만든다(이미 있으면 덮어쓰지 않고 개선 제안).
- skills 문서에 `explain-code` 예제(그림·비유로 코드 설명)가 있다. 문서 본문은 복사하지 않고 요약·링크만.

**커뮤니티** (별 수는 2026-10-12 한 번 확인)
- luongnv89/claude-howto (MIT로 표기, 별 41.8k): 초보자 과정이 있는 튜토리얼. 지침 파일이 아니라 학습 자료. README에 광고·강의 판매 없음. LICENSE 파일은 아직 직접 확인 안 함.
- LukeRenton/explore-claude-code (MIT, 317): `.claude/` 기능을 클릭하며 배우는 예제 프로젝트.
- 그 밖에 작은 저장소·블로그(초보자용 CLAUDE.md 템플릿)는 인지도·검증이 부족해 제외. 초보자를 위한 검증된 공개 CLAUDE.md 템플릿은 찾지 못함.

**제안**: 가장 확실한 건 공식 Explanatory·Learning 스타일(라이선스 문제 없음)을 설정에서 켜게 하는 것. 그 위에 앱 자체 초보자 지침 블록(쉬운 말로 설명, 명령 실행 전 무엇을 하는지 말하고 확인 받기, 오류는 원인 한두 문장 + 해결, 삭제·덮어쓰기·push 전 확인)을 직접 써서 내장 블록으로 넣는 것.

## 10. Agent Skills Hub · 비슷한 앱 (2026-10-12 조사: haiku, 저장소 clone 후 직접 확인)

**Agent Skills Hub (https://agentskillshub.dev/)**
- skill·MCP 모음 사이트. 유료 보안 인증($99)과 후원 광고를 팔고, skill별 출처 저장소·라이선스를 보여주지 않는다. 별 개수도 GitHub와 다르다(obra/superpowers 142k vs 실제 약 298k). 그래서 사이트 자체는 출처로 쓰지 않고, 거기 올라온 skill의 **원본 저장소**를 직접 확인했다.
- 추가: mattpocock/skills(MIT, 커밋 `49dd158`)의 research, retro, writing-for-agents, triage, improve-codebase-architecture, teach. 모두 Markdown만 있고, 광고·추천인 링크·외부 스크립트 실행은 없다. improve-codebase-architecture의 HTML 보고서는 Tailwind·Mermaid를 CDN에서 불러온다(설명에 적음).
- 제외: mattpocock `pr`(다른 저장소 skill을 참고했다고 적혀 있고 그쪽 라이선스 미확인), obra `writing-skills`(681줄 + Anthropic 문서 사본 `anthropic-best-practices.md`), obra `brainstorming`(이전 조사대로 제외), 스크립트가 있어야 하는 skill(subagent-driven-development, executing-plans, webapp-testing, mcp-builder, skill-creator 등), anthropics docx/pdf/pptx/xlsx(소스만 공개, 오픈소스 아님), doc-coauthoring(라이선스 없음).

**비슷한 앱** (별 개수는 조사 당시 대략값)
| 앱 | 라이선스 | 특징 |
|---|---|---|
| [tylergraydev/claude-code-tool-manager](https://github.com/tylergraydev/claude-code-tool-manager) (~380★) | MIT | Tauri 2 + Svelte. MCP 관리·테스트, 슬래시 명령·skills·subagents·hooks 편집, 프로필(묶음 전환), 상태줄 빌더, 사용량·비용 통계, 세션 기록 탐색, 프로젝트별 할당 |
| [winfunc/opcode](https://github.com/winfunc/opcode) (~22k★) | AGPL-3.0 | 세션 이어하기·검색, 체크포인트·타임라인, 사용량, MCP 관리 |
| [ryoppippi/ccusage](https://github.com/ryoppippi/ccusage) (~19k★) | MIT | 사용량·비용 리포트(일·주·월·세션, 5시간 창) |
| [siteboon/claudecodeui](https://github.com/siteboon/claudecodeui) (~14k★) | AGPL-3.0 | 웹·모바일 UI, 파일 탐색·편집, git |
| [smtg-ai/claude-squad](https://github.com/smtg-ai/claude-squad) (~8.6k★) | AGPL-3.0 | worktree마다 에이전트 여러 개, 적용 전 검토 |
| [davila7/claude-code-templates](https://github.com/davila7/claude-code-templates) (~33k★) | MIT | agents·commands·hooks·MCP 설치, 설정 점검 |
| [musistudio/claude-code-router](https://github.com/musistudio/claude-code-router) (~38k★) | MIT | 다른 모델로 라우팅(범위 밖) |

AGPL 프로젝트는 아이디어만 참고하고 코드는 가져오지 않는다.

## 11. MCP 서버 관리 (2026-10-12 조사: haiku, 검증: sonnet, claude 2.1.296으로 직접 확인)

**설정 위치와 방식** (code.claude.com/docs/en/mcp, `claude mcp --help`)
- 범위: project = 프로젝트 루트 `.mcp.json`(git 공유), local = `~/.claude.json`의 `projects[<경로>].mcpServers`, user = `~/.claude.json`의 `mcpServers`. 이름이 겹치면 local > project > user 순서로 쓴다. 이 밖에 플러그인과 claude.ai 커넥터 서버가 있다.
- `~/.claude.json`에는 `oauthAccount` 같은 계정 정보도 있어서 앱은 이 파일을 열지 않는다. local과 user 범위는 `claude mcp list/get/add/remove`로만 읽고 쓴다.
- 항목 형식: stdio `{type, command, args, env}`, http·sse `{type, url, headers}`, ws `{type, url}`. `type`이 없으면 command가 있을 때 stdio로 본다. 값 안의 `${VAR}`, `${VAR:-기본값}`은 Claude Code가 불러올 때 펼친다.
- `claude mcp list`: JSON 출력이 없다. 한 줄 형식은 `이름: 명령|URL (HTTP) - 상태`이고, 연결 상태를 실제로 확인한다(✓ Connected, × Failed to connect, ⏸ Pending approval). 빈 줄 뒤에는 "MCP config diagnostics"가 나온다. `disabledMcpjsonServers`에 있는 project 서버는 목록에서 빠진다. 종료 코드는 항상 0이다.
- `claude mcp get <이름>`: `Scope:`, `Status:`, `Type:`, `Command:`, `Args:`(공백으로 이어 붙임), `Environment:`(4칸 들여쓰기 `K=V`), `URL:`, `Headers:`를 보여준다. 값은 가리지 않는다. ws 서버는 Type과 URL이 빠진다. 없는 이름이면 종료 코드 1이다.
- `claude mcp add <이름> [옵션] -- <명령|URL> [인자…]`: `-e`/`-H`는 뒤의 값을 여러 개 받는다(variadic). 그래서 이름을 맨 앞에 쓴다. 이미 있는 이름이나 잘못된 이름이면 종료 코드 1이다. `add-json`은 ws도 받는다.
- 프로젝트 승인: 설정의 `enableAllProjectMcpServers`, `enabledMcpjsonServers`, `disabledMcpjsonServers`가 정한다. disabled는 바로 적용되지만, enabled는 폴더를 신뢰한 뒤에만 적용된다. `/mcp`의 켜기/끄기(local·user 서버)는 `~/.claude.json`에 저장돼서 앱이 바꾸지 않는다.
- Windows에서 npx 서버를 `cmd /c`로 감싸야 하는지는 확인하지 못했다(문서마다 다름). 추천 서버는 Windows에서 `cmd /c npx …`로 넣는다.

**추천 서버** (앱은 설정만 쓰고 서버 코드를 포함하지 않음, 원본 저장소에서 라이선스 확인)
| 서버 | 출처 | 라이선스 | 비고 |
|---|---|---|---|
| filesystem, memory | modelcontextprotocol/servers | MIT (새 기여분 Apache-2.0) | npx |
| git, fetch, time | modelcontextprotocol/servers | MIT | uvx (uv 필요) |
| github | github/github-mcp-server | MIT | 원격 `https://api.githubcopilot.com/mcp/`, 토큰은 `${GITHUB_PAT}` |
| playwright | microsoft/playwright-mcp | Apache-2.0 | `--isolated` |
| chrome-devtools | ChromeDevTools/chrome-devtools-mcp | Apache-2.0 | 사용 통계·CrUX 전송 끄는 옵션 포함 |
| context7 | upstash/context7 | MIT(클라이언트) | 원격 `https://mcp.context7.com/mcp` |
| notion, linear | 업체 운영 | 해당 없음 | 공개 URL만 씀, `/mcp`로 로그인 |

- 제외: Sentry(코드 라이선스 FSL, 소스만 공개), Supabase·Cloudflare(이번에는 보류), sequential-thinking(효용이 애매함), 예전 Notion npm 서버(지원 중단).

## 12. 카탈로그·MCP 추가 조사 (2026-10-12 조사: haiku, 검증: sonnet, 저장소 clone 후 직접 확인)

사용자가 준 출처 목록(awesome 목록, superpowers, claude4world, awesome-mcp-servers)을 조사했다. 목록 자체의 라이선스는 거기 연결된 프로젝트에 적용되지 않으므로, 연결된 저장소마다 LICENSE를 직접 확인했다.

**출처별 결론**
- hesreallyhim/awesome-claude-code: 목록이 CC BY-NC-ND라 내용은 가져오지 않고 찾는 용도로만 썼다.
- ComposioHQ/awesome-claude-skills: LICENSE 파일이 없고, 스킬 832개 모두 Composio 호스팅 서비스(Rube MCP)가 필요해 제외.
- VoltAgent/awesome-design-md: 실제 회사(애플, 나이키, 스트라이프 등)의 브랜드를 따라 하는 문서라 상표 위험으로 제외.
- claude4world.com/skills: 항목별 출처·라이선스를 보여주지 않아 출처로 쓰지 않음.

**넣은 항목**(27개, 출처·커밋·변경 사항은 THIRD_PARTY_NOTICES.md)
- addyosmani/agent-skills 5개
- everything-claude-code 스킬 5개
- revfactory/harness 2개(한국어)
- obra writing-skills
- microsoft zustand-store-ts
- supabase-postgres-best-practices
- tjboudreaux pre-mortem·reversibility
- mhattingpete test-fixing
- caveman
- OneRedOak design-review 에이전트와 CLAUDE.md 조각
- ibelick baseline-ui·fixing-accessibility
- marketingskills 4개

참고 파일이 있는 스킬은 이번에 추가한 "동반 파일" 방식으로 SKILL.md 옆에 따로 쓴다.

**사용자가 골랐지만 넣지 않은 것**(Anthropic 콘텐츠를 바탕으로 만든 것으로 보이고, 그 원본의 라이선스를 확인할 수 없음)
- addyosmani `code-simplification`: Claude Code Simplifier 플러그인에서 영감을 받았다고 밝히고, 구조가 같다.
- everything-claude-code `silent-failure-hunter`, `pr-test-analyzer`: Anthropic pr-review-toolkit 에이전트를 줄여 쓴 것으로 보인다.
- OneRedOak 보안 리뷰 명령: README에 Anthropic claude-code-security-review에서 가져왔다고 적혀 있다. Claude Code에 내장된 `/security-review`와도 같다.

**그 밖의 제외**
| 항목 | 이유 |
|---|---|
| karpathy CLAUDE.md | LICENSE 파일 없음 |
| ccstatusline | Claude 로그인 토큰을 읽음 |
| tdd-guard | 도구를 쓸 때마다 `npx @latest` 실행 |
| claude-code-safety-net | 확인할 수 없는 컴파일된 번들 |
| claude-powerline | 이름이 Claude 브랜딩 |
| trailofbits/skills, osint-investigation | CC BY-SA |
| context-engineering-kit | GPL-3.0 |
| humanizer, marketingskills ai-tells.md | 위키백과(CC BY-SA)를 바탕으로 함 |
| eskill | 사용만 허락하는 자체 라이선스 |
| marketingskills `ai-seo` | 제휴 홍보 |
| obra/the-elements-of-style | LICENSE 파일 없음. 사용자 결정으로 제외 |

**MCP 추천 추가**(8개)
| 서버 | 출처 · 라이선스 | 설정 |
|---|---|---|
| Cloudflare | cloudflare/mcp · Apache-2.0 | 원격 OAuth |
| Supabase | supabase-community/supabase-mcp · Apache-2.0 | 원격 OAuth, `read_only=true`로 넣음 |
| Stripe | stripe/agent-toolkit · MIT | 원격 OAuth |
| AWS 문서 | awslabs/mcp · Apache-2.0 | uvx |
| Postgres MCP Pro | crystaldba · MIT | uvx, `--access-mode=restricted`(읽기 전용) + `DATABASE_URI` |
| Next.js DevTools | vercel · MIT | 텔레메트리 기본 켜짐 → `NEXT_TELEMETRY_DISABLED=1` |
| Atlassian | sooperset/mcp-atlassian · MIT | 커뮤니티, 토큰 환경변수 |
| Figma | GLips/Figma-Context-MCP · MIT | 커뮤니티, 텔레메트리 기본 켜짐 → `FRAMELINK_TELEMETRY=off`, `DO_NOT_TRACK=1`. README에 자사 사이트 UTM 링크가 있지만 서버 자체와는 무관 |

**MCP 제외**
| 서버 | 이유 |
|---|---|
| Sentry | FSL-1.1, 소스만 공개 |
| Slack(korotovsky) | 브라우저 세션 토큰 필요 |
| SQLite 서버들 | 6개월 넘게 업데이트 없음 |
| Docker Hub | 소스에서 직접 빌드해야 함 |

검색 서버(Tavily, Exa, Brave)는 라이선스 문제는 없지만, 사용자가 이번에 고르지 않았다.

## 13. Codex 설정 구조 (2026-10-13 조사, sonnet이 소스 코드로 검증)

출처: [AGENTS.md](https://learn.chatgpt.com/docs/agent-configuration/agents-md), [config basic](https://learn.chatgpt.com/docs/config-file/config-basic), [config reference](https://learn.chatgpt.com/docs/config-file/config-reference), [skills](https://learn.chatgpt.com/docs/build-skills), [subagents](https://learn.chatgpt.com/docs/agent-configuration/subagents), [rules](https://learn.chatgpt.com/docs/agent-configuration/rules), [hooks](https://learn.chatgpt.com/docs/hooks), [worktrees](https://learn.chatgpt.com/docs/environments/git-worktrees), [non-interactive](https://learn.chatgpt.com/docs/non-interactive-mode), [managed config](https://learn.chatgpt.com/docs/enterprise/managed-configuration), [Windows app](https://learn.chatgpt.com/docs/windows/windows-app). (developers.openai.com/codex/* 주소가 여기로 넘어간다.) 소스는 [openai/codex](https://github.com/openai/codex) `codex-rs/`(2026-10-13 main).

- 버전: CLI 0.162.1(2026-10-09). 실행 파일 `codex`(Windows `codex.exe`, npm 설치면 `codex.cmd`). 설치: `npm install -g @openai/codex` 또는 `irm https://chatgpt.com/codex/install.ps1 | iex`.
- 홈 폴더: `CODEX_HOME`, 없으면 `~/.codex`(Windows `%USERPROFILE%\.codex`). 로그인 정보 `auth.json`은 읽지 않는다.
- **AGENTS.md** (`core/src/agents_md.rs`)
  - 전역: `$CODEX_HOME/AGENTS.override.md`가 비어 있지 않으면 그것, 아니면 `AGENTS.md`. 하나만 읽는다.
  - 프로젝트: 루트(`project_root_markers`, 기본 `[".git"]`이 있는 가장 가까운 상위 폴더)부터 현재 폴더까지 폴더마다 `AGENTS.override.md` → `AGENTS.md` → `project_doc_fallback_filenames` 중 **처음 있는 하나만** 읽고, 위에서 아래로 이어 붙인다. 루트가 없으면 현재 폴더만.
  - 프로젝트 문서 합계 `project_doc_max_bytes`(기본 32KiB)를 넘으면 넘는 파일을 바이트 단위로 자르고 이후 파일은 버린다. 빈 파일은 건너뛴다.
  - **HTML 주석을 지우지 않는다.** 작성기의 관리 구역 표시 주석도 모델에 그대로 들어간다.
  - 프로젝트가 `untrusted`로 표시되면 프로젝트 AGENTS.md를 전혀 읽지 않는다.
  - Codex는 CLAUDE.md를 읽지 않는다(`project_doc_fallback_filenames`에 넣으면 읽음).
- **config.toml**: 우선순위(높은 순) CLI 플래그 > 프로젝트 `.codex/config.toml`(루트→현재 폴더, 가까운 쪽 우선, **신뢰한 프로젝트만**) > `--profile`(`$CODEX_HOME/<이름>.config.toml`) > 사용자 `~/.codex/config.toml` > 클라우드 관리 기본값 > `/etc/codex/config.toml` > 기본값.
  - 신뢰: 사용자 설정 `[projects."<경로>"] trust_level = "trusted" | "untrusted"`. Windows에서는 경로를 정규화하고 **소문자**로 쓴다.
  - 관리 정책: `requirements.toml`(Unix `/etc/codex/`, Windows `%ProgramData%\OpenAI\Codex\`), 예전 `managed_config.toml`(Unix `/etc/codex/`, Windows `~/.codex/`).
  - 프로젝트별 개인 설정 파일(`config.local.toml` 같은 것)은 **없다**.
- **Skill** (`ext/skills/src/host_roots.rs`, `skills/src/parser.rs`)
  - 위치: 현재 폴더부터 저장소 루트까지의 `.agents/skills`, 각 프로젝트 설정 층의 `.codex/skills`, `~/.agents/skills`, `$CODEX_HOME/skills`(구 위치, 아직 읽음), `/etc/codex/skills`, 내장 skill.
  - SKILL.md는 frontmatter가 필수. `description` 필수, `name`은 없으면 폴더 이름(최대 64자). 문서는 둘 다 필수라고 하므로 둘 다 쓴다. 같은 이름은 합치지 않고 둘 다 보인다.
  - 끄기: `[[skills.config]] path = "<SKILL.md 절대 경로>"`, `enabled = false`. **사용자 설정에서만** 적용되고 프로젝트 `.codex/config.toml`의 값은 무시된다. 나중 항목이 앞 항목을 덮는다.
- **Subagent**: `~/.codex/agents/*.toml`, `.codex/agents/*.toml`(파일 하나에 agent 하나). 필수 `name`·`description`·`developer_instructions`. Markdown 형식은 없다. **하나만 끄는 설정은 없고** `agents.enabled = false`로 전부 끈다.
- **MCP**: `[mcp_servers.<이름>]`(사용자·신뢰한 프로젝트 설정). `command`/`args` 또는 `url`. `enabled = false`면 시작하지 않는다.
- **Custom prompts**(`~/.codex/prompts/*.md`): 폐지 예정(deprecated), skill로 대체.
- **Rules**(`~/.codex/rules/*.rules`, 프로젝트 `.codex/rules/`): Starlark로 쓴 **명령 실행 허용 정책**이다. 지침이 아니다.
- **Hooks**: `hooks.json` 또는 config.toml `[hooks]`(사용자·프로젝트). 신뢰 확인이 필요하다.
- **Worktree**(Codex 앱): `$CODEX_HOME/worktrees/<UUID 앞 4자리>/<레포 폴더 이름>`에 브랜치 없이(detached) 만든다. `[desktop] git-worktree-root`로 위치 변경, 오래된 것은 자동 정리된다.
- **Hooks 형식** (`config/src/hook_config.rs`, `hooks/`): `hooks.json` = `{"description"?, "hooks": {<이벤트>: [{"matcher"?, "hooks": [핸들러…]}]}}`로 **Claude Code의 `hooks` 객체와 같은 모양**이다. 알 수 없는 최상위 키가 있으면 파일 전체가 거부된다. config.toml에서는 `[[hooks.PreToolUse]]` + `[[hooks.PreToolUse.hooks]]`로 쓴다. 한 층에 둘 다 있으면 둘 다 실행된다(경고).
  - 이벤트 12개: PreToolUse, PermissionRequest, PostToolUse, PreCompact, PostCompact, SessionStart, SessionEnd, UserPromptSubmit, SubagentStart, SubagentStop, Stop, Interrupt.
  - 핸들러: `{"type":"command","command","commandWindows"?,"timeout"?(초, 기본 600),"async"?,"statusMessage"?}` 또는 `{"type":"mcp_tool",…}`. `prompt`·`agent` 형식은 무시된다.
  - 실행 규약이 Claude와 일부 다르다: PreToolUse는 `permissionDecision:"deny"`(이유 필수)만 지원하고 `ask`는 거부, Stop은 stdout에 JSON 필수. 명령은 `$SHELL -lc`(Windows `cmd /C`).
  - 신뢰: 관리 정책이 아닌 hook은 사용자가 `/hooks`에서 신뢰해야 실행된다. 신뢰는 hook 내용의 해시로 사용자 config.toml `[hooks.state."<키>"] trusted_hash`에 기록된다. **앱이 대신 신뢰 처리하지 않는다.** 내용이 바뀌면 다시 신뢰해야 한다. `[features] hooks = false`로 전부 끈다.
- **MCP CLI** (`cli/src/mcp_cmd.rs`): `codex mcp list [--json]`, `get <이름> [--json]`, `add <이름> (--url <URL> | -- <명령>…) [--env K=V]`, `remove`, `login`, `logout`. `add`·`remove`는 **사용자 `$CODEX_HOME/config.toml`에만** 쓴다(프로젝트 범위 없음). `list --json`은 `{name, enabled, disabled_reason, transport, …, auth_status}` 배열. **연결 확인(health check)은 하지 않는다.**
- **`codex exec`로 한 번 답 받기**: `codex exec --ephemeral --skip-git-repo-check -s read-only --ignore-user-config --ignore-rules -C <빈 폴더> -o <임시 파일> -`(프롬프트는 stdin). 최종 답은 stdout과 `-o` 파일에 나오고 진행 상황은 stderr. 승인 정책은 항상 Never. 도구를 완전히 끄는 옵션은 없다(읽기 전용 샌드박스가 한계). 빈 폴더에서 실행하면 프로젝트 AGENTS.md·설정·hook을 읽지 않는다.
- **슬래시 명령**: 사용자가 만드는 명령은 폐지 예정인 `~/.codex/prompts`뿐이고, 대체는 skill(`$이름` 또는 `/skills`)이다.
- **headless**: `codex exec`(기본 읽기 전용 샌드박스) `--json`, `--output-schema <파일>`, `--skip-git-repo-check`. 로그인 확인은 `codex login status`의 **종료 코드**(0 로그인, 1 아님).
- **데스크탑 앱(Windows)**: "ChatGPT desktop app"(Microsoft Store). 실행 파일·프로세스 이름과 Electron 여부는 공식 자료에 없다 → 실기 확인 필요.

## 14. 다른 도구 (Copilot, Antigravity) — 어댑터 설계 참고용 (2026-10-13, haiku 조사, 미검증)

출처: [Copilot custom instructions](https://code.visualstudio.com/docs/copilot/customization/custom-instructions), [agent skills](https://code.visualstudio.com/docs/copilot/customization/agent-skills), [custom agents](https://code.visualstudio.com/docs/copilot/customization/custom-agents), [Copilot CLI config dir](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-config-dir-reference), [Antigravity rules](https://www.antigravity.google/docs/rules), [skills](https://www.antigravity.google/docs/skills), [MCP](https://www.antigravity.google/docs/mcp)

| 개념 | GitHub Copilot (VS Code · CLI `copilot`) | Google Antigravity (IDE · CLI `agy`) |
|---|---|---|
| 항상 읽는 지침 | `.github/copilot-instructions.md`, `AGENTS.md`, `CLAUDE.md`, `~/.copilot/copilot-instructions.md` | `AGENTS.md`, `GEMINI.md`, `.agents/AGENTS.md`, `~/.gemini/AGENTS.md` |
| 경로별 지침 | `.github/instructions/*.instructions.md`(`applyTo`) | `.agents/rules/*.md`(`trigger`, `globs`) |
| Skill | `.github/skills`, `.claude/skills`, `.agents/skills`, `~/.copilot/skills` | `.agents/skills`, `~/.gemini/config/skills` |
| Agent | `.github/agents/*.agent.md`, `~/.copilot/agents` | 없음 |
| MCP | `.vscode/mcp.json`(`servers`), `.mcp.json`, `~/.copilot/mcp-config.json` | `.agents/mcp_config.json`, `~/.gemini/config/mcp_config.json` |
| 끄기 | CLI `settings.json`의 `disabledSkills`(개인 `.github/copilot/settings.local.json` 있음) | MCP `disabled: true`, 그 밖에는 미확인 |

- 공통: `AGENTS.md`와 `.agents/skills/<이름>/SKILL.md`는 Codex·Copilot·Antigravity가 함께 읽는다. 앱이 Codex용 skill을 `.agents/skills`에 넣으면 다른 도구에서도 보인다.
- Gemini CLI는 2026-06에 Antigravity CLI로 대체됐다.

## 15. 세션 기록 파일 (작업 트리용, 2026-10-14: Claude Code는 실제 파일 확인, Codex는 sonnet이 소스로 검증)

둘 다 **공개 규격이 아니다**. 버전 필드가 없고 필드가 자주 늘어나므로 모르는 줄·필드·종류는 건너뛴다. 읽기만 하고 로그인 정보 파일은 열지 않는다.

**Claude Code** (claude 2.1.x 실제 파일)
- 위치: `~/.claude/projects/<작업 폴더 경로의 / 등을 -로 바꾼 이름>/<세션 id>.jsonl` (`CLAUDE_CONFIG_DIR`가 있으면 그 아래). 같은 폴더에 세션 id 이름의 하위 폴더(subagent 기록 등)도 있다.
- 줄마다 JSON 하나, `type`: `user`, `assistant`, `system`, `attachment`, `last-prompt`, `queue-operation`, `mode`, `cost-state` 등.
- 공통 필드: `uuid`, `parentUuid`, `sessionId`, `timestamp`(ISO), `cwd`, `gitBranch`, `isSidechain`(subagent면 true), `isMeta`.
- **사람이 보낸 프롬프트**: `type:"user"`, `isMeta`·`isSidechain`이 아니고, `message.content`가 문자열이거나(이미지를 붙인 프롬프트는 `text`·`image` 블록 배열 — `text`를 이어 붙이고 이미지는 "[이미지]"로 적는다. `tool_result` 블록이 있으면 프롬프트가 아니다. `[Request interrupted by user…]`는 제외), `origin.kind == "human"`(있을 때. 없으면 `<task-notification>`·`<command-` 등으로 시작하는 자동 메시지를 제외). `promptId`가 있다.
- 프로젝트 폴더 이름은 작업 폴더에서 영문자·숫자가 아닌 글자를 모두 `-`로 바꾼 것이라(200자가 넘으면 잘림), 저장소 폴더 이름으로 시작하지 않는 폴더는 열지 않는다. 한 줄이 4 MB를 넘으면 읽지 않고 건너뛴다.
- 도구 사용: `type:"assistant"`의 `message.content[]` 중 `type:"tool_use"` — `Edit`/`Write`/`MultiEdit`/`NotebookEdit`의 `input.file_path`(절대 경로), `Bash`의 `input.command`(`git commit`, `git push` 감지). 커밋 sha는 그 tool_use에 대한 결과(`type:"user"`, `toolUseResult`/`message.content[].type:"tool_result"`)의 출력에서 `[branch abc1234]` 형식으로 얻는다.

**Codex** (`codex-rs/` HEAD 2026-10-14, 소스 기준. 예시는 serde 정의로 만든 것)
- 위치: `$CODEX_HOME/sessions/YYYY/MM/DD/rollout-<로컬시각>-<thread id>.jsonl`, 보관된 것은 `archived_sessions/`. **오래된 파일은 `.jsonl.zst`(zstd)로 압축될 수 있다.** `codex exec --ephemeral`은 기록을 남기지 않는다.
- 줄: `{"timestamp", "type", "payload"}`(새 파일은 `ordinal`도). `type`: `session_meta`, `turn_context`, `response_item`, `event_msg`, `compacted` 등.
- `session_meta.payload`: `id`, `cwd`, `source`(`cli`·`vscode`·`exec`·`{sub_agent}` 등 — subagent 세션은 건너뜀), `git.branch`·`git.commit_hash`(세션 시작 시점만). 턴마다 `turn_context.payload.cwd`.
- **사람이 보낸 프롬프트**: 가장 확실한 신호는 `event_msg`의 `user_message`(`payload.message`). paginated 기록 방식에서는 `event_msg`의 `item_completed`(TurnItem `user_message`)로 바뀐다 — 둘 다 지원. `response_item`의 `role:"user"` 메시지는 `<environment_context>`, `<user_instructions>`(AGENTS.md) 등 주입된 내용이 섞여 있어 그대로 쓰지 않는다.
- 파일 수정: `response_item`의 `custom_tool_call`(`name:"apply_patch"`, `input`=패치 글) 또는 `function_call`. 경로는 `^\*\*\* (Add File|Update File|Delete File|Move to): (.+)$`. 명령: `function_call` `exec_command`의 `arguments`(JSON 글)의 `cmd`, 또는 `local_shell_call.action.command`.
- `history.jsonl`(`{session_id, ts, text}`)은 프롬프트 기록이지만 저장소 정보가 없어 쓰지 않는다.
- 인증 토큰은 구조에 없지만 명령 출력·파일 내용이 들어 있으니 민감 자료로 다룬다.
