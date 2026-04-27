import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "../../lib/utils";

const badgeVariants = cva("inline-flex items-center rounded-md border px-2 py-0.5 text-xs font-medium", {
  variants: {
    variant: {
      default: "border-line bg-white text-ink",
      accent: "border-accent/20 bg-accent/10 text-accent-strong",
      muted: "border-line bg-soft text-ink/70",
      blue: "border-blue-200 bg-blue-50 text-blue-700",
      purple: "border-purple-200 bg-purple-50 text-purple-700",
      green: "border-green-200 bg-green-50 text-green-700",
      amber: "border-amber-200 bg-amber-50 text-amber-800",
      teal: "border-teal-200 bg-teal-50 text-teal-700",
      gray: "border-zinc-200 bg-zinc-50 text-zinc-700",
      rust: "border-orange-200 bg-orange-50 text-orange-800",
    },
  },
  defaultVariants: {
    variant: "default",
  },
});

export type BadgeProps = React.HTMLAttributes<HTMLSpanElement> & VariantProps<typeof badgeVariants>;

export function Badge({ className, variant, ...props }: BadgeProps) {
  return <span className={cn(badgeVariants({ variant, className }))} {...props} />;
}
