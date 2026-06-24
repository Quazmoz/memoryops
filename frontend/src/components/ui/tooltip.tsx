import * as React from "react";
import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import { CircleHelp } from "lucide-react";

import { cn } from "../../lib/utils";

export function TooltipProvider({
  delayDuration = 320,
  disableHoverableContent = true,
  ...props
}: React.ComponentProps<typeof TooltipPrimitive.Provider>) {
  return <TooltipPrimitive.Provider delayDuration={delayDuration} disableHoverableContent={disableHoverableContent} {...props} />;
}

export const Tooltip = TooltipPrimitive.Root;
export const TooltipTrigger = TooltipPrimitive.Trigger;

export const TooltipContent = React.forwardRef<
  React.ElementRef<typeof TooltipPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof TooltipPrimitive.Content>
>(({ className, sideOffset = 10, collisionPadding = 12, ...props }, ref) => (
  <TooltipPrimitive.Portal>
    <TooltipPrimitive.Content
      ref={ref}
      sideOffset={sideOffset}
      collisionPadding={collisionPadding}
      className={cn(
        "z-[90] max-w-[22rem] rounded-md border border-zinc-800/80 bg-zinc-950 px-3 py-2 text-sm leading-5 text-white shadow-xl",
        className,
      )}
      {...props}
    />
  </TooltipPrimitive.Portal>
));
TooltipContent.displayName = TooltipPrimitive.Content.displayName;

type HelpTooltipProps = {
  label: string;
  children: React.ReactNode;
  className?: string;
  contentClassName?: string;
  side?: React.ComponentProps<typeof TooltipContent>["side"];
  align?: React.ComponentProps<typeof TooltipContent>["align"];
  iconClassName?: string;
};

export function HelpTooltip({
  label,
  children,
  className,
  contentClassName,
  side = "top",
  align = "center",
  iconClassName,
}: HelpTooltipProps) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          aria-label={`Help: ${label}`}
          className={cn(
            "inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-ink/45 transition hover:text-accent-strong focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent cursor-pointer",
            className,
          )}
        >
          <CircleHelp className={cn("h-3.5 w-3.5", iconClassName)} aria-hidden="true" />
        </button>
      </TooltipTrigger>
      <TooltipContent side={side} align={align} className={contentClassName}>
        <p>{children}</p>
        <TooltipPrimitive.Arrow className="fill-zinc-950" width={10} height={6} />
      </TooltipContent>
    </Tooltip>
  );
}

export function FieldHelp(props: HelpTooltipProps) {
  return <HelpTooltip {...props} className={cn("h-4.5 w-4.5", props.className)} iconClassName={cn("h-3.5 w-3.5", props.iconClassName)} />;
}

export function InfoLabel({
  label,
  tooltip,
  className,
  labelClassName,
}: {
  label: React.ReactNode;
  tooltip: React.ReactNode;
  className?: string;
  labelClassName?: string;
}) {
  const labelText = typeof label === "string" ? label : "field";

  return (
    <span className={cn("inline-flex items-center gap-1.5", className)}>
      <span className={labelClassName}>{label}</span>
      <FieldHelp label={labelText}>{tooltip}</FieldHelp>
    </span>
  );
}
