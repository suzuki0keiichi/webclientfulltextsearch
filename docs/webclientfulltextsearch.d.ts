/* tslint:disable */
/* eslint-disable */

export class FullTextSearch {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Adds documents to the index.
     *
     * Each document must have an `id` field. Other fields are indexed or stored
     * according to the schema.
     */
    add(docs: any): void;
    /**
     * Exports the full index for IndexedDB persistence.
     */
    export_index(): any;
    /**
     * Imports a previously exported index.
     */
    static import_index(data: any): FullTextSearch;
    /**
     * Creates a new search index with the given schema.
     *
     * ```js
     * const index = new FullTextSearch({
     *     fields: [
     *         { name: "title", kind: "text", weight: 2.0 },
     *         { name: "body",  kind: "text", weight: 1.0 },
     *         { name: "date",  kind: "stored" },
     *     ],
     *     ngram_size: 3,
     * });
     * ```
     */
    constructor(schema: any);
    /**
     * Removes a document by id (lazy deletion).
     */
    remove(id: string): void;
    /**
     * Searches the index.
     *
     * Query syntax:
     * - `hello world` — AND (both must match)
     * - `hello OR world` — OR
     * - `NOT hello` or `-hello` — exclude
     * - `"exact phrase"` — phrase match
     * - `title:hello` — field-specific
     * - `(a OR b) c` — grouping
     */
    search(query: string, options: any): any;
    /**
     * Number of active (non-deleted) documents.
     */
    readonly count: number;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_fulltextsearch_free: (a: number, b: number) => void;
    readonly fulltextsearch_add: (a: number, b: any) => [number, number];
    readonly fulltextsearch_count: (a: number) => number;
    readonly fulltextsearch_export_index: (a: number) => [number, number, number];
    readonly fulltextsearch_import_index: (a: any) => [number, number, number];
    readonly fulltextsearch_new: (a: any) => [number, number, number];
    readonly fulltextsearch_remove: (a: number, b: number, c: number) => void;
    readonly fulltextsearch_search: (a: number, b: number, c: number, d: any) => [number, number, number];
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
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
