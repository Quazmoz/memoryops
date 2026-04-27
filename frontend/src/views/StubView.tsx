import { Clock3 } from "lucide-react";

import { EmptyState } from "../components/EmptyState";
import { Badge } from "../components/ui/badge";

type StubViewProps = {
  title: string;
  message: string;
};

export function StubView({ title, message }: StubViewProps) {
  return (
    <div className="mx-auto grid max-w-5xl gap-6">
      <header>
        <p className="text-sm font-medium text-accent-strong">Milestone placeholder</p>
        <h1 className="mt-1 text-2xl font-semibold tracking-normal text-ink">{title}</h1>
      </header>
      <EmptyState
        title={message}
        message="The route is reserved so operators can keep the same navigation model as later milestones come online."
        action={
          <Badge variant="muted">
            <Clock3 className="mr-1 h-3 w-3" aria-hidden="true" />
            Available later
          </Badge>
        }
      />
    </div>
  );
}