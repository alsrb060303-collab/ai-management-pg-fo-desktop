# Third-party notices

The document composer bundles blocks adapted from these projects. Each block
records its source repository, pinned commit, file path, license and copyright,
and files the app writes from a block keep that attribution (SKILL.md
`license`/`metadata` fields, a `# Source:` comment in other frontmatter, or an
HTML comment in CLAUDE.md sections).

| Project | License | Copyright | Commit | Blocks |
|---|---|---|---|---|
| [anthropics/skills](https://github.com/anthropics/skills) | Apache-2.0 | Anthropic, PBC | `dbd4588f9e1033efb41dad4bef2f7947c8993d44` | frontend-design |
| [obra/superpowers](https://github.com/obra/superpowers) | MIT | (c) 2025 Jesse Vincent | `bb92a77741419a4ab5f06e711a283343f1ada0c3` | dispatching-parallel-agents, finishing-a-development-branch, receiving-code-review, using-git-worktrees, verification-before-completion, test-driven-development, writing-plans, requesting-code-review, systematic-debugging, writing-skills |
| [wshobson/agents](https://github.com/wshobson/agents) | MIT | (c) 2024 Seth Hobson | `46891e7e60da0e52baf1050b7b6391b64e84c6d9` | python-pro, typescript-pro, rust-pro, golang-pro, frontend-developer, security-auditor, performance-engineer |
| [mattpocock/skills](https://github.com/mattpocock/skills) | MIT | (c) 2026 Matt Pocock | `49dd158d1076134a641b33efb035946536778336` | grill-me, to-spec, to-tickets, handoff, domain-modeling, research, retro, writing-for-agents, triage, improve-codebase-architecture, teach |
| [VoltAgent/awesome-claude-code-subagents](https://github.com/VoltAgent/awesome-claude-code-subagents) | MIT | (c) 2025 VoltAgent | `408d3c9eee2d0c75c12a3abb5de29e53c2ca6b9e` | test-automator, architect-reviewer, accessibility-tester, api-designer, sql-pro, refactoring-specialist, technical-writer, documentation-engineer, devops-engineer, java-architect, kotlin-specialist, swift-expert, csharp-developer, php-pro |
| [addyosmani/agent-skills](https://github.com/addyosmani/agent-skills) | MIT | (c) 2025 Addy Osmani | `1be8e34187e34647bb83adc3a1323b26ae6f6abe` | source-driven-development, incremental-implementation, spec-driven-development, context-engineering, api-and-interface-design |
| [affaan-m/everything-claude-code](https://github.com/affaan-m/everything-claude-code) | MIT | (c) 2026 Affaan Mustafa | `b92f181b3b748d1abbc2afd94af6aa7d8c8fca29` | api-design, error-handling, security-checklist, architecture-decision-records, e2e-testing |
| [revfactory/harness](https://github.com/revfactory/harness) | Apache-2.0 | 2025 robin | `92d9f1b29e7af81acebf51a9fcd1832e739a1414` | harness, harness-evolve |
| [microsoft/skills](https://github.com/microsoft/skills) | MIT | (c) Microsoft Corporation | `91ec09549ca16bd6c44e6ccdef26d61eb2082f07` | zustand-store-ts |
| [supabase/agent-skills](https://github.com/supabase/agent-skills) | MIT | (c) 2026 Supabase | `c9be0e931b7930f7d02126d04774d904c381e7d7` | supabase-postgres-best-practices |
| [tjboudreaux/cc-thinking-skills](https://github.com/tjboudreaux/cc-thinking-skills) | MIT | (c) 2025 TJ Boudreaux | `7b8fece345dfaa11773be7152ccd194589cb5437` | thinking-pre-mortem, thinking-reversibility |
| [mhattingpete/claude-skills-marketplace](https://github.com/mhattingpete/claude-skills-marketplace) | Apache-2.0 | 2025 mhattingpete | `b5b34bcf4c920bb72cee1c391b54a33cb5353c12` | test-fixing |
| [JuliusBrussee/caveman](https://github.com/JuliusBrussee/caveman) | Apache-2.0 (earlier parts MIT, see its NOTICE) | 2026 Julius Brussee | `2e08b9177c07bb7249a8a2d1a6758e5db281d002` | caveman |
| [OneRedOak/claude-code-workflows](https://github.com/OneRedOak/claude-code-workflows) | MIT | (c) 2025 Patrick Ellis | `29a34c725313c7621f426f838ed31cf482473fb4` | design-review (subagent), design-review-check (instruction) |
| [ibelick/ui-skills](https://github.com/ibelick/ui-skills) | MIT | (c) 2026 Julien Thibeaut | `f2264c30213a8a55bd0a6a7668d0438e4564e090` | baseline-ui, fixing-accessibility |
| [coreyhaines31/marketingskills](https://github.com/coreyhaines31/marketingskills) | MIT | (c) 2025 Corey Haines | `1efedbc5148b54b2f0f6c6c9fe0be62e151c7fff` | copywriting, copy-editing, cro, seo-audit |
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

- Skills whose originals keep reference files next to SKILL.md (harness,
  writing-skills, security-checklist, supabase-postgres-best-practices and
  the four marketing skills) bundle those Markdown files unchanged under
  `src-tauri/catalog/external/<source>/<skill>/`, written next to the
  skill's SKILL.md at the same relative paths.
- addyosmani/agent-skills: sentences pointing to repo skills or files that
  aren't bundled are removed (`security-and-hardening`,
  `git-workflow-and-versioning`, `planning-and-task-breakdown`,
  `deprecation-and-migration`, `/build`, `references/definition-of-done.md`);
  in spec-driven-development the `skills/<x>/SKILL.md` paths become skill names.
- everything-claude-code: `security-review` is named `security-checklist`
  (Claude Code has a built-in `/security-review`) and gets one line linking
  its bundled `cloud-infrastructure-security.md`; architecture-decision-records
  drops its "Integration with Other Skills" section.
- revfactory/harness: `evolve` is named `harness-evolve` and the body's
  `harness:evolve` mentions follow. Its references `skill-writing-guide.md`
  and `skill-testing-guide.md` appear to be adapted from the skill-creator
  skill of [anthropics/skills](https://github.com/anthropics/skills)
  (Apache-2.0, Anthropic, PBC; checked at `dbd4588f9e1033efb41dad4bef2f7947c8993d44`),
  which the harness repo does not credit.
- obra/superpowers writing-skills: `anthropic-best-practices.md` (a copy of
  Anthropic's documentation), `render-graphs.js` and `graphviz-conventions.dot`
  are not included, and the sentences linking them and other harnesses'
  tool notes are removed.
- microsoft/skills zustand-store-ts: `assets/template.ts` is inlined into the body.
- caveman: the `/caveman ultra`/`wenyan` aliases, hook-relay instructions and
  the `/caveman-compress` exception are removed (they need the repo's hooks
  and other skills). Its NOTICE and LICENSE-MIT are kept with the LICENSE.
- OneRedOak design-review: the fixed `model` and `color` are dropped and the
  description's inner double quotes become single quotes. The CLAUDE.md
  snippet (design-review-check) gets a description and "(if the project has
  one)" / "when present" for the project's own design documents.
- Marketing skills: pointers to the repo's other skills and a GitHub link to
  its revops playbook are removed. Four reference files are not included
  because they are derived from sources under other terms: `ai-tells.md`
  (Wikipedia, CC BY-SA), `natural-transitions.md` (Manchester Academic
  Phrasebank), `plain-english-alternatives.md` (Plain English Campaign) and
  `ai-writing-detection.md`; the lines linking them are removed or reworded.

Only the Markdown of the listed files is included: no scripts, hooks, MCP
configuration or other files from these projects.

Fonts (SIL Open Font License 1.1): Pretendard, Source Serif 4, JetBrains Mono.
Icons: Lucide (ISC). Google and GitHub marks on the sign-in buttons: Simple
Icons 16.35.0 (CC0-1.0, unchanged paths drawn in the text color); the marks
are trademarks of Google LLC and GitHub, Inc., used only to label signing in
with those services. Graph editor: React Flow (MIT).
