import { PageHeader } from "../../components/ui";
import { WorkTrees } from "../../features/activity/WorkTrees";
import { useT } from "../../i18n";

export function TreesPage() {
  const t = useT();
  return (
    <div className="mx-auto max-w-4xl px-8 py-10">
      <PageHeader title={t("activity.title")} description={t("activity.description")} />
      <WorkTrees />
    </div>
  );
}
