---
kind: skill
title: 커밋 메시지 작성
title_en: Write commit messages
summary: 스테이징한 변경을 읽고 커밋 메시지를 써줘요
summary_en: Writes a commit message from staged changes
detail: 필요할 때 불러 쓰는 skill이에요. `git diff --staged`를 읽고 변경 목적을 파악해서, 72자 이하 제목과 이유를 담은 본문으로 커밋 메시지를 써요. 목적이 여러 개 섞였으면 커밋을 나누라고 제안해요.
detail_en: A skill Claude uses when you ask for a commit message: reads `git diff --staged`, finds the main purpose, and writes a subject under 72 characters plus a body explaining why. Suggests splitting unrelated changes.
category: Git
category_en: Git
name: commit-message
description: Write a clear git commit message for the currently staged changes. Use when the user asks to commit or to draft a commit message.
tokens: 0
performance: 0
quality: 1
---
# Commit message

1. Run `git diff --staged` to read the staged changes. If nothing is staged, say so and stop.
2. Identify the single main purpose of the change. If the changes have unrelated purposes, suggest splitting them into separate commits.
3. Write the message:
   - Subject: imperative mood, at most 72 characters, no trailing period.
   - Blank line, then a short body explaining why the change was made and any notable side effects.
4. Show the message to the user before committing, unless they asked you to commit directly.
