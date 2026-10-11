---
kind: skill
title: 코드 리뷰 체크리스트
summary: 변경 사항을 정확성·보안·테스트 관점으로 점검하는 skill.
category: 리뷰
name: review-checklist
description: Review a diff or a set of changed files for correctness, security, and missing tests. Use when the user asks for a review before merging.
---
# Review checklist

Review the changes (use `git diff` against the base branch if no diff is given). For each item, report only concrete problems with file and line:

1. Correctness: logic errors, unhandled edge cases (empty, null, large input), error paths.
2. Security: injection, secrets in code, missing authorization or validation.
3. Concurrency and resources: races, leaks, missing cleanup.
4. Tests: changed behavior without tests, tests that no longer test anything.
5. Readability: only when it hides a bug or will clearly confuse the next reader.

End with a one-line verdict: ready to merge, or the blocking issues.
