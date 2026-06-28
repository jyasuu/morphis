"use client";

const defaultColorMap: Record<string, string> = {
  active: "bg-emerald-100 text-emerald-700 border-emerald-300",
  discontinued: "bg-red-100 text-red-700 border-red-300",
  inactive: "bg-[var(--muted)] text-[var(--text-secondary)] border-[var(--border)]",
  yes: "bg-emerald-100 text-emerald-700 border-emerald-300",
  no: "bg-[var(--muted)] text-[var(--text-secondary)] border-[var(--border)]",
  true: "bg-emerald-100 text-emerald-700 border-emerald-300",
  false: "bg-[var(--muted)] text-[var(--text-secondary)] border-[var(--border)]",
};

interface Props {
  value: string;
  colorMap?: Record<string, string>;
}

export function StatusBadge({ value, colorMap }: Props) {
  const combined = { ...defaultColorMap, ...colorMap };
  const classes = combined[value.toLowerCase()] ?? "bg-blue-100 text-blue-700 border-blue-300";
  return (
    <span className={`inline-block px-2.5 py-0.5 rounded-full text-xs font-medium border ${classes}`}>
      {value}
    </span>
  );
}
