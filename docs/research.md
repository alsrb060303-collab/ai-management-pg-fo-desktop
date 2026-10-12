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
