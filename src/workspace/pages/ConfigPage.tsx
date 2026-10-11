import { PageHeader } from "../../components/ui";
import { ConfigMapPanel } from "../../features/docs/ConfigMapPanel";
import { useT } from "../../i18n";

export function ConfigPage() {
  const t = useT();
  return (
    <div className="mx-auto max-w-3xl px-8 py-10">
      <PageHeader title={t("docs.view.map")} description={t("docs.mapPage.description")} />
      <ConfigMapPanel />
    </div>
  );
}
