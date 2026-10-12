import { PageHeader } from "../../components/ui";
import { ConfigMapPanel } from "../../features/docs/ConfigMapPanel";
import { useT } from "../../i18n";
import { useProjectStore } from "../../store/project";
import { useProjectDocsStore } from "../../store/projectDocs";
import { useWorkspaceStore } from "../../store/workspace";

export function ConfigPage() {
  const t = useT();
  return (
    <div className="mx-auto max-w-3xl px-8 py-10">
      <PageHeader title={t("docs.view.map")} description={t("docs.mapPage.description")} />
      <ConfigMapPanel
        onEdit={async (rel) => {
          const docs = useProjectDocsStore.getState();
          await docs.load(useProjectStore.getState().current);
          if (await docs.open(rel)) useWorkspaceStore.getState().go("files");
        }}
      />
    </div>
  );
}
