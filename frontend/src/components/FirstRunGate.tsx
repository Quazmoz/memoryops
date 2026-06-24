import {
  ArrowRight,
  Bot,
  Database,
  KeyRound,
  Loader2,
  Search,
  Shield,
  Sparkles,
} from "lucide-react";
import { useMutation } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { Link } from "react-router-dom";

import { getDefaultWorkspace } from "../api/workspaces";
import { useAppStore } from "../store/app-store";
import { InlineError } from "./InlineError";
import { Button } from "./ui/button";

type FirstRunGateProps = {
  children: ReactNode;
};

export function FirstRunGate({ children }: FirstRunGateProps) {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const apiKey = useAppStore((state) => state.apiKey);
  const setWorkspace = useAppStore((state) => state.setWorkspace);

  const defaultWorkspaceMutation = useMutation({
    mutationKey: ["default-workspace"],
    mutationFn: getDefaultWorkspace,
    onSuccess: (workspace) => {
      if (workspace.api_key) {
        setWorkspace(workspace.id, workspace.api_key);
      }
    },
  });

  const isReady = workspaceId.trim().length > 0 && apiKey.trim().length > 0;
  if (isReady) {
    return <>{children}</>;
  }

  return (
    <div className="min-h-screen bg-soft text-ink">
      <main className="mx-auto grid min-h-screen max-w-6xl content-center gap-8 px-4 py-8 sm:px-6 lg:grid-cols-[1.1fr_0.9fr] lg:items-center lg:px-8">
        <section className="grid gap-7">
          <div className="flex items-center gap-3">
            <div className="grid h-11 w-11 place-items-center rounded-lg bg-accent text-white">
              <KeyRound className="h-5 w-5" aria-hidden="true" />
            </div>
            <div>
              <p className="text-sm font-semibold">MemoryOps</p>
              <p className="text-xs text-ink/55">Capsule Corp Memory Lab</p>
            </div>
          </div>

          <div className="grid gap-4">
            <p className="text-sm font-medium text-accent-strong">Open demo workspace</p>
            <h1 className="max-w-3xl text-4xl font-semibold tracking-normal text-ink sm:text-5xl">
              Explore MemoryOps with a ready-made Dragon Ball Z workspace.
            </h1>
            <p className="max-w-2xl text-base leading-7 text-ink/65">
              Jump straight into seeded memories, searchable context, and the default agent library.
              No password needed.
            </p>
          </div>

          <div className="flex flex-col gap-3 sm:flex-row">
            <Button
              type="button"
              data-testid="enter-default-workspace"
              onClick={() => defaultWorkspaceMutation.mutate()}
              disabled={defaultWorkspaceMutation.isPending}
            >
              {defaultWorkspaceMutation.isPending ? (
                <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
              ) : (
                <ArrowRight className="h-4 w-4" aria-hidden="true" />
              )}
              Enter Default Workspace
            </Button>
            <Button asChild type="button" variant="secondary">
              <Link to="/admin">
                <Shield className="h-4 w-4" aria-hidden="true" />
                Admin
              </Link>
            </Button>
          </div>

          {defaultWorkspaceMutation.isError ? (
            <InlineError
              title="Default workspace unavailable"
              message={defaultWorkspaceMutation.error.message}
            />
          ) : null}
        </section>

        <section className="grid gap-3">
          <LandingSignal
            icon={Database}
            title="Seeded memories"
            description="Dragon Balls, Namek, Super Saiyan escalation, Senzu scarcity, and timeline risk."
          />
          <LandingSignal
            icon={Bot}
            title="Agent library"
            description="Default skills, agents, prompts, and instructions are ready on first entry."
          />
          <LandingSignal
            icon={Search}
            title="Searchable context"
            description="The demo data behaves like normal workspace memory, so retrieval and lifecycle views work immediately."
          />
          <LandingSignal
            icon={Sparkles}
            title="Admin unlock"
            description="Use the generated root password from the API container to create private workspaces and keys."
          />
        </section>
      </main>
    </div>
  );
}

type LandingSignalProps = {
  icon: typeof Database;
  title: string;
  description: string;
};

function LandingSignal({ icon: Icon, title, description }: LandingSignalProps) {
  return (
    <div className="grid gap-3 rounded-lg border border-line bg-white p-4 shadow-sm">
      <div className="flex items-center gap-3">
        <div className="grid h-9 w-9 place-items-center rounded-md bg-accent/10 text-accent-strong">
          <Icon className="h-4 w-4" aria-hidden="true" />
        </div>
        <p className="text-sm font-semibold text-ink">{title}</p>
      </div>
      <p className="text-sm leading-6 text-ink/60">{description}</p>
    </div>
  );
}
