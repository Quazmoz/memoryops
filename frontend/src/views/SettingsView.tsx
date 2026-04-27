import { KeyRound, PlugZap, ScrollText, ShieldCheck } from "lucide-react";

import type { ProviderDefaults } from "../api/types";
import { EmptyState } from "../components/EmptyState";
import { Badge } from "../components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { useAppStore } from "../store/app-store";

const providerDefaults: ProviderDefaults = {
  embedding: {
    provider: "fastembed",
    model: "BAAI/bge-small-en-v1.5",
  },
  llm: {
    provider: "ollama",
    model: "llama3",
    baseUrl: "http://localhost:11434",
  },
};

export function SettingsView() {
  const workspaceId = useAppStore((state) => state.workspaceId);

  return (
    <div className="mx-auto grid max-w-7xl gap-6">
      <header>
        <p className="text-sm font-medium text-accent-strong">Read-only</p>
        <h1 className="mt-1 text-2xl font-semibold tracking-normal text-ink">Workspace Settings</h1>
      </header>

      <section className="grid gap-4 lg:grid-cols-[1fr_1fr]">
        <Card>
          <CardHeader>
            <CardTitle>Workspace</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div>
              <p className="text-xs font-medium uppercase text-ink/45">Workspace ID</p>
              <p className="mt-1 break-all rounded-md border border-line bg-soft px-3 py-2 font-mono text-sm">{workspaceId}</p>
            </div>
            <Badge variant="muted">Full workspace management available in M6</Badge>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Provider config</CardTitle>
          </CardHeader>
          <CardContent className="grid gap-4 sm:grid-cols-2">
            <ProviderBlock title="Embedding" rows={[providerDefaults.embedding.provider, providerDefaults.embedding.model]} />
            <ProviderBlock title="LLM" rows={[providerDefaults.llm.provider, providerDefaults.llm.model, providerDefaults.llm.baseUrl]} />
          </CardContent>
        </Card>
      </section>

      <section className="grid gap-4 md:grid-cols-3">
        <StubCard title="Integrations" icon={<PlugZap className="h-4 w-4" />} message="Integration setup lands with guarded workspace management." />
        <StubCard title="API Keys" icon={<KeyRound className="h-4 w-4" />} message="Key creation and rotation arrive with M6 auth." />
        <StubCard title="Audit Log" icon={<ScrollText className="h-4 w-4" />} message="Operator activity appears here once audit endpoints are live." />
      </section>
    </div>
  );
}

function ProviderBlock({ title, rows }: { title: string; rows: string[] }) {
  return (
    <div className="rounded-lg border border-line bg-soft p-4">
      <p className="text-sm font-semibold">{title}</p>
      <div className="mt-3 grid gap-2">
        {rows.map((row) => (
          <p key={row} className="break-all font-mono text-xs text-ink/70">
            {row}
          </p>
        ))}
      </div>
    </div>
  );
}

function StubCard({ title, icon, message }: { title: string; icon: React.ReactNode; message: string }) {
  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-3">
        <CardTitle>{title}</CardTitle>
        <span className="text-accent-strong">{icon}</span>
      </CardHeader>
      <CardContent className="space-y-4">
        <EmptyState title="Available in M6" message={message} />
        <Badge variant="muted">
          <ShieldCheck className="mr-1 h-3 w-3" aria-hidden="true" />
          Available in M6
        </Badge>
      </CardContent>
    </Card>
  );
}
