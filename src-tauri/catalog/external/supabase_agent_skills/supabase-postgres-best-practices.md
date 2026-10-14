---
kind: skill
title: Postgres 모범 사례
title_en: Postgres best practices
summary: Postgres 스키마·쿼리·RLS를 규칙대로 작성해요
summary_en: Writes Postgres schemas, queries and RLS by the rules
detail: Postgres 테이블·마이그레이션·인덱스·RLS 정책을 만들거나 느린 쿼리, 연결 고갈, 잠금 문제를 진단할 때 쓰여요. 쿼리 성능, 연결 관리, 보안, 스키마 설계 등 8개 분야의 규칙 파일(references/)을 참고해 잘못된 예와 올바른 예대로 SQL을 써요.
detail_en: Used when creating Postgres tables, migrations, indexes or RLS policies, or diagnosing slow queries, connection exhaustion and locking: follows bundled rule files (references/) across 8 areas, each with incorrect and correct SQL.
category: 데이터베이스
category_en: Database
name: supabase-postgres-best-practices
description: "Postgres best practices maintained by Supabase, for Postgres running anywhere. Load this skill BEFORE writing or changing anything that lives in a Postgres database: creating or altering tables and columns (including choosing column types), schema design, migrations and declarative schema files, RLS policies and the tests that verify them, indexes, triggers, database functions, queues and scheduled jobs (pg_cron, pgmq), vector/semantic search (pgvector), and restoring dumps (pg_restore) or importing data. Also load it when diagnosing slow queries, high CPU, timeouts, EXPLAIN plans, connection exhaustion, locking, bloat, or rows visible to the wrong user or tenant. This is not just a performance guide — schema, migration, security, and SQL authoring tasks need these rules too, even for a one-column change or a single query."
pinned: true
license: MIT
copyright: "Copyright (c) 2026 Supabase"
source_repo: supabase/agent-skills
source_commit: c9be0e931b7930f7d02126d04774d904c381e7d7
source_path: skills/supabase-postgres-best-practices/SKILL.md
license_path: LICENSE
tokens: 1
performance: 2
quality: 2
---
# Supabase Postgres Best Practices

Comprehensive performance optimization guide for Postgres, maintained by Supabase. Contains rules across 8 categories, prioritized by impact to guide automated query optimization and schema design.

## When to Apply

Reference these guidelines when:
- Writing SQL queries or designing schemas
- Implementing indexes or query optimization
- Reviewing database performance issues
- Configuring connection pooling or scaling
- Optimizing for Postgres-specific features
- Working with Row-Level Security (RLS)

## Rule Categories by Priority

| Priority | Category | Impact | Prefix |
|----------|----------|--------|--------|
| 1 | Query Performance | CRITICAL | `query-` |
| 2 | Connection Management | CRITICAL | `conn-` |
| 3 | Security & RLS | CRITICAL | `security-` |
| 4 | Schema Design | HIGH | `schema-` |
| 5 | Concurrency & Locking | MEDIUM-HIGH | `lock-` |
| 6 | Data Access Patterns | MEDIUM | `data-` |
| 7 | Monitoring & Diagnostics | LOW-MEDIUM | `monitor-` |
| 8 | Advanced Features | LOW | `advanced-` |

## How to Use

Read individual rule files for detailed explanations and SQL examples:

```
references/query-missing-indexes.md
references/query-partial-indexes.md
references/_sections.md
```

Each rule file contains:
- Brief explanation of why it matters
- Incorrect SQL example with explanation
- Correct SQL example with explanation
- Optional EXPLAIN output or metrics
- Additional context and references
- Supabase-specific notes (when applicable)

## References

- https://www.postgresql.org/docs/current/
- https://supabase.com/docs
- https://wiki.postgresql.org/wiki/Performance_Optimization
- https://supabase.com/docs/guides/database/overview
- https://supabase.com/docs/guides/auth/row-level-security
