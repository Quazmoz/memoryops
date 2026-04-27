import { AlertCircle } from "lucide-react";

type InlineErrorProps = {
  title?: string;
  message: string;
};

export function InlineError({ title = "Something needs attention", message }: InlineErrorProps) {
  return (
    <div className="flex items-start gap-3 rounded-lg border border-orange-200 bg-orange-50 p-4 text-orange-900">
      <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
      <div>
        <p className="text-sm font-semibold">{title}</p>
        <p className="mt-1 text-sm text-orange-900/80">{message}</p>
      </div>
    </div>
  );
}
