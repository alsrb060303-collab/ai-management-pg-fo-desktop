import { PageHeader } from "../../components/ui";
import { WorkPanel } from "../../features/work/WorkPanel";
import { useT } from "../../i18n";

export function WorkPage() {
  const t = useT();
  return (
    <div className="mx-auto max-w-3xl px-8 py-10">
      <PageHeader title={t("work.title")} description={t("work.description")} />
      <WorkPanel />
    </div>
  );
}
