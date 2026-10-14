import { create } from "zustand";
import { getPref, onPrefChanged, setPref } from "./prefs";

/** Text size for the whole app; `md` is the default size. */
export type FontSize = "sm" | "md" | "lg" | "xl";
export const FONT_SIZES: FontSize[] = ["sm", "md", "lg", "xl"];

// Kept in prefs, like the theme.
const KEY = "fontSize";

function read(): FontSize {
  const value = getPref(KEY);
  return FONT_SIZES.includes(value as FontSize) ? (value as FontSize) : "md";
}

/** Sets `data-font-size` on <html>; index.css scales `--fs` from it. */
function apply(size: FontSize) {
  if (size === "md") delete document.documentElement.dataset.fontSize;
  else document.documentElement.dataset.fontSize = size;
}

interface FontSizeState {
  size: FontSize;
  setSize: (size: FontSize) => void;
}

export const useFontSizeStore = create<FontSizeState>((set) => ({
  size: read(),
  setSize: (size) => {
    setPref(KEY, size);
    apply(size);
    set({ size });
  },
}));

/** Applies the saved size before the first render and follows synced changes. */
export function initFontSize() {
  apply(useFontSizeStore.getState().size);
  onPrefChanged(KEY, () => {
    const size = read();
    apply(size);
    useFontSizeStore.setState({ size });
  });
}
