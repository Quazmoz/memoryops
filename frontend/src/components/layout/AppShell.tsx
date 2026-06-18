import { Activity, BookOpen, Bot, Database, GitBranch, Home, KeyRound, PlugZap, ScrollText, Settings, ShieldAlert, Send, Wrench } from "lucide-react";
import type { ComponentType, ReactNode, SVGProps } from "react";
import { Link, NavLink } from "react-router-dom";

import { cn } from "../../lib/utils";
import { WorkspaceSwitcher } from "../WorkspaceSwitcher";

type AppShellProps = {
  children: ReactNode;
};

const primaryLinks = [
  { to: "/", label: "Dashboard", icon: Home, testId: "nav-dashboard" },
  { to: "/memory", label: "Memory", icon: Database, testId: "nav-memory" },
  { to: "/trace", label: "Traces", icon: Activity, testId: "nav-trace" },
  { to: "/lifecycle", label: "Lifecycle", icon: GitBranch, testId: "nav-lifecycle" },
  { to: "/ingest", label: "Ingest", icon: Send, testId: "nav-ingest" },
  { to: "/integrations", label: "Integrations", icon: PlugZap, testId: "nav-integrations" },
  { to: "/tools", label: "Tools", icon: Wrench, testId: "nav-tools" },
  { to: "/agent-skills", label: "Agent Library", icon: Bot, testId: "nav-agent-skills" },
  { to: "/contradictions", label: "Contradictions", icon: ShieldAlert, testId: "nav-contradictions" },
  { to: "/audit", label: "Audit", icon: ScrollText, testId: "nav-audit" },
  { to: "/guide", label: "Guide", icon: BookOpen, testId: "nav-guide" },
  { to: "/settings", label: "Settings", icon: Settings, testId: "nav-settings" },
];

export function AppShell({ children }: AppShellProps) {
  return (
    <div className="min-h-screen bg-soft text-ink lg:grid lg:grid-cols-[260px_1fr]">
      <aside className="border-b border-line bg-white lg:sticky lg:top-0 lg:flex lg:h-screen lg:flex-col lg:border-b-0 lg:border-r">
        <Link
          to="/"
          data-testid="nav-brand-home"
          aria-label="Go to dashboard"
          className="flex items-center gap-3 border-b border-line px-5 py-4 transition hover:bg-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent"
        >
          <div className="grid h-10 w-10 place-items-center rounded-lg bg-accent text-white">
            <KeyRound className="h-5 w-5" aria-hidden="true" />
          </div>
          <div>
            <p className="text-sm font-semibold">MemoryOps</p>
            <p className="text-xs text-ink/55">Control Center</p>
          </div>
        </Link>

        <WorkspaceSwitcher />

        <nav className="flex gap-2 overflow-x-auto px-3 py-3 lg:flex-1 lg:flex-col lg:gap-1 lg:overflow-visible" aria-label="Primary">
          {primaryLinks.map((link) => (
            <SidebarLink key={link.to} {...link} />
          ))}
        </nav>
      </aside>

      <main className="min-w-0 px-4 py-5 sm:px-6 lg:px-8 lg:py-8">{children}</main>
    </div>
  );
}

type SidebarLinkProps = {
  to: string;
  label: string;
  testId: string;
  icon: ComponentType<SVGProps<SVGSVGElement>>;
};

function SidebarLink({ to, label, testId, icon: Icon }: SidebarLinkProps) {
  return (
    <NavLink
      to={to}
      end={to === "/"}
      data-testid={testId}
      className={({ isActive }) =>
        cn(
          "flex min-h-10 items-center gap-3 rounded-md px-3 py-2 text-sm font-medium text-ink/70 transition hover:bg-soft hover:text-ink",
          isActive && "bg-accent/10 text-accent-strong",
        )
      }
    >
      <Icon className="h-4 w-4 shrink-0" aria-hidden={true} />
      <span className="truncate">{label}</span>
    </NavLink>
  );
}
