"use client";

interface Props {
  children: React.ReactNode;
  className?: string;
}

export function Card({ children, className = "" }: Props) {
  return (
    <div className={`bg-[var(--surface)] border border-[var(--border)] rounded-xl p-6 shadow-sm ${className}`}>
      {children}
    </div>
  );
}
