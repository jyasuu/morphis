"use client";

import messages from "@/messages/en.json";

type Messages = typeof messages;

function resolve(obj: Messages, path: string): string {
  const parts = path.split(".");
  let val: unknown = obj;
  for (const p of parts) {
    if (val && typeof val === "object" && p in val) {
      val = (val as Record<string, unknown>)[p];
    } else {
      return path;
    }
  }
  return typeof val === "string" ? val : path;
}

export function useT() {
  const t = (path: string, params?: Record<string, string | number>): string => {
    let msg = resolve(messages, path);
    if (params) {
      for (const [k, v] of Object.entries(params)) {
        msg = msg.replace(`{${k}}`, String(v));
      }
    }
    return msg;
  };

  t.entity = (raw: string): string => {
    const translated = t(`entity.${raw}`);
    return translated !== `entity.${raw}` ? translated : raw.replace(/_/g, " ");
  };

  t.field = (entityName: string, fieldName: string): string => {
    const translated = t(`field.${entityName}.${fieldName}`);
    return translated !== `field.${entityName}.${fieldName}` ? translated : fieldName;
  };

  return t;
}
