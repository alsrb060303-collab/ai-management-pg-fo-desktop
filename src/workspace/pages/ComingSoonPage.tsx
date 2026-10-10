import { Hourglass } from "lucide-react";
import { PageHeader } from "../../components/ui";

export function ComingSoonPage({ title, description }: { title: string; description: string }) {
  return (
    <div className="mx-auto max-w-3xl px-8 py-10">
      <PageHeader title={title} description={description} />
      <div className="flex flex-col items-center rounded-xl border border-dashed border-border-strong px-6 py-14 text-center">
        <Hourglass size={22} className="mb-3 text-text-subtle" />
        <p className="text-sm font-medium">준비 중이에요</p>
        <p className="mt-1 text-xs text-text-muted">다음 업데이트에서 만나요.</p>
      </div>
    </div>
  );
}
