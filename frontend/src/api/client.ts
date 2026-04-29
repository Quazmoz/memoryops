import { useAppStore } from "../store/app-store";
import type { JsonValue } from "./types";

const BASE_URL = import.meta.env.VITE_API_BASE_URL ?? "/api";

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
  const init: RequestInit = {
    ...requestOptions,
    headers: requestHeaders(options, auth),
  };

  if (body !== undefined) {
    init.body = JSON.stringify(body);
  }

  const response = await fetch(apiUrl(path), init);
  const payload = await parseResponse(response);

  if (!response.ok) {
    throw new ApiError(response.status, extractDetail(payload, response.statusText));
  }

  return payload as T;
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
    if (value !== undefined && value !== null && value !== "") {
      searchParams.set(key, String(value));
    }
  });

  const encoded = searchParams.toString();
  return encoded ? `?${encoded}` : "";
}

export function extractDetail(payload: unknown, fallback: string): string {
  if (typeof payload === "string" && payload.trim().length > 0) {
    return payload;
  }

  if (isRecord(payload)) {
    const detail = payload.detail ?? payload.error ?? payload.message;
    if (typeof detail === "string" && detail.trim().length > 0) {
      return detail;
    }
  }

  return fallback || "Request failed";
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
