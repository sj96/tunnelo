export type RemoteScheme = "http" | "https" | "tcp" | null;

export type ParsedTarget = { host: string; port: number; scheme: RemoteScheme };

/** Parse remote target from URL or host:port. */
export function parseRemoteTarget(raw: string): ParsedTarget | null {
  const s = raw.trim();
  if (!s) return null;

  try {
    if (s.includes("://")) {
      const url = new URL(s);
      if (url.protocol === "tcp:") {
        if (!url.hostname || !url.port) return null;
        const port = Number(url.port);
        if (!Number.isInteger(port) || port <= 0) return null;
        return { host: url.hostname, port, scheme: "tcp" };
      }
      if (url.protocol !== "http:" && url.protocol !== "https:") return null;
      const scheme: RemoteScheme = url.protocol === "https:" ? "https" : "http";
      const port = url.port
        ? Number(url.port)
        : scheme === "https"
          ? 443
          : 80;
      if (!url.hostname || !port) return null;
      return { host: url.hostname, port, scheme };
    }

    const colon = s.lastIndexOf(":");
    if (colon > 0 && !s.includes("/")) {
      const host = s.slice(0, colon);
      const port = Number(s.slice(colon + 1));
      if (host && Number.isInteger(port) && port > 0) {
        const scheme: RemoteScheme = isIpAddress(host) ? "tcp" : "http";
        return { host, port, scheme };
      }
    }

    const host = s.split("/")[0];
    if (!host) return null;

    // Bare IPv4 (or partial) without :port is invalid — avoids misclassifying as https.
    if (isIpAddress(host) || /^\d+(\.\d+)*$/.test(host)) return null;

    return { host, port: 80, scheme: "http" };
  } catch {
    return null;
  }
  return null;
}

/** Parse a local service URL or host:port into bind target. */
export function parseServiceUrl(raw: string): { host: string; port: number } | null {
  const parsed = parseRemoteTarget(raw);
  return parsed ? { host: parsed.host, port: parsed.port } : null;
}

/** Strip scheme/path/port suffix from a stored or pasted host string. */
export function normalizeRemoteHost(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) return "";
  const parsed = parseRemoteTarget(trimmed);
  return parsed?.host ?? trimmed;
}

export function isIpAddress(host: string): boolean {
  if (host.startsWith("[")) return true;
  const parts = host.split(".");
  return parts.length === 4 && parts.every((p) => /^\d{1,3}$/.test(p));
}

/** Display string for Port Forwards input (preserves http/https or bare host:port). */
export function formatForwardInput(
  host: string,
  port: number,
  scheme: RemoteScheme = null,
): string {
  const h = normalizeRemoteHost(host);
  if (!h) return "";

  if (scheme === "tcp") {
    return `tcp://${h}:${port}`;
  }
  if (scheme === "https") {
    if (port === 443) return `https://${h}`;
    return `https://${h}:${port}`;
  }
  if (scheme === "http" || scheme === null) {
    if (port === 80) return `http://${h}`;
    return `http://${h}:${port}`;
  }
  return `${h}:${port}`;
}

/** Local endpoint the user connects to once the tunnel is active. */
export function formatLocalAccessUrl(
  host: string,
  port: number,
  scheme: RemoteScheme = null,
  bindHost = "127.0.0.1",
): string {
  const h = normalizeRemoteHost(host);
  if (!h) return "";

  if (scheme === "tcp" || isIpAddress(h)) {
    return `tcp://${bindHost}:${port}`;
  }
  if (scheme === "https") {
    if (port === 443) return `https://${h}`;
    return `https://${h}:${port}`;
  }
  if (scheme === "http" || scheme === null) {
    if (port === 80) return `http://${h}`;
    return `http://${h}:${port}`;
  }
  return `${bindHost}:${port}`;
}

/** Display string for access once the tunnel is active. */
export function formatAccessUrl(
  host: string,
  port: number,
  scheme: RemoteScheme = null,
): string {
  return formatLocalAccessUrl(host, port, scheme);
}

export function isWebForward(scheme: RemoteScheme | undefined): boolean {
  return scheme === "http" || scheme === "https";
}

/** @deprecated Use formatForwardInput */
export function formatForwardTarget(
  host: string,
  port: number,
  scheme: RemoteScheme = null,
): string {
  return formatForwardInput(host, port, scheme);
}

/** @deprecated Use formatAccessUrl */
export function formatRemoteUrl(
  host: string,
  port: number,
  scheme: RemoteScheme = null,
): string {
  return formatAccessUrl(host, port, scheme);
}

/** Display string for a local bind address. */
export function formatServiceUrl(host: string, port: number): string {
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

/** @deprecated Use formatAccessUrl */
export function formatPublicUrl(host: string): string {
  return host ? `https://${host}` : "";
}
