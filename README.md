# AI Management (임시 이름)

Claude Code로 작업할 때 **터미널 옆에 붙어서** 문서·git 작업·브랜치 메모를 관리하는 데스크탑 앱. 메인 창과, 터미널 옆에 자동으로 붙는 사이드바(둘 중 하나만 표시)로 되어 있다. Windows 우선, Linux는 이후.

> 🚧 개발 중. 계획과 결정은 [`docs/PLAN.md`](docs/PLAN.md), 조사 자료는 [`docs/research.md`](docs/research.md), Windows 테스트 순서는 [`docs/testing/phase4-windows.md`](docs/testing/phase4-windows.md).

## 기능

**문서**
- **문서 작성기**: 지침·규칙·skill·subagent 블록을 캔버스에서 이어 붙여 `CLAUDE.md`, `.claude/rules`, `.claude/skills`, `.claude/agents` 파일을 만든다. 블록을 누르면 설명과 실제로 들어갈 내용이 보이고, 블록이나 출력을 더블클릭하면 그 파일을 미리 본다. hook 블록은 `.claude/settings.json`의 hooks에 합쳐 넣고(명령 확인 후), '직접 입력' 출력은 원하는 경로(예: AGENTS.md)에 내 글과 블록을 쓴다. 블록마다 켜기/끄기 스위치가 있고, subagent는 모델(자동·Fable·Opus·Sonnet·Haiku)을 고를 수 있다.
- **블록 카탈로그**: 내장 블록과, 라이선스를 확인한 공개 저장소 항목(obra/superpowers, mattpocock/skills, anthropics/skills, wshobson/agents, VoltAgent subagents, awesome-cursorrules). 출처·커밋·라이선스가 함께 기록된다([`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md)).
- **설정 지도**: 프로젝트에서 Claude Code가 실제로 읽는 지침·규칙·skill·subagent·명령·MCP·설정을 범위(개인/프로젝트/로컬)와 상태(사용 중/조건부/꺼짐 등)와 함께 보여준다.
- **프로젝트 문서**: 폴더를 연결하면 CLAUDE.md(하위 폴더 포함)·AGENTS.md·rules·skills·agents·commands·설정 파일을 자동으로 찾아 앱 안에서 읽고 고친다. 밖에서 파일이 바뀌면 알려준다.
- **라이브러리**: 자주 쓰는 skill·subagent를 모아 두고 프로젝트에 적용하거나 뺀다.

**MCP 서버**
- 프로젝트(.mcp.json)·로컬·개인 MCP 서버를 한 화면에서 보고 연결 상태를 확인하고, 추가·수정·삭제한다. 개인 서버는 설치된 `claude mcp` 명령으로 다룬다(로그인 정보가 있는 `~/.claude.json`은 열지 않음).
- 프로젝트 서버를 켜고 끄고, 라이선스를 확인한 추천 서버를 넣거나, README의 JSON·`claude mcp add` 명령을 붙여넣어 가져온다. 비밀값은 가려서 보여주고 `${변수}` 참조로 바꾸도록 안내한다.

**작업 (git · GitHub)**
- 브랜치·worktree 만들기·전환·삭제, 변경 파일과 diff, **커밋·push·pull**.
- GitHub 로그인(Device Flow): 브랜치의 PR·CI, 열린 PR·이슈, PR 만들기.
- **내 GitHub 저장소** 목록에서 고르기: clone이 있으면 그 폴더를 열고, 없으면 GitHub에서 바로 브랜치·커밋·PR을 보고 **파일을 고쳐 커밋**하거나 clone한다.

**메모**
- 저장소·브랜치마다 메모와 할 일. 터미널에서 브랜치를 바꾸면 따라 바뀐다.
- 여러 기기 동기화(Firebase, 허용 목록 계정만): 메모·할 일, 문서 작성기 문서, 내 라이브러리. 두 기기에서 같이 고치면 나중 것을 남기고 다른 것은 사본으로 보관한다. [`docs/sync-setup.md`](docs/sync-setup.md)

**Claude Code 연동 (mods, Claude Code 2.1.287+)**
- 설정에서 **브랜치 메모 mod**를 설치하면 Claude Code 입력창 위에 이 브랜치의 할 일·PR·CI가 한 줄로 보이고, `/todo` · `/memo` · `/notes`를 쓸 수 있고, Claude가 메모와 할 일을 알고 작업한다. 터미널 Claude Code와 데스크탑 앱의 Code 탭에 적용된다.
- 추천 mods(token-weather, blast-radius, replay-theater, claude-hud)도 설정에서 설치할 수 있다.

**사이드바**
- Windows Terminal · VS Code · Claude 데스크탑 중 지금 쓰는 창 옆에 자동으로 붙는다. 전역 단축키로 보이고 숨긴다.

## 개발 환경

필요한 것
- Node.js 22+ 와 pnpm 10 (`corepack enable`)
- Rust 1.90+ (`rustup`)
- Windows: [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)("C++를 사용한 데스크톱 개발") + WebView2 (Windows 11은 기본 포함)
- Linux: `libwebkit2gtk-4.1-dev libxdo-dev libayatana-appindicator3-dev librsvg2-dev`

자세한 내용: [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

```bash
pnpm install
pnpm tauri dev      # 앱 실행 (개발 모드)
pnpm lint           # Biome 린트·포맷 검사
pnpm build          # 타입 검사 + 프론트엔드 빌드
pnpm tauri build    # 설치 파일 빌드
```

Rust 검사 (`src-tauri/`):

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Claude Code mod 검사 (`claude-mod/branch-notes/`):

```bash
claude plugin validate .
claude plugin test .
```

Windows 설치 파일은 GitHub Actions(`.github/workflows/ci.yml`)가 빌드해서 아티팩트로 올린다.

## 폴더 구조

- `src/`: React 19 + TypeScript 프론트엔드 (`workspace/` 메인 창, `sidebar/` 사이드바, `features/` 공용 패널)
- `src-tauri/`: Rust 백엔드 (Tauri 2). `catalog/`에 문서 작성기 블록
- `claude-mod/`: Claude Code mod (앱과 `<앱 데이터>/bridge` 파일로 주고받음)
- `firebase/`: 메모 동기화 보안 규칙
- `docs/`: 계획·조사·설정 가이드·테스트 체크리스트

## 개인정보 · 보안

- Claude 로그인 정보(`~/.claude/.credentials.json` 등)는 읽거나 복사하지 않는다. Claude Code 관련 작업은 사용자가 설치한 `claude` 명령을 그대로 실행한다.
- GitHub 토큰은 OS 자격 증명 저장소(Windows 자격 증명 관리자)에 보관하고, git에는 명령 한 번에만 환경 변수로 넘긴다.

## 라이선스

[Apache-2.0](LICENSE). 번들한 서드파티 블록의 라이선스는 [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).

이 프로젝트는 Anthropic과 관련이 없으며, Anthropic의 후원이나 보증을 받지 않는다. Claude는 Anthropic, PBC의 상표다.
