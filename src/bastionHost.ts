/** Mirrors `bastion_resolve.rs` — IPv4 literal or wildcard pattern validation. */

export const MAX_BASTION_CANDIDATES = 512;

export function isWildcardBastionHost(host: string): boolean {
  const parts = host.trim().split(".");
  if (parts.length !== 4) return false;
  return parts.some((p) => p === "*");
}

function octetExpansion(part: string): number | null {
  if (part === "*") return 254;
  if (!/^\d{1,3}$/.test(part)) return null;
  const n = Number(part);
  if (!Number.isInteger(n) || n < 0 || n > 255) return null;
  return 1;
}

/** Valid IPv4 literal, wildcard pattern, or plain hostname (no `*`). */
export function isValidBastionHost(host: string): boolean {
  const h = host.trim();
  if (!h) return false;
  if (h.includes("*")) {
    const parts = h.split(".");
    if (parts.length !== 4) return false;
    let total = 1;
    for (const part of parts) {
      const count = octetExpansion(part);
      if (count === null) return false;
      total *= count;
      if (total > MAX_BASTION_CANDIDATES) return false;
    }
    return isWildcardBastionHost(h);
  }
  return true;
}
