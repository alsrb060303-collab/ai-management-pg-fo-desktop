# ai-management (임시 이름)

AI 코딩 도구(우선 Claude Code)로 작업할 때 **터미널 옆에 붙어서** 문서·작업·메모를 관리하는 데스크탑 사이드바 앱.

> 🚧 초기 개발 중. 계획은 [`docs/PLAN.md`](docs/PLAN.md), 조사 자료는 [`docs/research.md`](docs/research.md)를 참고.

## 계획 중인 기능

- **문서**: CLAUDE.md · skill · subagent 생성, 어떤 설정이 어디에 적용되는지 보여주는 지도, 프로젝트별 적용·제외
- **작업**: 브랜치 · worktree · 변경 상태, git 조작, GitHub PR · 이슈
- **메모**: 브랜치별 메모/TODO, 여러 기기 동기화
- **사이드바**: Windows Terminal · VS Code · Claude 데스크탑 중 지금 쓰는 창 옆에 자동으로 붙음

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

Windows 설치 파일은 GitHub Actions(`.github/workflows/ci.yml`)가 빌드해서 아티팩트로 올린다.

## 라이선스

[Apache-2.0](LICENSE)

이 프로젝트는 Anthropic과 관련이 없으며, Anthropic의 후원이나 보증을 받지 않는다. Claude는 Anthropic, PBC의 상표다.
