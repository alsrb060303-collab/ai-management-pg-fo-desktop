import { useReactFlow } from "@xyflow/react";
import { Trash2 } from "lucide-react";
import { useEffect, useState } from "react";

/**
 * Removes the node through React Flow, so undo, edge clean-up and the output
 * order go the same way as the Delete key.
 */
export function DeleteNodeButton({ id, label }: { id: string; label: string }) {
  const { deleteElements } = useReactFlow();
  return (
    <button
      type="button"
      aria-label={label}
      title={`${label} (Delete)`}
      onClick={(e) => {
        e.stopPropagation();
        void deleteElements({ nodes: [{ id }] });
      }}
      className="nodrag rounded-md p-1 text-text-subtle opacity-0 transition-opacity group-hover:opacity-100 hover:bg-hover hover:text-danger focus-visible:opacity-100"
    >
      <Trash2 size={13} />
    </button>
  );
}

/** A text field that saves on blur or Enter, so typing isn't one undo step per key. */
export function useDraft(value: string, commit: (next: string) => void) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  return {
    value: draft,
    onChange: (e: { target: { value: string } }) => setDraft(e.target.value),
    onBlur: () => draft !== value && commit(draft),
  };
}
