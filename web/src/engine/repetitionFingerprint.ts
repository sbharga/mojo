const MASK_64 = (1n << 64n) - 1n;
const FNV_OFFSET = 0xcbf29ce484222325n;
const FNV_PRIME = 0x100000001b3n;

function positionIdentity(fen: string) {
  return fen.trim().split(/\s+/).slice(0, 4).join(" ");
}

function hashPosition(fen: string) {
  let hash = FNV_OFFSET;
  for (const character of positionIdentity(fen)) {
    hash ^= BigInt(character.codePointAt(0) ?? 0);
    hash = (hash * FNV_PRIME) & MASK_64;
  }
  // SplitMix64 finalization makes the commutative sum less sensitive to the
  // linear structure of FNV while retaining duplicate-position counts.
  hash ^= hash >> 30n;
  hash = (hash * 0xbf58476d1ce4e5b9n) & MASK_64;
  hash ^= hash >> 27n;
  hash = (hash * 0x94d049bb133111ebn) & MASK_64;
  return hash ^ (hash >> 31n);
}

export function repetitionFingerprint(fen: string, priorFens: string[]) {
  const halfmoveClock = Number.parseInt(fen.trim().split(/\s+/)[4] ?? "0", 10);
  const window = Number.isFinite(halfmoveClock)
    ? Math.max(0, Math.min(halfmoveClock, priorFens.length))
    : 0;
  let fingerprint = 0n;
  for (const prior of priorFens.slice(priorFens.length - window)) {
    fingerprint = (fingerprint + hashPosition(prior)) & MASK_64;
  }
  return fingerprint.toString(16).padStart(16, "0");
}
