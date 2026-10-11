---
kind: instruction
title: Git 커밋 규칙
title_en: Git commit rules
summary: 작고 의미 있는 커밋, 명확한 메시지, 히스토리 강제 변경 금지.
summary_en: Small, meaningful commits with clear messages; never rewrite shared history.
category: Git
category_en: Git
---
## Git

- Make small, focused commits. Each commit should build and pass checks.
- Commit message: imperative subject line under 72 characters, then a blank line and the reason for the change.
- Never force-push, rebase shared branches, or amend pushed commits unless asked.
- Do not commit generated files, secrets, or local config.
