import { Navigate, Route, Routes } from "react-router-dom";

import { AppShell } from "./components/layout/AppShell";
import { Dashboard } from "./views/Dashboard";
import { Ingest } from "./views/Ingest";
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
        <Route path="/trace" element={<StubView title="Retrieval Trace" message="Trace queries available in M6" />} />
        <Route path="/lifecycle" element={<StubView title="Lifecycle" message="Promotion timeline available in M8" />} />
        <Route path="/integrations" element={<StubView title="Integration Status" message="Integration health available in M6" />} />
        <Route path="/audit" element={<StubView title="Audit Log" message="Audit log available in M6" />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </AppShell>
  );
}