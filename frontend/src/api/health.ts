import { apiRequest } from "./client";

export interface HealthCheck {
  name: string;
  status: "ok" | "warn" | "error";
  latency_ms: number | null;
  message: string | null;
}

export interface SystemHealthResponse {
  status: "healthy" | "degraded" | "unhealthy";
  checks: HealthCheck[];
  checked_at: string;
}

export async function getSystemHealth(): Promise<SystemHealthResponse> {
  return apiRequest<SystemHealthResponse>("/health/system");
}
