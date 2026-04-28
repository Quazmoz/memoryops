import { Navigate, Route, Routes } from "react-router-dom";

import { FirstRunGate } from "./components/FirstRunGate";
import { AppShell } from "./components/layout/AppShell";
import { AuditView } from "./views/AuditView";
import { Dashboard } from "./views/Dashboard";
import { Ingest } from "./views/Ingest";
import { IntegrationsView } from "./views/IntegrationsView";
import { LifecycleView } from "./views/LifecycleView";
import { MemoryDetail } from "./views/MemoryDetail";
import { MemoryExplorer } from "./views/MemoryExplorer";
import { RetrievalTraceView } from "./views/RetrievalTraceView";
import { SettingsView } from "./views/SettingsView";

export default function App() {
  return (
    <FirstRunGate>
      <AppShell>
        <Routes>
          <Route path="/" element={<Dashboard />} />
          <Route path="/memory" element={<MemoryExplorer />} />
          <Route path="/memory/:id" element={<MemoryDetail />} />
          <Route path="/ingest" element={<Ingest />} />
          <Route path="/settings" element={<SettingsView />} />
          <Route path="/trace" element={<RetrievalTraceView />} />
          <Route path="/lifecycle" element={<LifecycleView />} />
          <Route path="/integrations" element={<IntegrationsView />} />
          <Route path="/audit" element={<AuditView />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </AppShell>
    </FirstRunGate>
  );
}
