import { MemorySearchResult, MemoryUnit } from "./client";
import { truncate, firstLine } from "./markdown";

export function memoryFromCommandArgument(argument: unknown): MemorySearchResult | undefined {
  if (isMemory(argument)) {
    return argument;
  }
  return undefined;
}

export function memoryLabel(memory: MemoryUnit): string {
  return truncate(firstLine(memory.content ?? memory.id ?? "Memory"), 80);
}

function isMemory(value: unknown): value is MemorySearchResult {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    && ("content" in value || "id" in value)
    && ("memory_type" in value || "workspace_id" in value);
}