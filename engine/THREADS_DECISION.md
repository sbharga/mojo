# Optional Lazy SMP decision

Decision date: 2026-07-14  
Status: deferred; keep the single-threaded baseline and SIMD builds

Mojo must not ship a nominal threads artifact until it can run as real Lazy
SMP with shared, race-safe search knowledge. The current production target is
GitHub Pages, which does not let this repository attach the COOP/COEP response
headers needed for `crossOriginIsolated`. The optional SharedArrayBuffer stop
flag therefore already falls back safely there, and a threaded Wasm artifact
would be unreachable in the deployed application.

The engine is not internally ready for shared-memory search either. Each
`SearchCore` exclusively owns a boxed transposition table whose 16-byte entries
are updated with ordinary writes. Compiling that memory as shared before the
table has a torn-write-safe protocol would introduce data races, not Lazy SMP.
The existing analysis and move workers deliberately retain separate engine
instances; item 27 shares validated PV knowledge without weakening that
isolation.

## Measured baseline

Release benchmark after items 25–27:

- baseline Wasm: 1,453,959 bytes raw / 219,858 bytes gzip
- SIMD Wasm: 1,455,071 bytes raw / 220,010 bytes gzip
- initial Wasm memory: 2,686,976 bytes
- measured peak Wasm memory: 5,439,488 bytes per engine instance
- adaptive deadline overrun: 0–2 ms in the 24-case benchmark

A threaded build adds a third artifact, shared-memory initialization and a
stack per helper thread. Those costs have no production payoff while the host
cannot isolate the page.

## Adoption gates

Implement and ship the progressive-enhancement build only when all gates pass:

1. Production hosting sends `Cross-Origin-Opener-Policy: same-origin` and
   `Cross-Origin-Embedder-Policy: require-corp`, and an end-to-end browser test
   asserts `crossOriginIsolated === true`.
2. The TT uses a documented lockless publication scheme (for example,
   key/payload XOR validation) and stress tests reject torn or mismatched
   entries under concurrent writers.
3. A two- or three-thread Lazy SMP search shares that TT while retaining
   thread-local search stacks, histories and PV state. The single-thread build
   remains the unconditional fallback.
4. Feature detection requires shared-memory Wasm support and caps helpers to
   `min(3, hardwareConcurrency - 1)`; one- and two-core devices stay
   single-threaded.
5. Browser benchmarks at 100, 500 and 1,000 ms show at least a 20% median node
   or completed-depth gain on desktop and no deadline regression above one
   adaptive clock interval.
6. A representative mobile run shows no sustained regression from thread
   startup or thermal throttling, and both tactical suites plus paired
   fixed-time self-play remain green.

Until those gates are achievable, the smaller and safer optimization is to
retain cross-worker PV seeding and avoid shipping dead threaded code.
