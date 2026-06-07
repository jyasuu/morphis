"use client";

import { Icon, type IconName } from "./icon";

interface Props {
  icon?: IconName;
  title: string;
  description?: string;
}

export function EmptyState({ icon = "file-doc", title, description }: Props) {
  return (
    <div className="flex flex-col items-center justify-center py-12 text-center">
      <div className="mb-3 text-[var(--text-muted)]">
        <Icon name={icon} className="w-10 h-10" />
      </div>
      <p className="text-sm font-medium text-[var(--text-secondary)]">{title}</p>
      {description && (
        <p className="text-xs text-[var(--text-muted)] mt-1">{description}</p>
      )}
    </div>
  );
}
