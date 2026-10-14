---
kind: instruction
title: UI 변경 후 화면 확인
title_en: Visual check after UI changes
summary: 프런트엔드를 고치면 바로 화면을 열어 확인해요
summary_en: Checks the live page after each front-end change
detail: 프런트엔드를 고친 직후 바뀐 페이지를 Playwright로 열어 디자인 원칙·스타일 가이드(있으면)와 비교하고, 데스크톱 화면 캡처와 콘솔 오류 확인까지 해요. 큰 UI 작업이나 PR 마무리 전에는 디자인 리뷰어 subagent를 부르게 해요. Playwright MCP 서버가 있어야 해요.
detail_en: Right after a front-end change, Claude opens the affected pages with Playwright, compares them with the project's design principles and style guide (when present), takes a desktop screenshot and checks the console. For big UI work or before finishing a PR it calls the design reviewer subagent. Needs the Playwright MCP server.
category: 디자인
category_en: Design
name: design-review-check
description: "Quick visual check after front-end changes and when to invoke the design-review subagent."
pinned: true
license: MIT
copyright: "Copyright (c) 2025 Patrick Ellis"
source_repo: OneRedOak/claude-code-workflows
source_commit: 29a34c725313c7621f426f838ed31cf482473fb4
source_path: design-review/design-review-claude-md-snippet.md
license_path: LICENSE
tokens: 2
performance: 1
quality: 1
---
## Visual Development

### Design Principles
- Comprehensive design checklist in `/context/design-principles.md` (if the project has one)
- Brand style guide in `/context/style-guide.md` (if the project has one)
- When making visual (front-end, UI/UX) changes, always refer to these files for guidance

### Quick Visual Check
IMMEDIATELY after implementing any front-end change:
1. **Identify what changed** - Review the modified components/pages
2. **Navigate to affected pages** - Use `mcp__playwright__browser_navigate` to visit each changed view
3. **Verify design compliance** - Compare against `/context/design-principles.md` and `/context/style-guide.md` when present
4. **Validate feature implementation** - Ensure the change fulfills the user's specific request
5. **Check acceptance criteria** - Review any provided context files or requirements
6. **Capture evidence** - Take full page screenshot at desktop viewport (1440px) of each changed view
7. **Check for errors** - Run `mcp__playwright__browser_console_messages`

This verification ensures changes meet design standards and user requirements.

### Comprehensive Design Review
Invoke the `@agent-design-review` subagent for thorough design validation when:
- Completing significant UI/UX features
- Before finalizing PRs with visual changes
- Needing comprehensive accessibility and responsiveness testing
