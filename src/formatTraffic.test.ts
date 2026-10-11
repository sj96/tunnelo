import { describe, expect, it } from "vitest";
import { formatBytes, formatRate } from "./formatTraffic";

describe("traffic formatting", () => {
  it("formats byte units for readable monitor values", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toBe("2.0 KB");
    expect(formatBytes(2 * 1024 * 1024)).toBe("2.0 MB");
  });

  it("adds a per-second suffix to rates", () => {
    expect(formatRate(1024)).toBe("1.0 KB/s");
  });
});
