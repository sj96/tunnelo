/** Parse a local service URL or host:port into bind target. */
export function parseServiceUrl(raw: string): { host: string; port: number } | null {
  const s = raw.trim();
  if (!s) return null;

  try {
    if (s.includes("://")) {
      const url = new URL(s);
      if (url.protocol !== "http:" && url.protocol !== "https:") return null;
      const port = url.port
        ? Number(url.port)
        : url.protocol === "https:"
          ? 443
          : 80;
      if (!url.hostname || !port) return null;
      return { host: url.hostname, port };
    }

    const colon = s.lastIndexOf(":");
    if (colon > 0 && !s.includes("/")) {
      const host = s.slice(0, colon);
      const port = Number(s.slice(colon + 1));
      if (host && port > 0) return { host, port };
    }

    const host = s.split("/")[0];
    if (host) return { host, port: 443 };
  } catch {
    return null;
  }
  return null;
}

/** Display string for a local bind address. */
export function formatServiceUrl(host: string, port: number): string {
  if (!host) return "";
  if (port === 443) return `https://${host}`;
  if (port === 80) return `http://${host}`;
  return `https://${host}:${port}`;
}

/** Parse remote target from URL or host:port. */
export function parseRemoteTarget(raw: string): { host: string; port: number } | null {
  return parseServiceUrl(raw);
}

/** Display string for a remote target. */
export function formatRemoteUrl(host: string, port: number): string {
  if (!host) return "";
  if (port === 443) return `https://${host}`;
  if (port === 80) return `http://${host}`;
  return `https://${host}:${port}`;
}

/** @deprecated Use parseRemoteTarget */
export function parsePublicHost(raw: string): string | null {
  const parsed = parseRemoteTarget(raw);
  return parsed?.host ?? null;
}

/** @deprecated Use formatRemoteUrl */
export function formatPublicUrl(host: string): string {
  return host ? `https://${host}` : "";
}
