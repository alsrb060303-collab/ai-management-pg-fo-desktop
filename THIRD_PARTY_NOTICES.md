# Third-party notices

The document composer bundles blocks adapted from these projects. Each block
records its source repository, pinned commit, file path, license and copyright,
and files the app writes from a block keep that attribution (SKILL.md
`license`/`metadata` fields, a `# Source:` comment in other frontmatter, or an
HTML comment in CLAUDE.md sections).

| Project | License | Copyright | Commit | Blocks |
|---|---|---|---|---|
| [anthropics/skills](https://github.com/anthropics/skills) | Apache-2.0 | Anthropic, PBC | `dbd4588f9e1033efb41dad4bef2f7947c8993d44` | frontend-design |
| [obra/superpowers](https://github.com/obra/superpowers) | MIT | (c) 2025 Jesse Vincent | `bb92a77741419a4ab5f06e711a283343f1ada0c3` | dispatching-parallel-agents, finishing-a-development-branch, receiving-code-review, using-git-worktrees, verification-before-completion |
| [wshobson/agents](https://github.com/wshobson/agents) | MIT | (c) 2024 Seth Hobson | `46891e7e60da0e52baf1050b7b6391b64e84c6d9` | python-pro, typescript-pro, rust-pro, golang-pro, frontend-developer, security-auditor, performance-engineer |

Full license texts: `src-tauri/catalog/external/<source>/LICENSE`.

Changes from the originals: frontmatter is replaced with the app's own fields
(Korean title and summary, provenance), subagent names are shortened (for
example `security-scanning-security-auditor` → `security-auditor`), and
subagent `model` settings are dropped so they use the session's model. Bodies
are unchanged.

Only the Markdown of the listed files is included: no scripts, hooks, MCP
configuration or other files from these projects.

Fonts (SIL Open Font License 1.1): Pretendard, Source Serif 4, JetBrains Mono.
Icons: Lucide (ISC). Graph editor: React Flow (MIT).
