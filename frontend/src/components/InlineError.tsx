import { useState } from "react";
import { AlertCircle, Check, Clipboard } from "lucide-react";

type InlineErrorProps = {
  title?: string;
  message: string;
};

export function InlineError({ title = "Something needs attention", message }: InlineErrorProps) {
  const [copied, setCopied] = useState(false);
  const text = `${title}: ${message}`;

  function copyError() {
    void navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    });
  }

  return (
    <div
      role="alert"
      className="flex items-start gap-3 rounded-lg border border-orange-200 bg-orange-50 p-4 text-orange-900"
    >
      <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
      <div className="min-w-0 flex-1">
        <p className="text-sm font-semibold">{title}</p>
        <p className="mt-1 break-words text-sm text-orange-900/80">{message}</p>
      </div>
      <button
        type="button"
        className="inline-flex shrink-0 items-center gap-1 rounded-md px-2 py-1 text-xs font-medium text-orange-900/70 transition hover:bg-orange-100 hover:text-orange-950 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-orange-400"
        onClick={copyError}
        aria-label="Copy error details"
      >
        {copied ? (
          <Check className="h-3.5 w-3.5" aria-hidden="true" />
        ) : (
          <Clipboard className="h-3.5 w-3.5" aria-hidden="true" />
        )}
        {copied ? "Copied" : "Copy"}
      </button>
    </div>
  );
}
