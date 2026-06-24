import { Check, Clipboard, Eye, EyeOff, KeyRound, Loader2, Shield, TriangleAlert } from "lucide-react";
import { useMutation } from "@tanstack/react-query";
import type { FormEvent } from "react";
import { useState } from "react";
import { Link } from "react-router-dom";

import { createWorkspace, loginAdmin } from "../api/workspaces";
import type { WorkspaceSummary } from "../api/types";
import { InlineError } from "../components/InlineError";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Input } from "../components/ui/input";
import { useAppStore } from "../store/app-store";

export function AdminView() {
  const setWorkspace = useAppStore((state) => state.setWorkspace);
  const [rootPassword, setRootPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [workspaceName, setWorkspaceName] = useState("Private MemoryOps Workspace");
  const [createdWorkspace, setCreatedWorkspace] = useState<WorkspaceSummary | null>(null);
  const [copied, setCopied] = useState(false);

  const loginMutation = useMutation({
    mutationKey: ["admin", "login"],
    mutationFn: loginAdmin,
  });

  const createWorkspaceMutation = useMutation({
    mutationKey: ["admin", "workspace"],
    mutationFn: (name: string) => createWorkspace(name, rootPassword),
    onSuccess: (workspace) => {
      setCreatedWorkspace(workspace);
      setCopied(false);
    },
  });

  const unlocked = loginMutation.data === true;
  const apiKey = createdWorkspace?.api_key ?? "";

  function submitLogin(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    loginMutation.mutate(rootPassword);
  }

  function submitWorkspace(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const name = workspaceName.trim();
    if (name) {
      createWorkspaceMutation.mutate(name);
    }
  }

  function copyKey() {
    if (!apiKey) return;
    void navigator.clipboard.writeText(apiKey).then(() => setCopied(true));
  }

  function enterWorkspace() {
    if (createdWorkspace?.id && apiKey) {
      setWorkspace(createdWorkspace.id, apiKey);
    }
  }

  return (
    <div className="min-h-screen bg-soft px-4 py-8 text-ink">
      <main className="mx-auto grid max-w-3xl gap-6">
        <header className="flex items-center justify-between gap-4">
          <Link to="/" className="flex items-center gap-3 text-sm font-semibold text-ink">
            <div className="grid h-10 w-10 place-items-center rounded-lg bg-accent text-white">
              <KeyRound className="h-5 w-5" aria-hidden="true" />
            </div>
            MemoryOps
          </Link>
          <Button asChild variant="secondary" type="button">
            <Link to="/">Default Workspace</Link>
          </Button>
        </header>

        <Card>
          <CardHeader>
            <div className="mb-3 grid h-11 w-11 place-items-center rounded-lg bg-accent text-white">
              <Shield className="h-5 w-5" aria-hidden="true" />
            </div>
            <CardTitle>Admin</CardTitle>
            <p className="text-sm text-ink/65">
              Unlock with the root password generated on first startup.
            </p>
          </CardHeader>
          <CardContent className="grid gap-5">
            {!unlocked ? (
              <form className="grid gap-4" onSubmit={submitLogin}>
                <label className="grid gap-2 text-sm font-medium text-ink/70">
                  Root password
                  <div className="flex min-w-0 items-center rounded-md border border-line bg-white focus-within:border-accent focus-within:ring-2 focus-within:ring-accent/20">
                    <input
                      data-testid="admin-root-password-input"
                      type={showPassword ? "text" : "password"}
                      autoComplete="current-password"
                      value={rootPassword}
                      onChange={(event) => setRootPassword(event.target.value)}
                      className="min-w-0 flex-1 rounded-l-md bg-transparent px-3 py-2 text-sm text-ink outline-none"
                    />
                    <button
                      type="button"
                      className="grid h-10 w-10 shrink-0 place-items-center rounded-r-md text-ink/45 transition hover:bg-soft hover:text-ink/70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/40"
                      onClick={() => setShowPassword((visible) => !visible)}
                      aria-label={showPassword ? "Hide root password" : "Show root password"}
                    >
                      {showPassword ? (
                        <EyeOff className="h-4 w-4" aria-hidden="true" />
                      ) : (
                        <Eye className="h-4 w-4" aria-hidden="true" />
                      )}
                    </button>
                  </div>
                </label>
                <Button
                  type="submit"
                  data-testid="admin-login-button"
                  disabled={loginMutation.isPending || rootPassword.trim().length === 0}
                >
                  {loginMutation.isPending ? (
                    <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
                  ) : (
                    <Shield className="h-4 w-4" aria-hidden="true" />
                  )}
                  Unlock Admin
                </Button>
                {loginMutation.isError ? (
                  <InlineError title="Admin login failed" message={loginMutation.error.message} />
                ) : null}
              </form>
            ) : (
              <div className="grid gap-5">
                <div className="rounded-md border border-green-200 bg-green-50 px-3 py-2 text-sm font-medium text-green-700">
                  Admin unlocked
                </div>

                <form className="grid gap-4" onSubmit={submitWorkspace}>
                  <label className="grid gap-2 text-sm font-medium text-ink/70">
                    Workspace name
                    <Input
                      data-testid="admin-workspace-name-input"
                      value={workspaceName}
                      onChange={(event) => setWorkspaceName(event.target.value)}
                    />
                  </label>
                  <Button
                    type="submit"
                    data-testid="admin-create-workspace-button"
                    disabled={createWorkspaceMutation.isPending || workspaceName.trim().length === 0}
                  >
                    {createWorkspaceMutation.isPending ? (
                      <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
                    ) : (
                      <KeyRound className="h-4 w-4" aria-hidden="true" />
                    )}
                    Create Workspace
                  </Button>
                  {createWorkspaceMutation.isError ? (
                    <InlineError
                      title="Workspace creation failed"
                      message={createWorkspaceMutation.error.message}
                    />
                  ) : null}
                </form>

                {createdWorkspace && apiKey ? (
                  <div className="grid gap-3 rounded-lg border border-line bg-soft/50 p-4">
                    <div className="flex items-center gap-2 rounded-md border border-amber-200 bg-amber-50 p-3 text-sm font-semibold text-amber-900">
                      <TriangleAlert className="h-4 w-4 shrink-0" aria-hidden="true" />
                      Copy this API key now. It will not be shown again.
                    </div>
                    <p className="break-all rounded-md border border-line bg-white px-3 py-2 font-mono text-xs text-ink/75">
                      {apiKey}
                    </p>
                    <div className="flex flex-col gap-2 sm:flex-row">
                      <Button type="button" variant="secondary" onClick={copyKey}>
                        {copied ? (
                          <Check className="h-4 w-4" aria-hidden="true" />
                        ) : (
                          <Clipboard className="h-4 w-4" aria-hidden="true" />
                        )}
                        {copied ? "Copied" : "Copy Key"}
                      </Button>
                      <Button type="button" onClick={enterWorkspace} disabled={!copied}>
                        Enter Workspace
                      </Button>
                    </div>
                  </div>
                ) : null}
              </div>
            )}
          </CardContent>
        </Card>
      </main>
    </div>
  );
}
