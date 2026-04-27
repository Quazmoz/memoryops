/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_MEMORYOPS_WORKSPACE_ID?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
