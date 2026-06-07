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
      <div className="mb-3 text-zinc-300">
        <Icon name={icon} className="w-10 h-10" />
      </div>
      <p className="text-sm font-medium text-zinc-500">{title}</p>
      {description && (
        <p className="text-xs text-zinc-400 mt-1">{description}</p>
      )}
    </div>
  );
}
