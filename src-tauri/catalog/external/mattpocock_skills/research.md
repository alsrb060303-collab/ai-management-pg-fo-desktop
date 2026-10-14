---
kind: skill
title: 자료 조사 맡기기
title_en: Research from primary sources
summary: 공식 문서·소스 같은 1차 자료로 조사해 파일로 남겨요
summary_en: Researches primary sources and saves the findings
detail: 주제 조사나 문서·API 사실 확인을 맡길 때 쓰여요. 백그라운드 에이전트가 공식 문서·소스 코드·스펙 같은 1차 자료만 따라가 조사하고, 출처를 단 Markdown 파일로 저장소에 남겨요.
detail_en: Used when you want a topic researched or API facts gathered: a background agent follows official docs, source and specs, and saves a Markdown file citing each claim.
category: 조사
category_en: Research
name: research
description: "Investigate a question against high-trust primary sources and capture the findings as a Markdown file in the repo. Use when the user wants a topic researched, docs or API facts gathered, or reading legwork delegated to a background agent."
license: MIT
copyright: "Copyright (c) 2026 Matt Pocock"
source_repo: mattpocock/skills
source_commit: 49dd158d1076134a641b33efb035946536778336
source_path: skills/engineering/research/SKILL.md
license_path: LICENSE
tokens: 1
performance: 1
quality: 1
---
Spin up a **background agent** to do the research, so you keep working while it reads.

Its job:

1. Investigate the question against **primary sources** (official docs, source code, specs, first-party APIs), not a secondary write-up of them. Follow every claim back to the source that owns it.
2. Write the findings to a single Markdown file, citing each claim's source.
3. Save it where the repo already keeps such notes; match the existing convention, and if there is none, put it somewhere sensible and say where.
