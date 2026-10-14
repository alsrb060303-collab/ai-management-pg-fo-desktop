---
kind: skill
title: 실패한 테스트 고치기
title_en: Fix failing tests
summary: 실패한 테스트를 원인별로 묶어 차례로 고쳐요
summary_en: Groups failing tests by cause and fixes them in order
detail: 테스트가 실패할 때 전체를 돌려 실패를 오류 종류·파일·원인별로 묶고, 영향이 큰 묶음부터 인프라→API 변경→로직 순으로 고쳐요. 묶음마다 해당 테스트만 다시 돌려 확인한 뒤 마지막에 전체를 돌려요.
detail_en: When tests fail, Claude runs the suite, groups failures by error type, file and root cause, and fixes the biggest groups first (infrastructure, then API changes, then logic), rerunning each group before a final full run.
category: 테스트
category_en: Testing
name: test-fixing
description: "Run tests and systematically fix all failing tests using smart error grouping. Use when user asks to fix failing tests, mentions test failures, runs test suite and failures occur, or requests to make tests pass."
pinned: true
license: Apache-2.0
copyright: "Copyright 2025 mhattingpete"
source_repo: mhattingpete/claude-skills-marketplace
source_commit: b5b34bcf4c920bb72cee1c391b54a33cb5353c12
source_path: engineering-workflow-plugin/skills/test-fixing/SKILL.md
license_path: LICENSE
tokens: 1
performance: 1
quality: 1
---
# Test Fixing

Systematically identify and fix all failing tests using smart grouping strategies.

## When to Use

- Explicitly asks to fix tests ("fix these tests", "make tests pass")
- Reports test failures ("tests are failing", "test suite is broken")
- Completes implementation and wants tests passing
- Mentions CI/CD failures due to tests

## Systematic Approach

### 1. Initial Test Run

Run `make test` to identify all failing tests.

Analyze output for:
- Total number of failures
- Error types and patterns
- Affected modules/files

### 2. Smart Error Grouping

Group similar failures by:
- **Error type**: ImportError, AttributeError, AssertionError, etc.
- **Module/file**: Same file causing multiple test failures
- **Root cause**: Missing dependencies, API changes, refactoring impacts

Prioritize groups by:
- Number of affected tests (highest impact first)
- Dependency order (fix infrastructure before functionality)

### 3. Systematic Fixing Process

For each group (starting with highest impact):

1. **Identify root cause**
   - Read relevant code
   - Check recent changes with `git diff`
   - Understand the error pattern

2. **Implement fix**
   - Use Edit tool for code changes
   - Follow project conventions (see CLAUDE.md)
   - Make minimal, focused changes

3. **Verify fix**
   - Run subset of tests for this group
   - Use pytest markers or file patterns:
     ```bash
     uv run pytest tests/path/to/test_file.py -v
     uv run pytest -k "pattern" -v
     ```
   - Ensure group passes before moving on

4. **Move to next group**

### 4. Fix Order Strategy

**Infrastructure first:**
- Import errors
- Missing dependencies
- Configuration issues

**Then API changes:**
- Function signature changes
- Module reorganization
- Renamed variables/functions

**Finally, logic issues:**
- Assertion failures
- Business logic bugs
- Edge case handling

### 5. Final Verification

After all groups fixed:
- Run complete test suite: `make test`
- Verify no regressions
- Check test coverage remains intact

## Best Practices

- Fix one group at a time
- Run focused tests after each fix
- Use `git diff` to understand recent changes
- Look for patterns in failures
- Don't move to next group until current passes
- Keep changes minimal and focused

## Example Workflow

User: "The tests are failing after my refactor"

1. Run `make test` → 15 failures identified
2. Group errors:
   - 8 ImportErrors (module renamed)
   - 5 AttributeErrors (function signature changed)
   - 2 AssertionErrors (logic bugs)
3. Fix ImportErrors first → Run subset → Verify
4. Fix AttributeErrors → Run subset → Verify
5. Fix AssertionErrors → Run subset → Verify
6. Run full suite → All pass ✓
