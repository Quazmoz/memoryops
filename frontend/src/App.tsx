import { Navigate, Route, Routes } from "react-router-dom";

import { FirstRunGate } from "./components/FirstRunGate";
import { AppShell } from "./components/layout/AppShell";
import { AuditView } from "./views/AuditView";
import { ContradictionsView } from "./views/ContradictionsView";
import { Dashboard } from "./views/Dashboard";
import { Ingest } from "./views/Ingest";
import { IntegrationsView } from "./views/IntegrationsView";
import { LifecycleView } from "./views/LifecycleView";
import { MemoryDetail } from "./views/MemoryDetail";
import { MemoryExplorer } from "./views/MemoryExplorer";
import { RetrievalTraceView } from "./views/RetrievalTraceView";
import { GuideView } from "./views/GuideView";
import { SettingsView } from "./views/SettingsView";
import { ToolsView } from "./views/ToolsView";
import { AgentSkillsView } from "./views/AgentSkillsView";

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
          <Route path="/tools" element={<ToolsView />} />
          <Route path="/agent-skills" element={<AgentSkillsView />} />
          <Route path="/contradictions" element={<ContradictionsView />} />
          <Route path="/audit" element={<AuditView />} />
          <Route path="/guide" element={<GuideView />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </AppShell>
    </FirstRunGate>
  );
}
