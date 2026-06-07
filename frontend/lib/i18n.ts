"use client";

import en from "@/messages/en.json";
import zhTW from "@/messages/zh-TW.json";
import { useLocale } from "@/components/locale-provider";
import type { Locale } from "@/components/locale-provider";

const messages: Record<Locale, Record<string, unknown>> = {
  en: en as unknown as Record<string, unknown>,
  "zh-TW": zhTW as unknown as Record<string, unknown>,
};

function resolve(msgs: Record<string, unknown>, path: string): string {
  const parts = path.split(".");
  let val: unknown = msgs;
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
  const { locale } = useLocale();

  const t = (path: string, params?: Record<string, string | number>): string => {
    const msgs = messages[locale] ?? messages.en;
    let msg = resolve(msgs, path);
    if (params) {
      for (const [k, v] of Object.entries(params)) {
        msg = msg.replace(`{${k}}`, String(v));
      }
    }
    return msg;
  };

  t.entity = (raw: string): string => {
    const msgs = messages[locale] ?? messages.en;
    const translated = resolve(msgs, `entity.${raw}`);
    return translated !== `entity.${raw}` ? translated : raw.replace(/_/g, " ");
  };

  t.field = (entityName: string, fieldName: string): string => {
    const msgs = messages[locale] ?? messages.en;
    const translated = resolve(msgs, `field.${entityName}.${fieldName}`);
    return translated !== `field.${entityName}.${fieldName}` ? translated : fieldName;
  };

  t.locale = locale;

  return t;
}
