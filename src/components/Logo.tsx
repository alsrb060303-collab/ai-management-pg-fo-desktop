import { useId } from "react";

const RING = "translate(492 540) rotate(-20)";
const FRONT = "M-318 0 A318 86 0 0 0 318 0";
const BACK = "M-318 0 A318 86 0 0 1 318 0";

/** The app mark, a ringed planet (same drawing as src-tauri/icons/source/logo.svg). */
export function Logo({ size = 24, className = "" }: { size?: number; className?: string }) {
  const id = useId();
  const bg = `${id}-bg`;
  const gap = `${id}-gap`;
  return (
    <svg
      width={size}
      height={size}
      viewBox="64 64 896 896"
      className={className}
      role="img"
      aria-label="AI Management"
    >
      <defs>
        <linearGradient id={bg} x1="0.1" y1="0" x2="0.9" y2="1">
          <stop offset="0" stopColor="#f2915f" />
          <stop offset="0.55" stopColor="#c85d9a" />
          <stop offset="1" stopColor="#6d4fe8" />
        </linearGradient>
        <mask id={gap}>
          <rect width="1024" height="1024" fill="#fff" />
          <path transform={RING} fill="none" stroke="#000" strokeWidth="96" d={FRONT} />
        </mask>
      </defs>
      <rect x="64" y="64" width="896" height="896" rx="208" fill={`url(#${bg})`} />
      <path
        transform={RING}
        fill="none"
        stroke="#fff"
        strokeOpacity="0.32"
        strokeWidth="44"
        strokeLinecap="round"
        d={BACK}
      />
      <circle cx="492" cy="540" r="178" fill="#fff" mask={`url(#${gap})`} />
      <path
        transform={RING}
        fill="none"
        stroke="#fff"
        strokeWidth="44"
        strokeLinecap="round"
        d={FRONT}
      />
      <circle cx="768" cy="262" r="46" fill="#fff" fillOpacity="0.9" />
    </svg>
  );
}
