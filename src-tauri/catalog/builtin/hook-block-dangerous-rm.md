---
kind: hook
title: 위험한 rm -rf 막기
title_en: Block dangerous rm -rf
summary: 루트나 홈 폴더를 지우는 rm -rf 명령을 실행 전에 막아요
summary_en: Stops rm -rf on the root or home folder before it runs
detail: Bash 도구를 쓰기 직전(PreToolUse)에 명령을 검사해서 `rm -rf /`나 `rm -rf ~`이면 종료 코드 2로 막고 Claude에게 이유를 알려요. Node.js가 설치돼 있어야 동작해요. 모든 위험을 막아 주지는 않으니 권한 설정과 같이 쓰세요.
detail_en: Checks each Bash command right before it runs (PreToolUse) and blocks `rm -rf /` or `rm -rf ~` with exit code 2, telling Claude why. Needs Node.js. It doesn't catch everything, so keep using permission settings too.
category: 보안
category_en: Security
name: block-dangerous-rm
event: PreToolUse
matcher: Bash
tokens: 0
performance: 0
quality: 1
---
node -e "let s='';process.stdin.on('data',d=>s+=d).on('end',()=>{const c=(JSON.parse(s).tool_input||{}).command||'';if(/rm\s+-[a-z]*r[a-z]*f?\s+(\/|~)(\s|$)/.test(c)){console.error('Blocked: rm -rf on / or ~');process.exit(2)}})"
