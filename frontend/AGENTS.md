<!-- BEGIN:nextjs-agent-rules -->
# This is NOT the Next.js you know

This version has breaking changes — APIs, conventions, and file structure may all differ from your training data. Read the relevant guide in `node_modules/next/dist/docs/` before writing any code. Heed deprecation notices.
<!-- END:nextjs-agent-rules -->

## Dev

Run in cloud IDE (CodeSandbox, etc.) — polling mode avoids WebSocket HMR issues:
```bash
NEXT_PUBLIC_GRAPHQL_URL=https://4vxy5k-4000.csb.app/graphql NEXT_HMR_POLL_INTERVAL=2000 npx next dev --port 3000
```

Replace `4vxy5k` with the current CodeSandbox workspace ID.

## Runtime i18n

Locale files live in `public/locales/` and are fetched at runtime by the browser — no rebuild needed.

### File structure
```
public/locales/
  index.json   # Available locales (deployer edits to add/remove)
  en.json      # English translations
  zh-TW.json   # Traditional Chinese
  ...          # Deployer adds more
```

### Adding a new locale
1. Add entry to `public/locales/index.json`:
   ```json
   { "code": "ja", "label": "日本語" }
   ```
2. Create `public/locales/ja.json` with translations (copy existing locale as template)
3. Done — no build step required

### Translation key structure
- UI strings use dotted paths: `list.search`, `table.actions`, `form.create`
- Entity names under `entity.*`: `entity.materials`, `entity.colorways`
- Field names under `field.<entity>.*`: `field.materials.mat_no`
- Unrecognized entities fall back to `name.replace(/_/g, " ")`
- Unrecognized fields fall back to raw field name

### Fallback behavior
If a locale file fails to load (network error, missing file), the bundled `messages/en.json` is used as fallback. This also covers SSR — initial render always shows English until the client fetches the preferred locale.
