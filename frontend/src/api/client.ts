import { useAppStore } from "../store/app-store";
import type { JsonValue } from "./types";

const BASE_URL = import.meta.env.VITE_API_BASE_URL?.trim() || "/api";
const SLOW_PATHS = ["/v1/retrieve", "/v1/memory/search"];
const DEFAULT_TIMEOUT_MS = 15_000;
const SLOW_TIMEOUT_MS = 30_000;

type ApiRequestOptions = Omit<RequestInit, "body"> & {
  body?: JsonValue;
  auth?: boolean;
};

export class ApiError extends Error {
  readonly status: number;
  readonly detail: string;

  constructor(status: number, detail: string) {
    super(detail);
    this.name = "ApiError";
    this.status = status;
    this.detail = detail;
  }
}

export async function apiRequest<T>(path: string, options: ApiRequestOptions = {}): Promise<T> {
  const { body, auth = true, ...requestOptions } = options;
  const timeoutMs = SLOW_PATHS.some((slowPath) => path.startsWith(slowPath)) ? SLOW_TIMEOUT_MS : DEFAULT_TIMEOUT_MS;
  const controller = new AbortController();
  const timeoutId = window.setTimeout(() => controller.abort(), timeoutMs);
  const init: RequestInit = {
    ...requestOptions,
    headers: requestHeaders(options, auth),
    signal: controller.signal,
  };

  if (body !== undefined) {
    init.body = JSON.stringify(body);
  }

  try {
    const response = await fetch(apiUrl(path), init);
    const payload = await parseResponse(response);

    if (!response.ok) {
      throw new ApiError(response.status, extractDetail(payload, response.statusText));
    }

    return payload as T;
  } catch (error) {
    if (isAbortError(error)) {
      throw new ApiError(408, `Request timed out after ${timeoutMs / 1000}s`);
    }

    throw error;
  } finally {
    window.clearTimeout(timeoutId);
  }
}

export function apiUrl(path: string): string {
  if (/^https?:\/\//i.test(path)) {
    return path;
  }

  const base = BASE_URL.replace(/\/+$/, "");
  const normalizedPath = path.startsWith("/") ? path : `/${path}`;
  return `${base}${normalizedPath}`;
}

export function queryString(params: Record<string, string | number | boolean | null | undefined>): string {
  const searchParams = new URLSearchParams();

  Object.entries(params).forEach(([key, value]) => {
    const normalized = typeof value === "string" ? value.trim() : value;
    if (normalized !== undefined && normalized !== null && normalized !== "") {
      searchParams.set(key, String(normalized));
    }
  });

  const encoded = searchParams.toString();
  return encoded ? `?${encoded}` : "";
}

export function extractDetail(payload: unknown, fallback: string): string {
  if (typeof payload === "string") {
    const detail = payload.trim();
    if (detail.length > 0) {
      return detail;
    }
  }

  if (isRecord(payload)) {
    const detail = payload.detail ?? payload.error ?? payload.message;
    if (typeof detail === "string") {
      const trimmedDetail = detail.trim();
      if (trimmedDetail.length > 0) {
        return trimmedDetail;
      }
    }
  }

  const trimmedFallback = fallback.trim();
  return trimmedFallback || "Request failed";
}

export async function parseResponse(response: Response): Promise<unknown> {
  const text = await response.text();
  if (!text) {
    return null;
  }

  try {
    return JSON.parse(text) as unknown;
  } catch {
    return text;
  }
}

export function requestHeaders(options: Pick<RequestInit, "headers"> = {}, includeAuth = true): Headers {
  const headers = new Headers(options.headers);
  const apiKey = includeAuth ? useAppStore.getState().apiKey.trim() : "";

  if (!headers.has("content-type")) {
    headers.set("content-type", "application/json");
  }

  if (apiKey.length > 0) {
    headers.set("x-api-key", apiKey);
  }

  return headers;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function isAbortError(error: unknown): boolean {
  return error instanceof DOMException
    ? error.name === "AbortError"
    : error instanceof Error && error.name === "AbortError";
}
