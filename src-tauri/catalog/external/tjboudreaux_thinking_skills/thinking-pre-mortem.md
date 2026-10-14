---
kind: skill
title: 사전 부검(프리모템)
title_en: Pre-mortem
summary: 이미 실패했다고 가정하고 원인을 거꾸로 찾아요
summary_en: Assumes the plan failed and works back to causes
detail: 계획이나 출시 전에 직접 불러 쓰는 스킬이에요. 이미 실패했다고 과거형으로 가정한 뒤 구체적인 실패 원인을 모으고, 상위 위험마다 담당·확인 시점·통과 조건이 있는 예방책으로 바꿔 계획에 반영해요.
detail_en: A skill you call yourself before a plan or launch: Claude assumes it already failed, lists concrete causes, and turns each top risk into a prevention step with an owner, a checkpoint and a gate added to the plan.
category: 기획
category_en: Planning
name: thinking-pre-mortem
description: "Before committing to a plan or launch, assume it already failed and reason backward through concrete causes — convert failure paths into mitigations, gates, and stop checks."
disable-model-invocation: true
pinned: true
license: MIT
copyright: "Copyright (c) 2025 TJ Boudreaux"
source_repo: tjboudreaux/cc-thinking-skills
source_commit: 7b8fece345dfaa11773be7152ccd194589cb5437
source_path: skills/thinking-pre-mortem/SKILL.md
license_path: LICENSE
tokens: 1
performance: 0
quality: 0
---
# Pre-Mortem Analysis

**Core rule:** Prospective hindsight beats "what could go wrong?" Assume the plan has already failed (past tense), generate concrete failure paths, reverse them into prevention requirements, and bind each top path to a verifiable plan change.

## When to Use

- Before kickoff, major technical commitment, high-risk sprint work, launch, or release.
- After a plan looks solid but before execution, especially when optimism or overconfidence is likely.
- When a decision is costly to reverse and risks are still implicit rather than enforced.

## When NOT to Use

- Work is small, local, and cheaply reversible — skip the ceremony.
- You are mid-incident under time pressure — act now (OODA / scientific method); pre-mortem is pre-execution.
- You would only emit generic risks ("scope creep", "requirements unclear") that do not bind to this plan.
- Risks are already enforced by live gates (CI, canary, rollback, alerts) with no residual unlisted path.
- A mature risk register already covers the same specific failure paths with owners and checks.

## Procedure

1. **Set the failure frame in past tense:** "It is [post-deadline date]. The plan failed: [rollback / data loss / no adoption / SLA breach]." Explaining a happened failure is required; predicting one is not enough.
2. **Generate failure reasons before filtering:** sweep technical, process, assumptions, dependencies/external, and people. Force a second pass: "What did the plan most need to be true that was not?" Collect breadth first (aim for many distinct reasons); do not rank mid-sweep.
3. **Prioritize:** group by theme; score likelihood × impact; keep the top 3–5 concrete risks (drop generics that do not bind).
4. **Failure-first reverse analysis (absorbed reverse path):** for each top risk, (a) restate the failed outcome in past tense, (b) list **necessary/enabling conditions** that had to be true for that failure, (c) **invert** each condition into a verifiable prevention requirement (avoid/require rule + check), (d) bind **owner, verification checkpoint, and ship/stage gate**.
5. **Mitigate and update the plan:** for each top risk, attach mitigation steps, spike/contingency if needed, and the reverse-analysis prevention requirements as explicit plan gates. A pre-mortem with no plan change is incomplete.
6. **Stop when** top risks each have a bound mitigation or prevention gate, or when further reasons are only generic restatements — then ship the updated plan, do not keep brainstorming.

## Output

1. **Failure scenario** — past-tense disaster frame with date and failed outcome.
2. **Failure reasons** — unfiltered list, then prioritized top 3–5 with likelihood × impact.
3. **Reverse paths** — for each top risk: necessary conditions → inverted prevention requirements.
4. **Mitigations** — owner, verification checkpoint, ship/stage gate per top risk.
5. **Plan deltas** — concrete tasks, spikes, contingencies, and gates added to the plan.

## Verification

- **Falsify/stop:** if no plan-specific failure path can be stated in past tense with necessary conditions, stop — you are generating theater, not risk. If a "mitigation" has no verification checkpoint, it is not done.
- **Over-application guard:** do not pre-mortem reversible chores, mid-incident firefighting, or systems that already enforce the same paths. Do not leave abstract risks as "mitigate later"; either invert them into gates or drop them as non-actionable.
