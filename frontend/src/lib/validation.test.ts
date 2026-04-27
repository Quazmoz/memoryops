import { isValidImportanceScore, validateImportanceScore } from "./validation";

describe("importance score validation", () => {
  it("accepts inclusive 0 to 1 values", () => {
    expect(isValidImportanceScore(0)).toBe(true);
    expect(isValidImportanceScore(0.5)).toBe(true);
    expect(isValidImportanceScore(1)).toBe(true);
    expect(validateImportanceScore(0.75)).toBeNull();
  });

  it("rejects values outside the supported range", () => {
    expect(isValidImportanceScore(-0.01)).toBe(false);
    expect(isValidImportanceScore(1.01)).toBe(false);
    expect(isValidImportanceScore(Number.NaN)).toBe(false);
    expect(validateImportanceScore(2)).toBe("Importance score must be between 0 and 1.");
  });
});