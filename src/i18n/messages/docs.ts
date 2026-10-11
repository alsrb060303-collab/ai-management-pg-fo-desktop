import type { Messages } from "./types";

export const ko = {
  // Views and pages
  "docs.view.map": "설정 지도",
  "docs.view.library": "라이브러리",
  "docs.mapPage.description":
    "Claude Code가 프로젝트에서 읽는 지침 · skill · subagent · 설정을 범위별로 보여줘요.",
  "docs.libraryPage.description":
    "내 skill · subagent를 모아 두고 프로젝트마다 나만 또는 팀으로 적용해요.",

  // Settings map: sections (kinds)
  "docs.section.instructions": "지침",
  "docs.section.instructionsHint": "CLAUDE.md · 항상 읽힘",
  "docs.section.rule": "규칙",
  "docs.section.skill": "Skills",
  "docs.section.skillHint": "필요할 때 읽힘",
  "docs.section.agent": "Subagents",
  "docs.section.agentHint": "별도 컨텍스트에서 실행",
  "docs.section.command": "명령",
  "docs.section.mcp": "MCP 서버",
  "docs.section.plugin": "플러그인",
  "docs.section.settings": "설정 파일",

  // Settings map: scopes
  "docs.scope.managed": "관리",
  "docs.scope.user": "개인",
  "docs.scope.parent": "상위 폴더",
  "docs.scope.project": "프로젝트",
  "docs.scope.local": "로컬",

  // Settings map: states
  "docs.state.active": "사용 중",
  "docs.state.conditional": "조건부",
  "docs.state.limited": "제한",
  "docs.state.off": "꺼짐",
  "docs.state.shadowed": "가려짐",
  "docs.state.notLoaded": "안 읽힘",

  // Settings map: panel
  "docs.map.reading": "읽는 중…",
  "docs.map.pickProject":
    "프로젝트 폴더를 고르면 Claude Code가 그 프로젝트에서 읽는 설정을 범위별로 보여줍니다.",
  "docs.map.empty": "이 프로젝트에 적용되는 Claude Code 설정이 없습니다.",
  "docs.project.recent": "최근 프로젝트",
  "docs.project.none": "프로젝트 없음",
  "docs.project.choose": "폴더 선택",
  "docs.project.chooseTitle": "프로젝트 폴더 선택",
  "docs.project.refresh": "다시 읽기",
  "docs.item.openFailed": "파일을 열 수 없습니다. 다시 읽기(↻)를 눌러 주세요.",
  "docs.item.turnOnPersonal": "켜기 (나만으로 끔)",
  "docs.item.turnOnTeam": "켜기 (팀으로 끔)",
  "docs.item.turnOff": "끄기",
  "docs.item.import": "라이브러리에 추가",

  // Personal / team buttons
  "docs.mode.personal": "나만",
  "docs.mode.personalHint": "git에 올리지 않고 나만 사용",
  "docs.mode.team": "팀",
  "docs.mode.teamHint": "커밋해서 팀과 공유",

  // Library: statuses
  "docs.status.current": "적용됨",
  "docs.status.outdated": "업데이트 있음",
  "docs.status.modified": "프로젝트에서 수정됨",
  "docs.status.missing": "복사본 없음",
  "docs.status.conflict": "같은 이름이 이미 있음",

  // Library: panel
  "docs.library.changeFolder": "폴더 변경",
  "docs.library.default": "기본값",
  "docs.library.chooseTitle": "라이브러리 폴더 선택",
  "docs.library.structureHint":
    "skills/<이름>/SKILL.md, agents/<이름>.md 구조. git 레포 폴더를 고르면 여러 PC에서 같이 쓸 수 있습니다.",
  "docs.library.pickProject":
    "설정 지도에서 프로젝트를 고르면 항목을 그 프로젝트에 적용하거나 뺄 수 있습니다.",
  "docs.library.empty":
    '라이브러리가 비어 있습니다. 설정 지도에서 skill·agent의 "라이브러리에 추가"를 누르거나 폴더에 직접 넣어 주세요.',
  "docs.library.apply": "적용",
  "docs.library.update": "업데이트",
  "docs.library.overwrite": "라이브러리 버전으로 덮어쓰기",
  "docs.library.forget": "기록 지우기",
  "docs.library.remove": "빼기",
  "docs.library.confirmRemove":
    "프로젝트에 있는 복사본이 수정되었습니다. 수정한 내용을 버리고 뺄까요?",
  "docs.library.confirmUpdate":
    "프로젝트에 있는 복사본이 수정되었습니다. 라이브러리 버전으로 덮어쓸까요?",
} as const;

export const en: Messages<typeof ko> = {
  "docs.view.map": "Settings map",
  "docs.view.library": "Library",
  "docs.mapPage.description":
    "Shows the instructions, skills, subagents and settings Claude Code reads in a project, by scope.",
  "docs.libraryPage.description":
    "Keep your skills and subagents in one place and apply them to each project, just for you or for your team.",

  "docs.section.instructions": "Instructions",
  "docs.section.instructionsHint": "CLAUDE.md · always loaded",
  "docs.section.rule": "Rules",
  "docs.section.skill": "Skills",
  "docs.section.skillHint": "Loaded when needed",
  "docs.section.agent": "Subagents",
  "docs.section.agentHint": "Run in a separate context",
  "docs.section.command": "Commands",
  "docs.section.mcp": "MCP servers",
  "docs.section.plugin": "Plugins",
  "docs.section.settings": "Settings files",

  "docs.scope.managed": "Managed",
  "docs.scope.user": "User",
  "docs.scope.parent": "Parent folder",
  "docs.scope.project": "Project",
  "docs.scope.local": "Local",

  "docs.state.active": "Active",
  "docs.state.conditional": "Conditional",
  "docs.state.limited": "Limited",
  "docs.state.off": "Off",
  "docs.state.shadowed": "Overridden",
  "docs.state.notLoaded": "Not loaded",

  "docs.map.reading": "Reading…",
  "docs.map.pickProject":
    "Choose a project folder to see the settings Claude Code reads in that project, by scope.",
  "docs.map.empty": "No Claude Code settings apply to this project.",
  "docs.project.recent": "Recent projects",
  "docs.project.none": "No project",
  "docs.project.choose": "Choose folder",
  "docs.project.chooseTitle": "Choose project folder",
  "docs.project.refresh": "Reload",
  "docs.item.openFailed": "Couldn't open the file. Try Reload (↻).",
  "docs.item.turnOnPersonal": "Turn on (off just for me)",
  "docs.item.turnOnTeam": "Turn on (off for team)",
  "docs.item.turnOff": "Turn off",
  "docs.item.import": "Add to library",

  "docs.mode.personal": "Just me",
  "docs.mode.personalHint": "Only for you, not pushed to git",
  "docs.mode.team": "Team",
  "docs.mode.teamHint": "Committed and shared with your team",

  "docs.status.current": "Applied",
  "docs.status.outdated": "Update available",
  "docs.status.modified": "Modified in project",
  "docs.status.missing": "Copy missing",
  "docs.status.conflict": "Name already in use",

  "docs.library.changeFolder": "Change folder",
  "docs.library.default": "Default",
  "docs.library.chooseTitle": "Choose library folder",
  "docs.library.structureHint":
    "Layout: skills/<name>/SKILL.md, agents/<name>.md. Choose a git repo folder to share it across PCs.",
  "docs.library.pickProject":
    "Choose a project in the settings map to apply items to it or remove them.",
  "docs.library.empty":
    'The library is empty. Click "Add to library" on a skill or agent in the settings map, or put files in the folder yourself.',
  "docs.library.apply": "Apply",
  "docs.library.update": "Update",
  "docs.library.overwrite": "Overwrite with library version",
  "docs.library.forget": "Clear record",
  "docs.library.remove": "Remove",
  "docs.library.confirmRemove":
    "The copy in the project has been modified. Discard your changes and remove it?",
  "docs.library.confirmUpdate":
    "The copy in the project has been modified. Overwrite it with the library version?",
};
