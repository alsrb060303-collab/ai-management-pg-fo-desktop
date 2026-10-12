---
kind: hook
title: 세션 시작 때 git 상태 알려 주기
title_en: Git status at session start
summary: 세션을 열면 현재 브랜치와 바뀐 파일을 Claude에게 알려요
summary_en: Tells Claude the branch and changed files when a session starts
detail: SessionStart hook의 출력은 Claude의 맥락에 들어가요. 세션을 시작하거나 이어 갈 때 `git status --short --branch`를 실행해서, Claude가 어느 브랜치에서 무엇이 바뀐 상태인지 알고 시작해요. 읽기만 하는 명령이라 안전해요.
detail_en: A SessionStart hook's output goes into Claude's context. Runs `git status --short --branch` when a session starts or resumes, so Claude knows the branch and what has changed. Read-only and safe.
category: Git
category_en: Git
name: session-git-status
event: SessionStart
---
git status --short --branch
