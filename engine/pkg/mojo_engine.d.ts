/* tslint:disable */
/* eslint-disable */

/**
 * A reusable engine instance. Search heuristics and its fixed-size
 * transposition table survive iterative depths and adjacent positions.
 */
export class Engine {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Searches one iterative-deepening step while retaining earlier search state.
     *
     * # Errors
     * Returns an error if no position has been set or serialization fails.
     */
    analyze_depth(depth: number, multi_pv: number, time_limit_ms: number): any;
    /**
     * Returns the best static one-ply fallback for the current position.
     */
    fallback_move(): string | undefined;
    constructor();
    /**
     * Sets the root and its preceding game positions.
     *
     * # Errors
     * Returns an error when the root or any prior FEN is invalid.
     */
    set_position(fen: string, prior_fens: any): void;
}

/**
 * Compatibility entry point for consumers that do not retain an `Engine`.
 */
export function analyze_step(fen: string, depth: number, multi_pv: number, time_limit_ms: number): any;

export function engine_name(): string;

/**
 * Picks the best immediate legal move when a bounded search cannot finish.
 *
 * # Errors
 * Returns an error if `fen` is invalid.
 */
export function fallback_move(fen: string): string | undefined;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_engine_free: (a: number, b: number) => void;
    readonly analyze_step: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly engine_analyze_depth: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly engine_fallback_move: (a: number, b: number) => void;
    readonly engine_name: (a: number) => void;
    readonly engine_new: () => number;
    readonly engine_set_position: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly fallback_move: (a: number, b: number, c: number) => void;
    readonly __wbindgen_export: (a: number, b: number) => number;
    readonly __wbindgen_export2: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_export3: (a: number) => void;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export4: (a: number, b: number, c: number) => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
