import { describe, expect, it } from "vitest";
import { tokens } from "./format";
describe("tokens", () => {
  it("shortens counts for the context meter", () => {
    expect(tokens(812)).toBe("812");
    expect(tokens(4_240)).toBe("4.2k");
    expect(tokens(128_432)).toBe("128k");
    expect(tokens(1_000_000)).toBe("1M");
    expect(tokens(1_500_000)).toBe("1.5M");
  });
});
