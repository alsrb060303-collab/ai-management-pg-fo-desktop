import { useId } from "react";

/** The app mark (same drawing as src-tauri/icons/source/logo.svg). */
export function Logo({ size = 24, className = "" }: { size?: number; className?: string }) {
  const id = useId();
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
        <linearGradient id={id} x1="0.1" y1="0" x2="0.9" y2="1">
          <stop offset="0" stopColor="#f2915f" />
          <stop offset="0.55" stopColor="#c85d9a" />
          <stop offset="1" stopColor="#6d4fe8" />
        </linearGradient>
      </defs>
      <rect x="64" y="64" width="896" height="896" rx="208" fill={`url(#${id})`} />
      <path
        transform="rotate(45 492 532)"
        fill="#fff"
        fillOpacity="0.32"
        d="M492 302 C512 452 572 512 722 532 C572 552 512 612 492 762 C472 612 412 552 262 532 C412 512 472 452 492 302 Z"
      />
      <path
        fill="#fff"
        d="M492 252 C516 440 584 508 772 532 C584 556 516 624 492 812 C468 624 400 556 212 532 C400 508 468 440 492 252 Z"
      />
      <path
        fill="#fff"
        fillOpacity="0.9"
        d="M748 196 C755 252 776 273 832 280 C776 287 755 308 748 364 C741 308 720 287 664 280 C720 273 741 252 748 196 Z"
      />
    </svg>
  );
}
