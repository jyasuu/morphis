"use client";

interface Props {
  children: React.ReactNode;
  className?: string;
}

export function Card({ children, className = "" }: Props) {
  return (
    <div className={`bg-white border border-zinc-200 rounded-xl p-6 shadow-sm ${className}`}>
      {children}
    </div>
  );
}
