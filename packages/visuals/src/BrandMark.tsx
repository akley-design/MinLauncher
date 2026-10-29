export function BrandMark({ className = "" }: { className?: string }) {
  return (
    <svg
      className={`ml-brand-mark ${className}`}
      viewBox="0 0 48 48"
      fill="none"
      aria-hidden="true"
    >
      <path d="m24 2 20 11v22L24 46 4 35V13Z" fill="#B6D879" />
      <path d="m24 26 20-13v22L24 46Z" fill="#78994F" />
      <path
        d="M11 17v15l6 3V23l7 7 7-7v12l6-3V17l-6-3-7 8-7-8Z"
        fill="#182019"
      />
      <path
        d="m4 13 20 13L44 13M24 26v20"
        stroke="#141716"
        strokeOpacity=".15"
      />
    </svg>
  );
}
