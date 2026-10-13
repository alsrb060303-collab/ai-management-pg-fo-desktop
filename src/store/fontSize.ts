import { create } from "zustand";

/** Text size for the whole app; `md` is the default size. */
export type FontSize = "sm" | "md" | "lg" | "xl";
export const FONT_SIZES: FontSize[] = ["sm", "md", "lg", "xl"];

// Shared by both windows through localStorage, like the theme.
const KEY = "fontSize";

function read(): FontSize {
  try {
    const value = localStorage.getItem(KEY);
    if (FONT_SIZES.includes(value as FontSize)) return value as FontSize;
  } catch {
    // Storage can be unavailable; use the default.
  }
  return "md";
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
    try {
      localStorage.setItem(KEY, size);
    } catch {
      // Still applies for this session.
    }
    apply(size);
    set({ size });
  },
}));

/** Applies the saved size before the first render and follows the other window. */
export function initFontSize() {
  apply(useFontSizeStore.getState().size);
  window.addEventListener("storage", (event) => {
    if (event.key !== KEY) return;
    const size = read();
    apply(size);
    useFontSizeStore.setState({ size });
  });
}
