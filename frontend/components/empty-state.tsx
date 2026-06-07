"use client";

interface Props {
  icon?: string;
  title: string;
  description?: string;
}

export function EmptyState({ icon = "📄", title, description }: Props) {
  return (
    <div className="flex flex-col items-center justify-center py-12 text-center">
      <span className="text-3xl mb-3 opacity-50">{icon}</span>
      <p className="text-sm font-medium text-zinc-500">{title}</p>
      {description && (
        <p className="text-xs text-zinc-400 mt-1">{description}</p>
      )}
    </div>
  );
}
