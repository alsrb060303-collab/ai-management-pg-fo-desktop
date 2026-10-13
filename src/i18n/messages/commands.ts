import type { Messages } from "./types";

export const ko = {
  "commands.title": "명령어",
  "commands.description":
    "이 프로젝트의 Claude Code에서 /로 부를 수 있는 skill과 명령이에요. 누르면 복사돼요.",
  "commands.codex.title": "Codex에는 따로 보여줄 명령어가 없어요",
  "commands.codex.body":
    "Codex의 사용자 명령(~/.codex/prompts)은 더 이상 권장되지 않고 skills가 그 자리를 대신해요. 이 프로젝트의 skills는 설정 지도의 Skills 섹션에서 볼 수 있어요.",
  "commands.search": "명령어 검색",
  "commands.noProject": "명령어를 볼 프로젝트 폴더를 고르세요.",
  "commands.empty": "부를 수 있는 명령어가 없어요.",
  "commands.noMatch": "찾는 명령어가 없어요.",
  "commands.scope.managed": "관리 정책",
  "commands.scope.user": "개인 (모든 프로젝트)",
  "commands.scope.parent": "상위 폴더",
  "commands.scope.project": "이 프로젝트",
  "commands.scope.local": "로컬",
  "commands.manual": "직접 실행",
  "commands.manualHint": "Claude가 스스로 쓰지 않고, 직접 /로 불러야 실행돼요",
  "commands.command": "명령",
  "commands.copy": "복사",
  "commands.copied": "복사됨",
  "commands.refresh": "다시 읽기",
  "commands.plugins": "플러그인이 추가한 명령어는 Claude Code의 / 메뉴에서 확인하세요.",
};

export const en: Messages<typeof ko> = {
  "commands.title": "Commands",
  "commands.description":
    "Skills and commands you can call with / in this project's Claude Code. Click to copy.",
  "commands.codex.title": "Codex has no commands to list here",
  "commands.codex.body":
    "Codex's own user commands (~/.codex/prompts) are deprecated; skills replace them. See this project's skills in the Skills section of the config map.",
  "commands.search": "Search commands",
  "commands.noProject": "Pick a project folder to see its commands.",
  "commands.empty": "No commands to call.",
  "commands.noMatch": "No matching commands.",
  "commands.scope.managed": "Managed policy",
  "commands.scope.user": "Personal (all projects)",
  "commands.scope.parent": "Parent folder",
  "commands.scope.project": "This project",
  "commands.scope.local": "Local",
  "commands.manual": "Manual",
  "commands.manualHint": "Claude doesn't use it on its own; it runs only when you call it with /",
  "commands.command": "Command",
  "commands.copy": "Copy",
  "commands.copied": "Copied",
  "commands.refresh": "Reload",
  "commands.plugins": "Commands added by plugins are in Claude Code's / menu.",
};
