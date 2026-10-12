---
kind: instruction
title: 기존 코드 스타일 따르기
title_en: Match the existing code style
summary: 주변 코드와 같은 스타일로 써요
summary_en: Writes code in the surrounding style
detail: 새 코드를 쓸 때 근처 파일의 이름 짓는 방식, 구조, 주석 양, 쓰는 라이브러리를 그대로 따라요. 새 패턴이나 의존성을 마음대로 들여오지 않아서, 리뷰할 때 '이건 누가 짠 거지?' 싶은 코드가 줄어요.
detail_en: New code follows nearby files' naming, structure, comment density and libraries. It doesn't bring in new patterns or dependencies on its own, so changes read like the rest of the codebase.
category: 코드 품질
category_en: Code quality
---
## Code style

- Match the surrounding code: naming, file layout, error handling, and comment density.
- Prefer existing helpers and libraries in the repo over adding new dependencies.
- Comments explain why, not what. Do not leave commented-out code.
- Keep functions focused; extract a helper only when it is reused or clearly clarifies intent.
