import type { ReactNode } from "react";

export function Icon({
  name,
}: {
  name: "arrow" | "menu" | "close" | "spark" | "cube" | "bolt" | "check";
}) {
  const paths = {
    arrow: (
      <>
        <path d="M4 8h8M8 4l4 4-4 4" />
      </>
    ),
    menu: (
      <>
        <path d="M3 5h10M3 8h10M3 11h10" />
      </>
    ),
    close: (
      <>
        <path d="m4 4 8 8M12 4l-8 8" />
      </>
    ),
    spark: (
      <>
        <path d="m8 1 1.6 5.4L15 8l-5.4 1.6L8 15l-1.6-5.4L1 8l5.4-1.6z" />
      </>
    ),
    cube: (
      <>
        <path d="m8 1 6 3.5v7L8 15l-6-3.5v-7z" />
        <path d="M2.3 4.7 8 8l5.7-3.3M8 8v7" />
      </>
    ),
    bolt: (
      <>
        <path d="M9 1 3 9h4l-1 6 7-9H9z" />
      </>
    ),
    check: (
      <>
        <path d="m3 8 3.2 3L13 4" />
      </>
    ),
  };
  return (
    <svg
      className="icon"
      viewBox="0 0 16 16"
      aria-hidden="true"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      {paths[name]}
    </svg>
  );
}

export function PrimaryLink({
  href,
  children,
  variant = "primary",
}: {
  href: string;
  children: ReactNode;
  variant?: "primary" | "secondary";
}) {
  return (
    <a className={`button button--${variant}`} href={href}>
      {children}
      <Icon name="arrow" />
    </a>
  );
}
