export function isValidImportanceScore(value: number): boolean {
  return Number.isFinite(value) && value >= 0 && value <= 1;
}

export function validateImportanceScore(value: number): string | null {
  if (isValidImportanceScore(value)) {
    return null;
  }

  return "Importance score must be between 0 and 1.";
}
