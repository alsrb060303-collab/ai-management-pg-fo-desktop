import { PageHeader } from "../../components/ui";
import { ConfigMapPanel } from "../../features/docs/ConfigMapPanel";

export function ConfigPage() {
  return (
    <div className="mx-auto max-w-3xl px-8 py-10">
      <PageHeader
        title="설정 지도"
        description="Claude Code가 프로젝트에서 읽는 지침 · skill · subagent · 설정을 범위별로 보여줘요."
      />
      <ConfigMapPanel />
    </div>
  );
}
