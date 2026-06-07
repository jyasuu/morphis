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
