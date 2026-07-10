/**
 * Waveform-H mark — the Harknotes brand icon.
 * Constructed from equaliser bars forming the letter H.
 */
export function HarknotesIcon({ size = 24, className }: { size?: number; className?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 512 512"
      fill="none"
      className={className}
    >
      <defs>
        <linearGradient id="hn-bg" x1="0" y1="0" x2="512" y2="512">
          <stop offset="0%" stopColor="#0d2520" />
          <stop offset="100%" stopColor="#061210" />
        </linearGradient>
        <radialGradient id="hn-gl" cx="50%" cy="25%" r="60%">
          <stop offset="0%" stopColor="#1a4a3a" stopOpacity="0.9" />
          <stop offset="100%" stopColor="transparent" stopOpacity="0" />
        </radialGradient>
      </defs>
      <rect width="512" height="512" rx="112" fill="url(#hn-bg)" />
      <rect width="512" height="512" rx="112" fill="url(#hn-gl)" />
      {/* Left pillar */}
      <rect x="98" y="138" width="52" height="236" rx="26" fill="#3dd68c" />
      {/* Left inner bar */}
      <rect x="168" y="182" width="44" height="148" rx="22" fill="#3dd68c" opacity="0.52" />
      {/* Crossbar */}
      <rect x="98" y="232" width="316" height="48" rx="24" fill="#3dd68c" />
      {/* Right inner bar */}
      <rect x="300" y="182" width="44" height="148" rx="22" fill="#3dd68c" opacity="0.52" />
      {/* Right pillar */}
      <rect x="362" y="138" width="52" height="236" rx="26" fill="#3dd68c" />
    </svg>
  );
}
