import { PageHeader } from "../../components/ui";
import { LibraryPanel } from "../../features/docs/LibraryPanel";

export function LibraryPage() {
  return (
    <div className="mx-auto max-w-3xl px-8 py-10">
      <PageHeader
        title="라이브러리"
        description="내 skill · subagent를 모아 두고 프로젝트마다 나만 또는 팀으로 적용해요."
      />
      <LibraryPanel />
    </div>
  );
}
