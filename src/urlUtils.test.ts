import { describe, expect, it } from "vitest";

import {

  formatAccessUrl,

  formatForwardInput,

  parseRemoteTarget,

} from "./urlUtils";



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

    input: "http://172.16.54.37:5432",

    host: "172.16.54.37",

    port: 5432,

    scheme: "http" as const,

  },

  {

    input: "tcp://172.16.54.37:5432",

    host: "172.16.54.37",

    port: 5432,

    scheme: "tcp" as const,

  },

  {

    input: "172.16.54.37",

    host: null,

    port: null,

    scheme: null,

  },

];



describe("parseRemoteTarget + formatForwardInput round-trip", () => {

  for (const { input, host, port, scheme } of EXAMPLES) {

    it(`round-trips ${input}`, () => {

      const parsed = parseRemoteTarget(input);

      if (host === null) {

        expect(parsed).toBeNull();

        return;

      }

      expect(parsed).toEqual({ host, port, scheme });

      expect(formatForwardInput(host, port, scheme)).toBe(input);

    });

  }



  it("parses bare IP:port as tcp and canonicalizes to tcp://", () => {

    expect(parseRemoteTarget("172.16.54.37:5432")).toEqual({

      host: "172.16.54.37",

      port: 5432,

      scheme: "tcp",

    });

    expect(formatForwardInput("172.16.54.37", 5432, "tcp")).toBe(

      "tcp://172.16.54.37:5432",

    );

  });



  it("parses explicit tcp:// URL", () => {

    expect(parseRemoteTarget("tcp://172.16.54.37:5432")).toEqual({

      host: "172.16.54.37",

      port: 5432,

      scheme: "tcp",

    });

  });



  it("rejects tcp:// without port", () => {

    expect(parseRemoteTarget("tcp://172.16.54.37")).toBeNull();

  });



  it("parses bare hostname:port as http", () => {

    expect(parseRemoteTarget("app.example.com:8080")).toEqual({

      host: "app.example.com",

      port: 8080,

      scheme: "http",

    });

    expect(formatForwardInput("app.example.com", 8080, "http")).toBe(

      "http://app.example.com:8080",

    );

  });



  it("parses bare hostname without port as http on port 80", () => {

    expect(parseRemoteTarget("app.example.com")).toEqual({

      host: "app.example.com",

      port: 80,

      scheme: "http",

    });

    expect(formatForwardInput("app.example.com", 80, "http")).toBe(

      "http://app.example.com",

    );

  });



  it("defaults formatAccessUrl to http when scheme is null", () => {

    expect(formatAccessUrl("app.example.com", 443, null)).toBe(

      "http://app.example.com:443",

    );

  });



  it("formats tcp access URL with local loopback bind", () => {

    expect(formatAccessUrl("172.16.54.37", 5432, "tcp")).toBe(

      "tcp://127.0.0.1:5432",

    );

    expect(formatAccessUrl("172.16.54.37", 15432, "tcp")).toBe(

      "tcp://127.0.0.1:15432",

    );

  });



  it("keeps hostname in access URL for http/https forwards", () => {

    expect(formatAccessUrl("hrm.mservice.com.vn", 443, "https")).toBe(

      "https://hrm.mservice.com.vn",

    );

  });

});

