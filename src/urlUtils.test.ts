import { describe, expect, it } from "vitest";
import { formatForwardInput, parseRemoteTarget } from "./urlUtils";

const EXAMPLES = [
  {
    input: "https://hrm.mservice.com.vn",
    host: "hrm.mservice.com.vn",
    port: 443,
    scheme: "https" as const,
  },
  {
    input: "http://nexus.mservice.com.vn",
    host: "nexus.mservice.com.vn",
    port: 80,
    scheme: "http" as const,
  },
  {
    input: "https://atlassiansuite.mservice.com.vn:8443",
    host: "atlassiansuite.mservice.com.vn",
    port: 8443,
    scheme: "https" as const,
  },
  {
    input: "172.16.54.37:5432",
    host: "172.16.54.37",
    port: 5432,
    scheme: null,
  },
];

describe("parseRemoteTarget + formatForwardInput round-trip", () => {
  for (const { input, host, port, scheme } of EXAMPLES) {
    it(`round-trips ${input}`, () => {
      const parsed = parseRemoteTarget(input);
      expect(parsed).toEqual({ host, port, scheme });
      expect(formatForwardInput(host, port, scheme)).toBe(input);
    });
  }
});
