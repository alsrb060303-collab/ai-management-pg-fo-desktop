import { PageHeader } from "../../components/ui";
import { WorkPanel } from "../../features/work/WorkPanel";

export function WorkPage() {
  return (
    <div className="mx-auto max-w-3xl px-8 py-10">
      <PageHeader
        title="작업"
        description="브랜치와 worktree를 만들고 바꾸고, 커밋하지 않은 변경을 확인해요. 커밋과 푸시는 Claude Code나 터미널에서 해요."
      />
      <WorkPanel />
    </div>
  );
}
