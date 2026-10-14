---
kind: skill
title: 인수인계 문서 (/handoff)
title_en: Handoff note (/handoff)
summary: 대화를 다음 세션용 인수인계 문서로 압축해요
summary_en: Compacts the chat into a handoff note
detail: /handoff로 직접 불러 써요. 대화가 길어졌거나 다른 세션·사람에게 넘길 때, 지금까지의 목표·결정·진행 상황·다음 할 일을 짧은 문서로 정리해요.
detail_en: Run it with /handoff. When a conversation gets long or moves to another session or person, it compacts goals, decisions, progress and next steps into a short note.
category: 작업 방식
category_en: Workflow
name: handoff
description: "Compact the current conversation into a handoff document for another agent to pick up."
disable-model-invocation: true
license: MIT
copyright: "Copyright (c) 2026 Matt Pocock"
source_repo: mattpocock/skills
source_commit: 49dd158d1076134a641b33efb035946536778336
source_path: skills/productivity/handoff/SKILL.md
license_path: LICENSE
tokens: 0
performance: 0
quality: 0
---
Write a handoff document summarising the current conversation so a fresh agent can continue the work. Save to the temporary directory of the user's OS (`$TMPDIR`, else `/tmp`; `%TEMP%` on Windows) - not the current workspace.

Include a "suggested skills" section in the document, naming which skills the next agent should call the Skill tool for.

Do not duplicate content already captured in other artifacts (specs, plans, ADRs, issues, commits, diffs). Reference them by path or URL instead.

Redact any sensitive information, such as API keys, passwords, or personally identifiable information.

If the user passed arguments, treat them as a description of what the next session will focus on and tailor the doc accordingly.
