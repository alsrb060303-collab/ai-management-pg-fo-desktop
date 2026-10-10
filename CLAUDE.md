# CLAUDE.md

Tauri 2 desktop sidebar app that docks next to the user's terminal and manages Claude Code docs, git work, and per-branch memos. Windows first, Linux later.

- Plan, decisions, roadmap: `docs/PLAN.md` (read before starting a phase)
- Researched facts with sources (Claude Code config, Win32 docking, sync, GitHub): `docs/research.md`

## Layout

- `src/`: React 19 + TypeScript frontend (Vite, Tailwind CSS v4, Zustand)
  - `components/`: shared UI, `store/`: Zustand stores
- `src-tauri/`: Rust backend (Tauri 2.12, Rust >= 1.90)
  - `capabilities/default.json`: every frontend-callable window/plugin API must be allowed here
- `.github/workflows/ci.yml`: lint/build on Linux, clippy + NSIS installer on Windows

## Commands

```bash
pnpm lint            # Biome (lint + format check); `pnpm format` to fix
pnpm build           # tsc + vite build
pnpm tauri dev       # run the app
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Run `pnpm lint`, `pnpm build`, and the cargo checks before committing.

## Rules

- Windows-only code goes behind `#[cfg(windows)]` with a fallback for other OSes. The Linux container cannot compile it, so CI's Windows job is the check.
- Never read, copy, or store Claude credentials (`~/.claude/.credentials.json`, tokens). Run the user's own unmodified `claude` binary only.
- When writing Claude Code settings files (e.g. `.claude/settings.local.json`), read-merge-write and keep unknown keys.
- Do not use "Claude" or "Anthropic" in the product name or branding.
- UI strings are Korean for now; English i18n comes before the public release.
