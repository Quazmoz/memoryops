import { Navigate, Route, Routes } from "react-router-dom";

import { AppShell } from "./components/layout/AppShell";
import { AuditView } from "./views/AuditView";
import { Dashboard } from "./views/Dashboard";
import { Ingest } from "./views/Ingest";
import { IntegrationsView } from "./views/IntegrationsView";
import { MemoryDetail } from "./views/MemoryDetail";
import { MemoryExplorer } from "./views/MemoryExplorer";
import { SettingsView } from "./views/SettingsView";
import { StubView } from "./views/StubView";

export default function App() {
  return (
    <AppShell>
      <Routes>
        <Route path="/" element={<Dashboard />} />
        <Route path="/memory" element={<MemoryExplorer />} />
        <Route path="/memory/:id" element={<MemoryDetail />} />
        <Route path="/ingest" element={<Ingest />} />
        <Route path="/settings" element={<SettingsView />} />
        <Route path="/trace" element={<StubView title="Retrieval Trace" message="Promotion traces available in M8" />} />
        <Route path="/lifecycle" element={<StubView title="Lifecycle" message="Promotion timeline available in M8" />} />
        <Route path="/integrations" element={<IntegrationsView />} />
        <Route path="/audit" element={<AuditView />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </AppShell>
  );
}