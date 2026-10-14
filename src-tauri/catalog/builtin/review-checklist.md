---
kind: skill
title: 코드 리뷰 체크리스트
title_en: Code review checklist
summary: 정확성·보안·테스트 기준으로 변경을 검토해요
summary_en: Reviews changes for bugs, security and tests
detail: 리뷰를 부탁하면 쓰는 skill이에요. 변경 내용을 정확성 → 보안 → 동시성·자원 → 테스트 → 가독성 순서로 보고, 실제 문제만 파일·줄 번호와 함께 알려준 뒤 '병합 가능/막는 문제'로 결론을 내요.
detail_en: A skill for when you ask for a review: checks correctness, security, concurrency and resources, tests and readability in that order, reports only concrete problems with file and line, and ends with a merge-ready verdict.
category: 리뷰
category_en: Review
name: review-checklist
description: Review a diff or a set of changed files for correctness, security, and missing tests. Use when the user asks for a review before merging.
tokens: 0
performance: 1
quality: 1
---
# Review checklist

Review the changes (use `git diff` against the base branch if no diff is given). For each item, report only concrete problems with file and line:

1. Correctness: logic errors, unhandled edge cases (empty, null, large input), error paths.
2. Security: injection, secrets in code, missing authorization or validation.
3. Concurrency and resources: races, leaks, missing cleanup.
4. Tests: changed behavior without tests, tests that no longer test anything.
5. Readability: only when it hides a bug or will clearly confuse the next reader.

End with a one-line verdict: ready to merge, or the blocking issues.
