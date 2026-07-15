export interface StopSignal {
  buffer: SharedArrayBuffer;
  view: Int32Array;
}

export function createStopSignal(): StopSignal | null {
  if (
    !globalThis.crossOriginIsolated
    || typeof globalThis.SharedArrayBuffer !== "function"
  ) return null;
  const buffer = new SharedArrayBuffer(Int32Array.BYTES_PER_ELEMENT);
  return { buffer, view: new Int32Array(buffer) };
}

export function cancelThrough(signal: Int32Array | null, requestId: number) {
  if (signal) Atomics.store(signal, 0, requestId);
}

export function isCancelled(signal: Int32Array | null, requestId: number) {
  return signal !== null && Atomics.load(signal, 0) >= requestId;
}
