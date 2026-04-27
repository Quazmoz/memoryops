import { CheckCircle2, CircleDashed, XCircle } from "lucide-react";

import { cn } from "../lib/utils";

type StatusPillProps = {
  status: "ready" | "checking" | "unavailable";
  label: string;
};

export function StatusPill({ status, label }: StatusPillProps) {
  const Icon = status === "ready" ? CheckCircle2 : status === "checking" ? CircleDashed : XCircle;

  return (
    <span
      className={cn(
        "inline-flex items-center gap-2 rounded-md border px-2.5 py-1 text-xs font-medium",
        status === "ready" && "border-green-200 bg-green-50 text-green-700",
        status === "checking" && "border-amber-200 bg-amber-50 text-amber-800",
        status === "unavailable" && "border-orange-200 bg-orange-50 text-orange-800",
      )}
    >
      <Icon className="h-3.5 w-3.5" aria-hidden="true" />
      {label}
    </span>
  );
}
