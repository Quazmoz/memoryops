import { Check, Copy } from "lucide-react";
import { useState } from "react";

import { useAppStore } from "../store/app-store";

interface CodeBlockProps {
  code: string;
}

export function CodeBlock({ code }: CodeBlockProps) {
  const [copied, setCopied] = useState(false);
  const workspaceId = useAppStore((s) => s.workspaceId);
  const apiKey = useAppStore((s) => s.apiKey);

  const resolved = code
    .replace(/\{\{WORKSPACE_ID\}\}/g, workspaceId || "<YOUR_WORKSPACE_ID>")
    .replace(/\{\{API_KEY\}\}/g, apiKey || "<YOUR_API_KEY>")
    .replace(/\{\{API_URL\}\}/g, resolvedApiUrl());

  async function copy() {
    await navigator.clipboard.writeText(resolved);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }

  return (
    <div className="relative rounded-lg bg-ink" data-testid="code-block">
      <button
        type="button"
        onClick={copy}
        aria-label={copied ? "Copied!" : "Copy code"}
        className="absolute right-2.5 top-2.5 flex items-center gap-1 rounded px-2 py-1 text-xs text-white/55 transition hover:bg-white/10 hover:text-white/80"
      >
        {copied ? (
          <>
            <Check className="h-3.5 w-3.5" aria-hidden="true" />
            Copied!
          </>
        ) : (
          <>
            <Copy className="h-3.5 w-3.5" aria-hidden="true" />
            Copy
          </>
        )}
      </button>
      <pre className="overflow-x-auto px-4 pb-4 pt-4 pr-20 font-mono text-sm leading-relaxed text-white/85">
        <code>{resolved}</code>
      </pre>
    </div>
  );
}

function resolvedApiUrl(): string {
  const configured = import.meta.env.VITE_API_BASE_URL as string | undefined;
  if (configured && /^https?:\/\//i.test(configured)) {
    return configured;
  }
  const path = (configured ?? "/api").replace(/\/+$/, "");
  return `${window.location.origin}${path.startsWith("/") ? "" : "/"}${path}`;
}
