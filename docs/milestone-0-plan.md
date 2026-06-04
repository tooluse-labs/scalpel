# Milestone 0 Build-Order Plan

> Backlog source of truth for Milestone 0 of the MuPDF Rust PDF Debugger.
> Companion to `pdf-debugger-architecture.md` §13 (Milestone 0 acceptance
> checklist). Task IDs (`T0.1` … `T6.1`) map 1:1 to backlog issues.

## Committed decisions (do not re-litigate)

- **MuPDF-only**, **AGPL-3.0**, **hand-written C shim over checked-in raw Rust
  ABI bindings** guarded against `pdbg_shim.h` (no `mupdf-rs`/`mupdf-sys`),
  **egui** desktop UI.

## Current Status (2026-06-04)

Last verified locally with:

- `cargo fmt --check`;
- `cargo test --workspace`;
- `cargo clippy --workspace -- -D warnings`;
- `cargo run -p pdbg-app --quiet`;
- `python3 scripts/check_pdbg_shim_abi_snapshot.py`;
- `python3 scripts/check_notices.py`;
- `sh scripts/test_fz_try_gate.sh`;
- `sh scripts/run_m0_fuzz_smoke.sh`.

Completed:

- **T0.1–T0.5** scaffold/core ABI substrate: workspace crates, frozen
  `pdbg_shim.h`, fake C shim, checked-in raw bindings, structural ABI drift
  script covering enum values, concrete and opaque struct field order/types,
  required `#[repr(C)]`, callback typedefs, and extern function signatures, plus
  Rust RAII wrappers/accessors for context, document, buffer, image, node-list,
  and text-page handles.
- **T0.7** scaffold-level `fz_try` static gate with good/bad fixtures.
- **T0.8** checked-in CI skeleton: `.github/workflows/m0.yml` defines contract,
  C ASAN/UBSan, TSan, and fuzz-smoke jobs; `scripts/run_m0_local_gate.sh` runs
  the stable local gate without linking MuPDF.
- **T0.6** panic-into-C policy: workspace profiles explicitly use
  `panic = "unwind"` and `pdbg-core` exposes a single `catch_ffi_callback`
  boundary helper that catches Rust panics from future `extern "C"` callbacks,
  maps them to `PDBG_ERROR_GENERIC`, and fills `pdbg_error`.
- **T1.1–T1.6** pure-Rust DTO/config contract surface: core identifiers,
  `RenderRequest`, `TextRequest`, schema constants, stable `SerializedNodeId`
  JSON, stable diagnostic/resource strings, diagnostic payload JSON including
  `diagnostic_schema_version`, egress escaping, capability gating, and safe-mode
  defaults.
- **T2.0–T2.4, T2.7** fake-shim-backed wire conversion surface: `Shim` /
  `ShimDocument`, document summary, children, object detail, stream, render, and
  text extraction operations; enum/discriminant guards; document-summary field
  coverage; diagnostic, `StringBytes`, and stream-summary conversion; FFI
  string/byte copying with interior-NUL text; and node-token registry tests
  including unknown-token fallback.
- **T2.5** capability gating consumers: `pdbg-app` records app feature gates and
  `pdbg-mcp` exposes tool visibility / `UNSUPPORTED` gating for structure,
  stream, render, text, and artifact tools.
- **T2.6** text-coordinate golden: fake text extraction fixes top-left page-space
  coordinates and contract tests pin page index, bbox, and untrusted span
  handling.
- **T3.3** decode-time limit contract: a configured low
  `max_decoded_stream_bytes` returns `PDBG_ERROR_LIMIT` during fake decoded-stream
  loading before a `pdbg_buffer` is materialized.
- **T3.2** `pdbg_document_open_fd` ownership contract: success and post-dup
  failure paths both leave the caller's original fd usable; the fake shim closes
  only the duplicated fd it owns.
- **T3.4** Rust-to-C callback panic boundary: the fake C shim exposes a
  test-only callback invoker, Rust `extern "C"` callbacks route through
  `catch_ffi_callback`, caught panics return `PDBG_ERROR_GENERIC`, and `pdbg_error`
  is filled before control returns to C.
- **T3.1** opaque-handle accessor lifetime contract: node-list, object-detail,
  stream-buffer, render-image, and text-page accessors copy borrowed C data into
  owned Rust DTOs before the opaque handles/document are dropped; sanitizer
  execution is wired by **T5.3**.
- **T4.1** `DocumentSession` and fake lock/store wiring: sessions serialize
  document tasks through a per-document `std::sync::Mutex`, cache owned summary
  output, `PdbgDoc` is documented `Send`/not `Sync`, and `FakeShim` installs a
  root fake lock context before opening documents.
- **T4.2** concurrency smoke: multiple `DocumentSession`s are driven from worker
  threads through `run_task`, sharing the fake lock/store and asserting task
  entry/completion counts; cloned handles to the same `DocumentSession` also
  prove only one task enters the document critical section at a time; the actual
  TSan job remains part of **T5.3**.
- **T4.3** MCP allowlist contract: roots and request paths are canonicalized,
  URL-like paths and canonicalization failures are rejected, accepted paths must
  be path-component descendants of a configured root, and symlink / `..` escapes
  are rejected.
- **T4.4** MCP input validation contract: document ids, object ids, child ranges,
  stream limits, output byte limits, page indexes, render dimensions, pixel caps,
  and rotations are bounded by explicit `McpInputLimits`.
- **T4.5** MCP artifact store contract: artifact references use unguessable
  random ids, are scoped by session/client, preserve media type and image
  dimensions, honor per-read byte-limit truncation, expire by TTL, and evict by
  LRU under the configured byte cap.
- **T4.6** headless app-state smoke: `pdbg-app` constructs the summary/tree/detail
  /output panel state over `FakeShim` + `DocumentSession`, wires safe-mode
  defaults, capability gates, Markdown egress escaping, and runs a command-loop
  smoke without opening a real window/GPU surface.
- **T5.1** MuPDF build/vendoring/linking ADR: M0 keeps MuPDF unbundled,
  unlinked, and un-bindgened; `real-mupdf` is off, and any future generated
  binding path must target `pdbg_shim.h` only, with M1 entry and upgrade
  criteria documented.
- **T5.2** AGPL compliance scaffold: `LICENSE`, `NOTICES`, future network
  source-offer path, and `scripts/check_notices.py` covering MuPDF placeholders
  plus all Cargo.lock workspace packages.
- **T5.4** fixture policy: synthetic-only fixture README plus a tiny
  license-clean minimal PDF seed under `fixtures/synthetic/`.
- **T5.3** heavy CI scaffold: checked-in C ASAN/UBSan and C/Rust TSan jobs plus a
  deterministic fake-shim contract-smoke job (kept under the historical
  `fuzz-smoke` name) covering traversal, decode limits, DTO/egress contracts,
  wire conversions, callback panic mapping, and concurrency smoke.
- **T6.1** M0 exit gate: local `scripts/run_m0_local_gate.sh` exercises the full
  fake-shim contract baseline without libmupdf, `pdbg-app` launches headlessly,
  CI jobs are checked in, and `docs/ci/required-jobs.md` lists the jobs to mark
  required on `main`.

Not started:

- None.

Deferred to Milestone 1:

- Real MuPDF malformed-PDF loop tests for `fz_try`/`fz_catch` stack integrity.
- Real `fz_locks_context` callback wiring and a genuinely racy MuPDF shared-store
  target under TSan.
- Native egui window/GPU smoke beyond the M0 headless app-state check.
- True coverage-guided fuzzing; M0's `fuzz-smoke` job is deterministic contract
  smoke, not a fuzzer.

## Two load-bearing principles (they decide the whole order)

1. **M0 runs entirely on a fake shim; real MuPDF is deferred to Milestone 1.**
   All M0 acceptance items are *contracts* (ABI shape, wire↔DTO conversions,
   golden JSON, limits, locks, panic policy, fd ownership, allowlist, artifact
   store, egress, capability gating). Every one is exercisable behind a
   `FakeShim`. The default CI matrix keeps the `real-mupdf` cargo feature **OFF**.
2. **The only thing on the critical path that must be "real" is the frozen
   `pdbg_shim.h` ABI header.** M0 uses checked-in raw Rust ABI declarations and a
   structural drift check against that header; it does **not** run bindgen. If M1
   later adopts generated bindings, they must target `pdbg_shim.h` only — never
   the MuPDF `fz_*`/`pdf_*` headers. Pointing any generator at MuPDF headers
   silently drags the entire MuPDF source tree + toolchain onto the critical path
   and breaks the deferral. **This is the #1 trap.**

## Crate layout (revised per review)

```
xreflab/  (cargo virtual workspace)
├─ crates/pdbg-shim            RAW ABI ONLY: frozen pdbg_shim.h, fake C impl,
│                             build.rs (cc), checked-in raw bindings +
│                             structural drift check. No Rust safe types, no
│                             DTO logic.
├─ crates/pdbg-core           Safe newtypes over raw handles (Drop), Shim trait
│                             + FakeShim, wire↔DTO conversions, node-token
│                             registry, decode-time limits, egress escaping,
│                             DocumentSession/scheduler/lock wiring, capability
│                             gating, safe-mode config, all DTOs/identifiers.
├─ crates/pdbg-contract-tests  Cross-crate golden / contract / concurrency / fuzz
│                             tests + the synthetic fixture corpus. (A real test
│                             crate — a root tests/ dir would NOT run under a
│                             virtual workspace.)
├─ crates/pdbg-app            egui binary: HEADLESS app-state smoke only
│                             (panel construction + FakeShim command loop). No
│                             real window/GPU at M0.
└─ crates/pdbg-mcp            MCP tool-contract core: allowlist, artifact store,
                             tool input/output schema. No transport / server
                             lifecycle at M0.
```

Dependency direction: `pdbg-shim` → `pdbg-core` → {`pdbg-app`, `pdbg-mcp`};
`pdbg-contract-tests` depends on all of them.

## Calibrations applied (from review)

1. **`pdbg-contract-tests` crate, not root `tests/`** — a top-level `tests/` dir
   is not auto-run by `cargo test --workspace` under a virtual workspace (no root
   package). Cross-crate tests live in a real test crate.
2. **`pdbg-shim` = raw sys/ABI only; safe newtypes + conversions live in
   `pdbg-core`** — keeps the `*-sys`/safe split clean so the M1 real-MuPDF swap
   stays a drop-in and the sys boundary doesn't get muddied by conversion logic.
3. **raw ABI declarations are checked-in + structurally drift-checked** — CI
   compares `pdbg_shim.h` against committed `raw.rs` for enum values, struct
   layouts, callback typedefs, and function signatures; contributors do not need
   bindgen/clang to hold the ABI line in M0.
4. **The `fz_try` static gate is scaffold-level at M0** — the fake shim has no
   real `fz_try`/`fz_catch` macros, so M0 ships a script + good/bad fixtures;
   Milestone 1 activates it against the real C shim and supplements it with
   MuPDF-backed malformed-PDF loop tests.
5. **egui headless smoke tests app-state, not a window/GPU** — panel
   construction + FakeShim-driven command loop only; a real native-window smoke
   is deferred so CI doesn't go brittle on platform graphics.
6. **`pdbg-mcp` is a tool-contract core, not a server** — allowlist + artifact
   store + tool I/O schema only; no full MCP server lifecycle at M0.
7. **C7b node-token registry is a shared prerequisite, not a golden-test
   appendage** — it backs `ObjectSummary`, `DiagnosticSummary.node`, and shim
   reverse lookup, so it sits on the T2 critical path (see Sequencing traps).

## Phases (✅ = can run in parallel with P0 from day one)

| Phase | Goal | Starts after |
|---|---|---|
| **P0** Trunk | Workspace → **freeze `pdbg_shim.h`** → fake C shim → build.rs (cc) + raw ABI drift check → safe newtypes (in `pdbg-core`) → **panic-into-C policy decision** → **`fz_try` static gate (scaffold)** → green CI floor | start |
| **P1** Pure-Rust DTO/serialization ✅ | DTOs, schema versions, SerializedNodeId golden, diagnostic-code strings, egress escaping, capability/safe-mode types. **Zero FFI — needs only the empty workspace.** | `T0.1` |
| **P5** Governance docs + heavy CI ✅ | MuPDF build/vendoring ADR (decision only), AGPL compliance + NOTICES + §13 stub, fixture policy, fuzz/ASAN/UBSan/**TSan** jobs | `T0.1` (docs) |
| **P2** wire↔DTO conversions | Shim/FakeShim seam → enum conversion → **node-token registry** → diagnostic/stream/string/text-coord conversions | `T0.5` + `T1.1` |
| **P3** FFI boundary safety | accessor sanitizer, `open_fd` ownership, decode-time limits, callback panic boundary | `T0.5`, ∥ P2 |
| **P4** Threading / MCP cores / egui | DocumentSession + lock wiring → TSan-clean concurrency smoke, MCP allowlist (B3), artifact store, headless egui shell | `T1.1`, ∥ P2/P3 |
| **P6** Green-gate convergence | Mark all CI jobs required; full contract baseline green against the fake shim | after P2/P3/P4/P5 |

## Task DAG

All tasks are `needs_real_mupdf: false`.

### P0 — Trunk (`pdbg-shim`, + `pdbg-core` newtypes, + CI)

- **T0.1** Cargo virtual workspace: 5 crates, each builds an empty lib/bin with a
  trivial passing test; toolchain pin, rustfmt/clippy config, `.gitignore`;
  `cargo build && cargo test` green. *(§13: workspace + baseline substrate)*
  — deps: —
- **T0.2** **Freeze `pdbg-shim/include/pdbg_shim.h`**: transcribe every
  type/enum/struct/function from arch §7.2–§7.4 verbatim; enum discriminants
  explicit and marked **append-only ABI**. Single source of truth for C and Rust
  raw declarations. *(defines the wire surface P2 pins)* — deps: T0.1
- **T0.3** **Fake C shim** (`pdbg_shim_fake.c`): implement every §7.4 symbol as a
  fake — handle lifecycle with matching drops, bounded fake decode loop returning
  `PDBG_ERROR_LIMIT` mid-decode, fd-dup on `open_fd`, `pdbg_node_children`
  returning fake `pdbg_dict_entry` lists with `path_token`s, a Rust-callback hook,
  `pdbg_fill_error`/`pdbg_map_error` + no-context error path. Feature-gated `fake`
  (default) vs `real-mupdf` (off). *(C build/compile smoke; backs most contracts)*
  — deps: T0.2
- **T0.4** **build.rs**: `cc`-compile the fake `.c` → static lib; committed
  `raw.rs` exposes the C ABI; a CI drift script fails if `pdbg_shim.h` and
  `raw.rs` disagree on enum values, struct field order/types, callback typedefs,
  or extern function signatures. *(raw ABI drift check)*
  — deps: T0.3
- **T0.5** **Safe newtypes over raw handles** *(in `pdbg-core`)*:
  `PdbgDoc/PdbgContext/PdbgBuffer/PdbgImage/PdbgNodeList/PdbgTextPage` over the
  raw opaque pointers, `Drop` calling the matching `pdbg_*_drop`, accessor
  wrappers that copy bytes/strings into owned Rust before returning; documented
  `!Send`/`!Sync` per §6.2; compiles against the checked-in raw ABI types guarded
  by T0.4. — deps: T0.4
- **T0.6** **Panic-into-C policy decision** + crate attribute: workspace
  `panic="abort"` *or* a documented `catch_unwind` wrapper shape at every
  `extern "C"` callback entry. Workspace-wide profile — fix it **before** the
  callback tests (T3.4) and the fuzz/ASAN/TSan profile (T5.3). — deps: T0.1
- **T0.7** **`fz_try` static gate (scaffold-level)**: script scanning `pdbg-shim`
  C sources, fails on `return`/`goto`/`longjmp` inside `fz_try`/`fz_always`
  (documented `break` allowed); good + bad fixtures prove it fires. **M0 is
  conservative scaffolding** (fake shim has no real `fz_try`); M1 activates it on
  the real shim + adds MuPDF-backed malformed-PDF loop tests. — deps: T0.3
- **T0.8** **CI skeleton**: `cargo fmt --check`, `clippy -D warnings`,
  `cargo test` (workspace), C build/compile smoke, raw ABI drift check (T0.4),
  `fz_try` gate (T0.7); green on the wired-but-mostly-empty tree. — deps: T0.4, T0.7

### P1 — Pure-Rust DTO + serialization (✅ parallel, zero FFI; `pdbg-core`)

- **T1.1** Core identifiers + DTO structs + schema-version consts (all of §5/§11:
  `NodeId`/`SerializedNodeId`/`ObjectId`, `ObjectSummary`/`Detail`/`Value`,
  `DocumentSummary`/`Permissions`/`SafetyState`, `StreamChunk`/`StreamSummary`,
  `RenderRequest`/`Result`/`ColorMode`, `TextPage`/`Span`, `ChildPage`/`Range`,
  `DiagnosticSummary`, `MuPdfCapabilities`, `PUBLIC`/`DIAGNOSTIC_SCHEMA_VERSION`).
  **Fan-in base for nearly every later task.** — deps: T0.1
- **T1.2** `SerializedNodeId` public JSON encode + golden tests (lowercase segment
  tags, `{num,gen}`, `schema_version`, never exposes `path_token`). — deps: T1.1
- **T1.3** `DiagnosticCode`/severity stable lowercase-string serialization +
  golden tests (incl. `javascript_disabled`) + diagnostic payload JSON emitting
  `diagnostic_schema_version`.
  — deps: T1.1
- **T1.4** Egress escaping (plaintext/Markdown/HTML/JSON) + unit tests per §9.
  — deps: T1.1
- **T1.5** `MuPdfCapabilities` + `CancellationCapability` types + pure gating
  logic (capability set → hide/disable/UNSUPPORTED). — deps: T1.1
- **T1.6** Safe-mode default config + unit tests (JS off, OCR opt-in, no URL,
  bounded resource defaults §6.4; maps to `pdbg_open_options`). — deps: T1.1

### P2 — wire↔DTO conversions + node-token registry (`pdbg-core`)

- **T2.0** **Shim/Backend trait + `FakeShim`** — the single M1 swap seam. All
  later contract tasks reach MuPDF only through this trait. — deps: T0.5, T1.1
- **T2.1** C wire enum conversions + **append-only discriminant guard test**
  (`pdbg_object_kind`/`value_kind`/`resource_group`/`color_mode`/`repair_policy`/
  `diagnostic_severity`/`diagnostic_code`; stable `javascript_disabled`/
  `xobjects` strings). — deps: T2.0
- **T2.4** FFI string/byte conversion tests (UTF-8/NUL, nullable display fields,
  `pdbg_text_span.text` copied by `text_len` with interior NUL preserved,
  `bytes`/`byte_len` authoritative over lossy `decoded_text`). — deps: T2.0
- **T2.7** **Node-token registry** *(critical path; missing from all 3 planners)*:
  per §7.3, convert `pdbg_dict_entry` → `ObjectSummary` reconstructing the public
  `NodeId` from parent + dict key / array index (`index = ChildRange.offset +
  list position`); record `pdbg_node_id`/`path_token` for reverse lookup; resolve
  diagnostic `pdbg_node_id` through the registry. Tests: dict child, array child,
  reverse lookup, diagnostic-node conversion, **unknown-token fallback** (keep
  diagnostic + object, omit `node`, never leak the token). — deps: T2.0, T1.2
- **T2.2** Diagnostic wire conversion (all carriers: `pdbg_diagnostic` +
  `pdbg_diagnostic_list` (NULL = empty), from dict-entry/document-summary/
  object-detail/stream-buffer/render-image), resolving optional node via T2.7.
  — deps: T2.1, T1.3, T2.7
- **T2.3** Stream-summary wire conversion (`pdbg_stream_summary` from
  `pdbg_object_detail_out`: filters, raw/decoded size hints, `can_decode`,
  `image_preview_available`). — deps: T2.1
- **T2.5** Capability gating wired into `pdbg-app` panels / MCP tools (real
  consumer; returns `PDBG_ERROR_UNSUPPORTED`) — completes the product half of the
  capability item. — deps: T1.5, T2.1
- **T2.6** Text-coordinate normalization + golden tests (top-left page space,
  CropBox-else-MediaBox, page rotation applied, viewer zoom / render rotation
  excluded, no out-of-page clamping; §7.3). — deps: T2.4

### P3 — FFI boundary safety contracts (`pdbg-core` + `pdbg-shim` fake bodies; ∥ P2)

- **T3.1** Opaque-handle accessor lifetime tests (valid borrowed access before
  cleanup; copy-before-drop into owned Rust DTOs; use-after-cleanup/leak checks
  run under the ASAN/UBSan jobs finalized by T5.3). — deps: T0.5, T2.4
- **T3.2** `pdbg_document_open_fd` ownership tests (success keeps caller's original
  fd; failure closes only the shim's dup; returned doc owns the dup; no-POSIX-fd
  fallback documented). *Depends only on the sys/shim layer — corrected from the
  thin-slice plan that wrongly chained it after the MCP allowlist.* — deps: T0.5
- **T3.3** Decode-time limit-enforcement tests (`PDBG_ERROR_LIMIT` returned
  **during** decode, before full materialization; zero-limit = product default).
  — deps: T0.5, T2.3
- **T3.4** Rust-to-C callback panic-policy boundary tests (matching T0.6: caught
  & mapped under `catch_unwind`, or process-level abort test under `panic=abort`).
  — deps: T0.6, T0.5

### P4 — Threading / MCP cores / egui (`pdbg-core`, `pdbg-mcp`, `pdbg-app`; ∥ P2/P3)

- **T4.1** `DocumentSession` + per-doc task queue + **`fz_locks`-equivalent
  lock-callback wiring** installed at `context_new` (root lock context exists
  before any cloned context); M0 uses dependency-free `std::sync::Mutex` around
  the open document, cache-of-owned-outputs, documented unsafe `Send`/`!Sync`.
  FakeShim has a shared fake store modeling cross-context global state. — deps:
  T1.1, T0.5
- **T4.2** Concurrency smoke over the shared fake store (multiple sessions +
  worker threads through the scheduler for shared-store accounting) plus
  same-session cloned-handle serialization (load-bearing max-active assertion);
  executed under TSan by T5.3. Real MuPDF lock-callback races are deferred to
  M1. — deps: T4.1, T3.4
- **T4.3** MCP allowlist (Blocker B3): canonicalize roots at load + request path,
  **path-component-descendant (not `starts_with`)**, reject canonicalization
  failures, reject `..` + symlink escape, no-URL. *(`pdbg-mcp`)* — deps: T1.1
- **T4.4** MCP input validation unit tests (ids, bounds, max output bytes).
  — deps: T4.3
- **T4.5** MCP artifact store contract (unguessable ids, session/client scoping,
  byte-limit truncation, TTL + LRU, media type + dimensions; `pdf_render_page`
  ref + `pdf_get_artifact` retrieval goldens over a fake image). — deps: T1.1, T4.3
- **T4.6** **Headless egui app-state smoke** (`pdbg-app`): four-panel layout
  constructed over FakeShim, fake summary/tree displayed, safe-mode config (T1.6)
  + capability gating (T2.5) + egress (T1.4) wired; CI runs **app-state/command
  loop only — no real window/GPU**. — deps: T4.1, T2.5

### P5 — Governance docs + heavy CI (✅ parallel from day one)

- **T5.1** MuPDF build/vendoring/linking/bindgen/OS-matrix/upgrade-policy ADR —
  **decision only**, no vendoring/linking at M0 (§7.8). — deps: T0.1
- **T5.2** AGPL-3.0 compliance scaffold: `LICENSE`, corresponding-source path,
  regenerated `NOTICES` enumerating MuPDF + every bundled dep with license +
  AGPL-compat confirmation, §13 source-offer stub for any future network MCP,
  CI lint asserting NOTICES references each known dep. — deps: T5.1
- **T5.4** Malicious/damaged fixture policy README + tiny **synthetic** seed
  fixtures (where fixtures live, license-sensitive exclusion, how regressions are
  added). A documentation deliverable — don't defer to "when we add fixtures".
  — deps: T0.1
- **T5.3** Finalize deterministic fuzz-smoke + ASAN/UBSan + **TSan** CI jobs
  (fake-shim-backed traversal/decode/DTO/egress/callback/concurrency smoke; C
  shim ASAN/UBSan; TSan over the T4.2 harness; uses T0.6 profile). **Finalized
  last among CI** so it covers all earlier code. — deps: T0.8, T4.2, T3.1, T3.3,
  T0.6

### P6 — Green-gate convergence

- **T6.1** Mark all CI jobs required on the default branch; one `cargo test` + CI
  run executes the **entire contract baseline green against the fake shim (no
  libmupdf linked)**; confirm every §13 item covered and the egui shell launches
  headlessly. — deps: T2.6, T2.2, T2.7, T3.1, T3.2, T3.3, T4.2, T4.5, T4.6, T5.2,
  T5.3, T5.4

## Critical path (longest chain → M0 exit)

```
T0.1 workspace → T0.2 freeze ABI header → T0.3 fake C → T0.4 raw ABI drift
  → T0.5 safe newtypes → T2.0 Shim/FakeShim seam → T2.1 enum conversion
  → T2.7 node-token registry → T2.2 diagnostic wire → T6.1 green gate
```

Everything else fans out from this spine. Real MuPDF is not on it.

## Sequencing traps (do NOT get these wrong)

1. **Freeze `pdbg_shim.h` (T0.2) before any conversion or fake C.** It's the
   single source of truth for C and Rust raw ABI declarations; writing conversions first forces a
   rewrite of every P2/P3 golden. Lock enum discriminants up front + add the
   renumber-detecting guard (T2.1).
2. **Do not run bindgen in M0; if generated bindings are introduced in M1, point
   them at `pdbg_shim.h` only, never MuPDF headers** — else the whole MuPDF tree
   + toolchain lands on the critical path.
3. **The Shim/FakeShim trait (T2.0) is the single swap point** — if anything
   reaches past it into raw ABI types, M1's real-MuPDF swap stops being a drop-in.
4. **Decide the panic-into-C policy (T0.6) early** — a workspace-wide profile
   that retroactively invalidates the other family of tests if flipped late; must
   precede T3.4 and the T5.3 sanitizer profile.
5. **Install lock-callback wiring (T4.1) before the concurrency smoke (T4.2) and
   TSan** — the root lock context must exist before any cloned context, or TSan
   over an unlocked scheduler is meaningless (the B2 race).
6. **Test limits DURING decode (T3.3)**, not after a full buffer is materialized,
   or the decompression-bomb contract passes vacuously.
7. **Capability gating has two halves** — pure logic (T1.5) and a real consumer
   (T2.5 / T4.6). Testing the logic alone under-satisfies it.
8. **MCP allowlist needs path-component-descendant checks, not `starts_with`**
   (Blocker B3), plus symlink + `..` cases.
9. **Plain CI loop (T0.8) + `fz_try` gate (T0.7) first; heavy sanitizer/fuzz
   (T5.3) last.** Standing up ASAN/UBSan/TSan before their targets exist yields
   empty jobs that look green then flip red.
10. **Keep the link MuPDF-free:** `real-mupdf` stays OFF in the default CI matrix;
    if it leaks on, CI starts requiring a MuPDF toolchain and the green floor
    breaks for everyone.

## M0 exit gate

- `cargo build` + `cargo test` green across the workspace against the fake shim
  (no libmupdf linked); `real-mupdf` OFF in default CI.
- CI baseline green: fmt, clippy `-D warnings`, workspace tests, C build/compile
  smoke, **raw ABI drift check**.
- `fz_try` static gate fires on the bad fixture, passes the fake sources, wired
  as a required gate.
- Governance landed + CI-asserted: MuPDF build ADR, AGPL `LICENSE` +
  corresponding-source path + `NOTICES`, fixture policy README.
- Safe-mode defaults config-represented + unit-tested.
- Capability gating drives panels / MCP tools (logic + real consumer).
- `SerializedNodeId` golden JSON; **node-token registry** tests (incl.
  unknown-token fallback); diagnostic-code golden strings and diagnostic payload
  schema version; enum conversions + discriminant guard; diagnostic/StringBytes/
  stream wire conversions; FFI string/interior-NUL; text-coordinate goldens.
- Boundary safety over the fake shim: accessor valid/invalid-after-cleanup under
  ASAN/UBSan; `open_fd` ownership; decode-time `PDBG_ERROR_LIMIT`; callback panic
  policy.
- Egress escaping; MCP allowlist + artifact store.
- Concurrency smoke clean under TSan; fuzz + ASAN/UBSan/TSan jobs green (tiny
  corpus OK).
- Headless egui app-state smoke launches over FakeShim.
- **All of the above CI jobs marked required on the default branch (T6.1).**

## Deferred to Milestone 1 (explicitly)

Vendoring/pinning the MuPDF tree; the `build.rs` branch that compiles+links
libmupdf; real `fz_*`/`pdf_*` bodies behind each `pdbg_*`; the MuPDF-backed
malformed-PDF `fz_try`/`fz_catch` integrity loop (replaces the M0 static-gate
scaffold); MuPDF-backed concurrent open/traversal over a real `fz_locks_context`
(replaces the fake-store concurrency smoke). The danger to manage: keep the
`FakeShim` ABI-faithful so the M1 swap stays a drop-in.
