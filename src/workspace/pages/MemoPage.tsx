import { PageHeader } from "../../components/ui";
import { MemoPanel } from "../../features/memo/MemoPanel";

export function MemoPage() {
  return (
    <div className="mx-auto max-w-2xl px-8 py-10">
      <PageHeader
        title="메모"
        description="브랜치마다 메모와 할 일을 남겨요. 터미널에서 브랜치를 바꾸면 따라 바뀌어요."
      />
      <MemoPanel />
    </div>
  );
}
