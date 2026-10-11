---
kind: skill
title: 커밋 메시지 작성
summary: 스테이징된 변경을 읽고 규칙에 맞는 커밋 메시지를 써 주는 skill.
category: Git
name: commit-message
description: Write a clear git commit message for the currently staged changes. Use when the user asks to commit or to draft a commit message.
---
# Commit message

1. Run `git diff --staged` to read the staged changes. If nothing is staged, say so and stop.
2. Identify the single main purpose of the change. If the changes have unrelated purposes, suggest splitting them into separate commits.
3. Write the message:
   - Subject: imperative mood, at most 72 characters, no trailing period.
   - Blank line, then a short body explaining why the change was made and any notable side effects.
4. Show the message to the user before committing, unless they asked you to commit directly.
