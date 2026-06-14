# Milestone 1 Open And Inspect Plan

Status: implemented for the macOS developer path on 2026-06-05. The opt-in
real-MuPDF local gate is `scripts/run_real_gate.sh`; the default workspace
gate remains MuPDF-free.

Milestone 1 replaces the fake backend for the first real debugger slice:
opening PDFs through MuPDF, reading document summaries, traversing the lazy COS
tree, and showing object details. Streams, rendering, search, and MCP remain in
later milestones except where small smoke coverage is needed to prove the M1
boundary.

## Committed Decisions

- Backend remains MuPDF-only.
- The default workspace and default CI stay MuPDF-free; `real-mupdf` remains
  opt-in.
- Rust bindings, generated or hand-written, target only `pdbg_shim.h`; no Rust
  binding generation over MuPDF `fz_*` / `pdf_*` headers.
- The C shim owns all MuPDF pointers and catches every MuPDF exception before
  control returns to Rust.
- M1 must add real coverage for the two M0-deferred crash-class invariants:
  `fz_try` / `fz_catch` stack integrity and `fz_locks_context` concurrency.

## Entry Decisions

These decisions must be recorded before enabling `real-mupdf` in any required
build:

- MuPDF source acquisition: pinned vendored release archive, pinned git
  submodule, or developer-only system package.
- Link mode: static or dynamic, with the corresponding AGPL source and notice
  obligations.
- Supported M1 platform matrix. macOS can be first; Linux/Windows can remain
  explicit follow-up targets if build support is not ready.
- MuPDF upgrade owner and cadence for security releases.
- Binding policy: keep checked-in raw ABI declarations or introduce generated
  bindings over `pdbg_shim.h` only.
- If generated bindings are introduced, update the ABI drift guard at the same
  time. The current script validates the checked-in raw ABI declarations against
  `pdbg_shim.h`; it must not become a stale false-green check.

Recommended first implementation path: a pinned vendored MuPDF release for
reproducible development builds, `real-mupdf` off by default, with the default
fake-shim gate unchanged. If this changes, update ADR 0001 or add a new ADR
before code lands.

M1.1 source/link/platform/binding decisions are recorded in
`docs/adr/0002-mupdf-m1-source-link-policy.md`.

## Implementation Phases

### M1.1 Build And Source Policy

- Pick the MuPDF release and source strategy.
- Update `NOTICES` with the exact MuPDF source URL, version, bundled dependency
  notice path, and corresponding-source expectations for `real-mupdf` builds.
- Extend `scalpel-shim/build.rs` with a `real-mupdf` branch while preserving the
  current fake branch as the default.
- Add a local non-required command for real builds, for example:

```sh
cargo test -p scalpel-shim --no-default-features --features real-mupdf
```

### M1.2 Minimal Real Shim

Implement only the calls needed for Open And Inspect:

- context create/drop with `fz_locks_context`;
- document open from path and fd/stream entry points already defined by the ABI;
- real open must honor `pdbg_open_options.safe_mode` and
  `disable_javascript`: JavaScript stays disabled for GUI and MCP paths, and
  OpenAction / additional actions must not auto-run during document open;
- password failure / encrypted-document status mapping. When MuPDF can report
  encryption state before authentication, expose `encrypted=true` and
  `needs_password=true` in the summary, keep tree/object traversal gated until a
  password is supplied, and map true authentication failures distinctly;
- document summary;
- root/trailer/xref/page tree children;
- object detail for dictionaries, arrays, scalars, indirect refs, and stream
  summary metadata. `StreamSummary.decoded_size_hint` may remain unknown in M1;
  do not force stream decoding only to compute it;
- diagnostic list plumbing for repair warnings and format errors.

Do not implement full stream decode, page render, text extraction, search, or
MCP transport in this milestone unless needed as a small smoke fixture.
Any `pdbg_*` entry point that is not implemented by the real shim in M1 must
return `PDBG_ERROR_UNSUPPORTED` with a filled `pdbg_error`, not crash, return
garbage, or silently route to fake data.

### M1.3 Rust Adapter Swap

- Add a real `Shim` implementation behind `real-mupdf`.
- Keep `FakeShim` as the default test backend.
- Ensure `scalpel-app` and `scalpel-core` reach the backend only through the existing
  `Shim` / `ShimDocument` seam.
- Keep DTO conversion behavior identical where MuPDF and FakeShim overlap.

### M1.4 Crash-Class Tests

- Malformed-PDF loop: repeatedly open damaged PDFs on the same context and
  verify clean `PDBG_ERROR_FORMAT` or equivalent mapped errors without crashes
  or poisoned MuPDF exception state.
- Repair-success fixture: open a damaged-but-repairable PDF, verify it succeeds,
  emits a `repair_warning` diagnostic, sets `repaired_or_damaged=true`, and
  populates `parsed_object_count` consistently with the repaired object graph
  (possibly lower than `xref_size`).
- Damaged fixtures must follow the M0 fixture policy: license-clean synthetic
  or redistributable files only, no private customer PDFs, and a clear note for
  every regression fixture explaining the bug class it exercises.
- Locks/concurrency smoke: open and traverse multiple documents across worker
  threads with cloned MuPDF contexts and the shared lock table installed before
  any clone is created.
- Run the locks/concurrency smoke under TSan where available; this is the first
  milestone with real MuPDF shared state for `fz_locks_context` to protect.
- Run the real shim under ASAN/UBSan where available; keep the fake-shim local
  gate as the required baseline until real MuPDF CI is stable.
- Any real MuPDF callback that can cross into Rust, such as progress or cancel
  callback plumbing, must route through the existing `catch_ffi_callback` panic
  boundary before returning to C.

### M1.5 UI Real-Data Slice

- Replace fake tree rows with real lazy object summaries.
- Render tree rows using the PDFBox-inspired anatomy documented in
  `docs/ui/pdfbox-reference-notes.md`: type badge, key/label, inline scalar,
  child count, indirect ref, and diagnostic severity marker where available.
- Add the selected `NodeId` breadcrumb/path bar.
- Drive the existing Object / Stream / Diagnostics inspector tabs from real
  object detail. Stream byte viewing remains Milestone 2; M1 may show stream
  summary metadata only.
- Add a lightweight large-real-PDF responsiveness smoke using a license-clean
  fixture or synthetic stress generator. The smoke must open through real MuPDF,
  request only bounded lazy child pages/object details, exercise first
  interaction, several node expansions, and at least one indirect-reference
  jump, and verify the model/UI path does not materialize the whole object tree.
  Record coarse timings against the Product Success Criteria budgets in
  `docs/product-shape.md` Section 17; a repeatable
  cross-platform performance harness remains a later milestone.

## Exit Gate

M1 is complete when:

- `real-mupdf` can open ordinary and encrypted PDFs through the shim, including
  the unauthenticated encrypted summary state when MuPDF exposes it.
- The document summary, root/trailer/xref/page tree, and object detail panel are
  populated from real MuPDF data.
- Indirect-reference navigation works against real objects through stable
  `NodeId`s.
- The malformed-PDF loop, damaged-but-repairable fixture, and MuPDF
  locks/concurrency smoke pass.
- The large-real-PDF responsiveness smoke passes without whole-tree
  materialization or visible UI freezes. If the platform cannot provide stable
  timing in the M1 gate, the smoke must still record timings and fail on
  unbounded traversal or materialization.
- The locks/concurrency smoke has TSan coverage where supported by the platform
  toolchain.
- The default local gate still passes without linking MuPDF.
- `NOTICES` and source-offer documentation cover the enabled MuPDF build path.

## Validation Record

Local validation was run on macOS with MuPDF 1.27.2 built from the pinned source
tree recorded in ADR 0002:

```sh
SCALPEL_MUPDF_SOURCE_DIR=/private/tmp/scalpel-mupdf/mupdf-1.27.2-source \
sh scripts/run_real_gate.sh
```

That gate covers:

- real shim open/summary/fd/malformed/repair/encrypted tests;
- real core open/summary/tree/detail/stream-summary/concurrency tests;
- real app GUI-model tests, including bounded large-PDF responsiveness smoke;
- real clippy for `scalpel-shim`, `scalpel-core`, and `scalpel-app`;
- real headless `--pdf` smoke;
- the full default M0 local gate to prove the MuPDF-free floor still holds.

The current local macOS toolchain is stable-only, so real MuPDF TSan is not
claimed as locally executed. TSan coverage remains a platform hardening gate
for environments that provide nightly Rust plus a TSan-built MuPDF dependency;
the M1 real gate still runs the cloned-context concurrency smoke without TSan.

## Non-Goals

- Full stream viewer and decoded byte paging.
- Page rendering.
- Text extraction and search.
- Resource-specific inspectors.
- MCP transport.
- Operator visualization and render overlays.
- Full cross-platform performance benchmarking.
