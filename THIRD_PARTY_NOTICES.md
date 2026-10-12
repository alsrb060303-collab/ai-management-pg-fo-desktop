# Third-party notices

The document composer bundles blocks adapted from these projects. Each block
records its source repository, pinned commit, file path, license and copyright,
and files the app writes from a block keep that attribution (SKILL.md
`license`/`metadata` fields, a `# Source:` comment in other frontmatter, or an
HTML comment in CLAUDE.md sections).

| Project | License | Copyright | Commit | Blocks |
|---|---|---|---|---|
| [anthropics/skills](https://github.com/anthropics/skills) | Apache-2.0 | Anthropic, PBC | `dbd4588f9e1033efb41dad4bef2f7947c8993d44` | frontend-design |
| [obra/superpowers](https://github.com/obra/superpowers) | MIT | (c) 2025 Jesse Vincent | `bb92a77741419a4ab5f06e711a283343f1ada0c3` | dispatching-parallel-agents, finishing-a-development-branch, receiving-code-review, using-git-worktrees, verification-before-completion, test-driven-development, writing-plans, requesting-code-review, systematic-debugging |
| [wshobson/agents](https://github.com/wshobson/agents) | MIT | (c) 2024 Seth Hobson | `46891e7e60da0e52baf1050b7b6391b64e84c6d9` | python-pro, typescript-pro, rust-pro, golang-pro, frontend-developer, security-auditor, performance-engineer |
| [mattpocock/skills](https://github.com/mattpocock/skills) | MIT | (c) 2026 Matt Pocock | `49dd158d1076134a641b33efb035946536778336` | grill-me, to-spec, to-tickets, handoff, domain-modeling, research, retro, writing-for-agents, triage, improve-codebase-architecture, teach |
| [VoltAgent/awesome-claude-code-subagents](https://github.com/VoltAgent/awesome-claude-code-subagents) | MIT | (c) 2025 VoltAgent | `408d3c9eee2d0c75c12a3abb5de29e53c2ca6b9e` | test-automator, architect-reviewer, accessibility-tester, api-designer, sql-pro, refactoring-specialist, technical-writer, documentation-engineer, devops-engineer, java-architect, kotlin-specialist, swift-expert, csharp-developer, php-pro |
| [PatrickJS/awesome-cursorrules](https://github.com/PatrickJS/awesome-cursorrules) | CC0-1.0 | PatrickJS and contributors (public domain dedication) | `b044f956f021b6e8877f16781bcfc466a6a120e9` | Next.js App Router, FastAPI, Docker, Flutter Riverpod, Spring Boot JPA, Kotlin Ktor, Laravel package, HTMX + Django rules |

Full license texts: `src-tauri/catalog/external/<source>/LICENSE`.

Changes from the originals: frontmatter is replaced with the app's own fields
(Korean title and summary, provenance), subagent names are shortened (for
example `security-scanning-security-auditor` → `security-auditor`), and
subagent `model` settings are dropped so they use the session's model. Bodies
are unchanged, except:

- VoltAgent subagents: the "Communication Protocol" section, JSON status
  payloads, "query context manager" steps, delivery notifications and lists of
  other agents are removed (they describe VoltAgent's own multi-agent setup,
  which Claude Code doesn't have), and the steps are renumbered.
- mattpocock/skills `to-spec` and `to-tickets`: the pointer to
  `/setup-matt-pocock-skills` is replaced by asking the user where to publish.
  In `triage` the same pointer is replaced by asking the user which labels
  their issue tracker uses.
- Cursor rules: converted to Claude Code rules (`paths` set per language
  instead of `**/*`), and "save this file as .cursorrules" notes are removed.
- Skills whose folder has companion Markdown files (test-driven-development,
  requesting-code-review, systematic-debugging, domain-modeling, triage,
  improve-codebase-architecture, writing-for-agents, teach) carry those
  files appended to the SKILL.md body; scripts and test fixtures are not
  included.

Only the Markdown of the listed files is included: no scripts, hooks, MCP
configuration or other files from these projects.

Fonts (SIL Open Font License 1.1): Pretendard, Source Serif 4, JetBrains Mono.
Icons: Lucide (ISC). Graph editor: React Flow (MIT).
