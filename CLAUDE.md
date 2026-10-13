# CLAUDE.md

Tauri 2 desktop app (a main window plus a sidebar that docks next to the user's terminal; one is shown at a time) that manages AI coding tool docs (Claude Code, Codex), git work, and per-branch memos. Windows first, Linux later.

- Plan, decisions, roadmap: `docs/PLAN.md` (read before starting a phase)
- Researched facts with sources (Claude Code and Codex config, Win32 docking, sync, GitHub): `docs/research.md`

## Layout

- `src/`: React 19 + TypeScript frontend (Vite, Tailwind CSS v4, Zustand). `main.tsx` picks the UI by window label.
  - `workspace/`: main window, `sidebar/`: docked sidebar, `features/`: panels used by both
  - `components/ui.tsx`: shared controls, `store/`: Zustand stores
  - `index.css`: design tokens (light/dark). Use the semantic color names, not raw Tailwind colors.
- `src-tauri/`: Rust backend (Tauri 2.12, Rust >= 1.90). `shell.rs` switches main/sidebar windows.
  - `tools/`: one adapter per AI tool (`claude/`, `codex/`) plus `ToolId` file layout and project detection. `src/tools.ts` mirrors it; add a tool in both.
  - `capabilities/default.json`: every frontend-callable window/plugin API must be allowed here
- `claude-mod/branch-notes/`: the Claude Code mod (talks to the app through `<app data>/bridge`, see `src-tauri/src/bridge.rs`). Check with `claude plugin validate` and `claude plugin test` in that folder.
- `.github/workflows/ci.yml`: lint/build on Linux, clippy + NSIS installer on Windows
- `.github/workflows/release.yml`: each new version on main becomes a GitHub Release the app updates from (`docs/release-setup.md`)

## Commands

```bash
pnpm lint            # Biome (lint + format check); `pnpm format` to fix
pnpm build           # tsc + vite build
pnpm tauri dev       # run the app
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Run `pnpm lint`, `pnpm build`, and the cargo checks before committing.

Every change to the app (a PR that touches `src/` or `src-tauri/`) raises the version once: `pnpm bump`. Updates add a last digit 1-9 to the base the user set (0.1.4 → 0.1.41 → 0.1.42 … → 0.1.49), in package.json, Cargo.toml, Cargo.lock and tauri.conf.json. Docs-only changes don't. Change the base (the `m` or `x` in 0.m.x, e.g. 0.1.5 or 0.2.0) only when the user asks, with `pnpm bump <version>`; after 0.1.49 the script stops, so ask. The Claude Code mod in `claude-mod/` keeps its own version.

## Rules

- Windows-only code goes behind `#[cfg(windows)]` with a fallback for other OSes. The Linux container cannot compile it, so CI's Windows job is the check.
- Never read, copy, or store Claude credentials (`~/.claude/.credentials.json`, tokens). Run the user's own unmodified `claude` binary only.
- When writing Claude Code settings files (e.g. `.claude/settings.local.json`), read-merge-write and keep unknown keys.
- Do not use "Claude" or "Anthropic" in the product name or branding, and no Anthropic logos or proprietary fonts.
- Create windows only from async commands or `tauri::async_runtime::spawn` (sync handlers deadlock on Windows).
- UI strings are Korean for now; English i18n comes before the public release.

## Third-party content and licenses

Applies to everything taken from outside this repo: catalog blocks (instructions, rules, skills, subagents), recommended mods/plugins, npm/cargo dependencies, fonts, icons and other assets.

- Add it only if its license allows modifying, redistributing, distributing derivative works, and commercial use. OK: MIT, Apache-2.0, BSD, ISC, Zlib, Unlicense, CC0. Dependencies only: MPL-2.0 (unmodified), LGPL (dynamically linked or with a permissive alternative). Fonts: OFL-1.1 (bundled, not sold on their own).
- Exclude: no license, GPL/AGPL, CC BY-NC/ND/SA, "source-available" or custom terms. Ask the user when unsure.
- Check the license of the exact files you take, at a pinned commit: a folder can be licensed differently from its repo root (e.g. anthropics/skills).
- Also read the content and exclude ads, referral links, telemetry, remote scripts, and archived or unmaintained repos.
- Record provenance: catalog blocks keep `license`/`copyright`/`source_*` frontmatter, the LICENSE text goes in `src-tauri/catalog/external/<source>/`, and the project goes in `THIRD_PARTY_NOTICES.md`. Note any change from the original there.
- Recommended mods are installed by the user from their own source, not bundled, but get the same license and content check (`docs/research.md` §7).
- Never use another project's trademark (e.g. "Claude", "Anthropic") as the product's name or branding.

## Subagents

- Ask the user before running a workflow or spawning subagents.
- Web research subagents (WebSearch/WebFetch): use model `haiku`.
- Verification / fact-check subagents: use model `sonnet`.
