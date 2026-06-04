# Milestone 0 Build-Order Plan

> Backlog source of truth for Milestone 0 of the MuPDF Rust PDF Debugger.
> Companion to `pdf-debugger-architecture.md` §13 (Milestone 0 acceptance
> checklist). Task IDs (`T0.1` … `T6.1`) map 1:1 to backlog issues.

## Committed decisions (do not re-litigate)

- **MuPDF-only**, **AGPL-3.0**, **hand-written C shim over bindgen-generated raw
  bindings** (no `mupdf-rs`/`mupdf-sys`), **egui** desktop UI.

## Current Status (2026-06-04)

Last verified locally with:

- `cargo fmt --check`;
- `cargo test --workspace`;
- `cargo clippy --workspace -- -D warnings`;
- `python3 scripts/check_pdbg_shim_abi_snapshot.py`;
- `sh scripts/test_fz_try_gate.sh`.

Completed:

- **T0.1–T0.5** scaffold/core ABI substrate: workspace crates, frozen
  `pdbg_shim.h`, fake C shim, checked-in raw bindings, ABI drift script, and
  Rust RAII wrappers/accessors for context, document, buffer, image, node-list,
  and text-page handles.
- **T0.7** scaffold-level `fz_try` static gate with good/bad fixtures.
- **T1.1–T1.6** pure-Rust DTO/config contract surface: core identifiers,
  `RenderRequest`, `TextRequest`, schema constants, stable `SerializedNodeId`
  JSON, stable diagnostic/resource strings, egress escaping, capability gating,
  and safe-mode defaults.
- **T2.0–T2.4, T2.7** fake-shim-backed wire conversion surface: `Shim` /
  `ShimDocument`, document summary, children, object detail, stream, render, and
  text extraction operations; enum/discriminant guards; diagnostic and
  stream-summary conversion; FFI string/byte copying with interior-NUL text; and
  node-token registry tests including unknown-token fallback.

Partial:

- **T0.8** local green gate exists, but no checked-in CI workflow marks jobs
  required yet.
- **T2.5** capability logic exists; real app/MCP feature hiding is still pending.
- **T2.6** text span byte copying is covered; full coordinate-normalization
  golden coverage is still pending.

Not started:

- **T0.6**, **T3.1–T3.4**, **T4.1–T4.6**, **T5.1–T5.4**, **T5.3**, and **T6.1**.

## Two load-bearing principles (they decide the whole order)

1. **M0 runs entirely on a fake shim; real MuPDF is deferred to Milestone 1.**
   All M0 acceptance items are *contracts* (ABI shape, wire↔DTO conversions,
   golden JSON, limits, locks, panic policy, fd ownership, allowlist, artifact
   store, egress, capability gating). Every one is exercisable behind a
   `FakeShim`. The default CI matrix keeps the `real-mupdf` cargo feature **OFF**.
2. **The only thing on the critical path that must be "real" is the frozen
   `pdbg_shim.h` ABI header, and bindgen runs over *that header only* — never the
   MuPDF `fz_*`/`pdf_*` headers.** Pointing bindgen at MuPDF headers silently
   drags the entire MuPDF source tree + toolchain onto the critical path and
   breaks the deferral. **This is the #1 trap.**

## Crate layout (revised per review)

```
xreflab/  (cargo virtual workspace)
├─ crates/pdbg-shim            RAW ABI ONLY: frozen pdbg_shim.h, fake C impl,
│                             build.rs (cc + bindgen), checked-in raw bindings
│                             + drift check. No Rust safe types, no DTO logic.
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
3. **bindgen output is checked-in + drift-checked** — CI regenerates and diffs
   the committed `bindings.rs`; contributors don't need an identical local
   bindgen/clang to hold the ABI line.
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
| **P0** Trunk | Workspace → **freeze `pdbg_shim.h`** → fake C shim → build.rs (cc + bindgen + drift) → safe newtypes (in `pdbg-core`) → **panic-into-C policy decision** → **`fz_try` static gate (scaffold)** → green CI floor | start |
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
  explicit and marked **append-only ABI**. Single source of truth for C, bindgen,
  and Rust. *(defines the wire surface P2 pins)* — deps: T0.1
- **T0.3** **Fake C shim** (`pdbg_shim_fake.c`): implement every §7.4 symbol as a
  fake — handle lifecycle with matching drops, bounded fake decode loop returning
  `PDBG_ERROR_LIMIT` mid-decode, fd-dup on `open_fd`, `pdbg_node_children`
  returning fake `pdbg_dict_entry` lists with `path_token`s, a Rust-callback hook,
  `pdbg_fill_error`/`pdbg_map_error` + no-context error path. Feature-gated `fake`
  (default) vs `real-mupdf` (off). *(C build/compile smoke; backs most contracts)*
  — deps: T0.2
- **T0.4** **build.rs**: `cc`-compile the fake `.c` → static lib; bindgen over
  `pdbg_shim.h` **only** → committed `bindings.rs` snapshot; `sys` re-export; a
  CI regen-and-diff script fails on drift. *(generated-bindings drift check)*
  — deps: T0.3
- **T0.5** **Safe newtypes over raw handles** *(in `pdbg-core`)*:
  `PdbgDoc/PdbgContext/PdbgBuffer/PdbgImage/PdbgNodeList/PdbgTextPage` over the
  generated opaque pointers, `Drop` calling the matching `pdbg_*_drop`, accessor
  wrappers that copy bytes/strings into owned Rust before returning; documented
  `!Send`/`!Sync` per §6.2; compiles against the **generated** bindgen types
  (not a hand-typed copy, so the drift check stays load-bearing). — deps: T0.4
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
  `cargo test` (workspace), C build/compile smoke, bindings drift check (T0.4),
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
  golden tests (incl. `javascript_disabled`) + `DIAGNOSTIC_SCHEMA_VERSION`.
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

- **T3.1** Opaque-handle accessor sanitizer tests (valid borrowed access before
  cleanup; copy-before-drop; use-after-cleanup caught under ASAN/UBSan; no leaks).
  — deps: T0.5, T2.4
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
  before any cloned context); `parking_lot::Mutex<NonNull<PdbgDoc>>`,
  cache-of-owned-outputs, documented unsafe `Send`/`!Sync`. FakeShim with a
  shared fake store modeling cross-context global state. — deps: T1.1, T0.5
- **T4.2** Concurrency smoke over the shared fake store, **TSan-clean** (multiple
  sessions + worker threads through the scheduler; no race on lock/callback
  path). — deps: T4.1, T3.4
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
- **T5.3** Finalize fuzz + ASAN/UBSan + **TSan** CI jobs (cargo-fuzz over
  open/traversal/decode/DTO-conversion FakeShim-backed; C shim ASAN/UBSan; TSan
  over the T4.2 harness + callback path; tiny corpus OK; uses T0.6 profile).
  **Finalized last among CI** so it covers all earlier code. — deps: T0.8, T4.2,
  T3.1, T3.3, T0.6

### P6 — Green-gate convergence

- **T6.1** Mark all CI jobs required on the default branch; one `cargo test` + CI
  run executes the **entire contract baseline green against the fake shim (no
  libmupdf linked)**; confirm every §13 item covered and the egui shell launches
  headlessly. — deps: T2.6, T2.2, T2.7, T3.1, T3.2, T3.3, T4.2, T4.5, T4.6, T5.2,
  T5.3, T5.4

## Critical path (longest chain → M0 exit)

```
T0.1 workspace → T0.2 freeze ABI header → T0.3 fake C → T0.4 bindgen+drift
  → T0.5 safe newtypes → T2.0 Shim/FakeShim seam → T2.1 enum conversion
  → T2.7 node-token registry → T2.2 diagnostic wire → T6.1 green gate
```

Everything else fans out from this spine. Real MuPDF is not on it.

## Sequencing traps (do NOT get these wrong)

1. **Freeze `pdbg_shim.h` (T0.2) before any conversion or fake C.** It's the
   single source of truth for C/bindgen/Rust; writing conversions first forces a
   rewrite of every P2/P3 golden. Lock enum discriminants up front + add the
   renumber-detecting guard (T2.1).
2. **Point bindgen at `pdbg_shim.h` only, never MuPDF headers** — else the whole
   MuPDF tree + toolchain lands on the critical path.
3. **The Shim/FakeShim trait (T2.0) is the single swap point** — if anything
   reaches past it into bindgen types, M1's real-MuPDF swap stops being a drop-in.
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
  smoke, **generated-bindings drift check**.
- `fz_try` static gate fires on the bad fixture, passes the fake sources, wired
  as a required gate.
- Governance landed + CI-asserted: MuPDF build ADR, AGPL `LICENSE` +
  corresponding-source path + `NOTICES`, fixture policy README.
- Safe-mode defaults config-represented + unit-tested.
- Capability gating drives panels / MCP tools (logic + real consumer).
- `SerializedNodeId` golden JSON; **node-token registry** tests (incl.
  unknown-token fallback); diagnostic-code golden strings; enum conversions +
  discriminant guard; diagnostic/stream wire conversions; FFI string/interior-NUL;
  text-coordinate goldens.
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
