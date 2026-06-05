# Milestone 1 Open And Inspect Plan

Status: planned. Starts after accepted M1.0 UI Shell Spike.

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

Recommended first implementation path: a pinned vendored MuPDF release for
reproducible development builds, `real-mupdf` off by default, with the default
fake-shim gate unchanged. If this changes, update ADR 0001 or add a new ADR
before code lands.

## Implementation Phases

### M1.1 Build And Source Policy

- Pick the MuPDF release and source strategy.
- Update `NOTICES` with the exact MuPDF source URL, version, bundled dependency
  notice path, and corresponding-source expectations for `real-mupdf` builds.
- Extend `pdbg-shim/build.rs` with a `real-mupdf` branch while preserving the
  current fake branch as the default.
- Add a local non-required command for real builds, for example:

```sh
cargo test -p pdbg-core --features real-mupdf
```

### M1.2 Minimal Real Shim

Implement only the calls needed for Open And Inspect:

- context create/drop with `fz_locks_context`;
- document open from path and fd/stream entry points already defined by the ABI;
- password failure / encrypted-document status mapping;
- document summary;
- root/trailer/xref/page tree children;
- object detail for dictionaries, arrays, scalars, indirect refs, and stream
  summary metadata;
- diagnostic list plumbing for repair warnings and format errors.

Do not implement full stream decode, page render, text extraction, search, or
MCP transport in this milestone unless needed as a small smoke fixture.

### M1.3 Rust Adapter Swap

- Add a real `Shim` implementation behind `real-mupdf`.
- Keep `FakeShim` as the default test backend.
- Ensure `pdbg-app` and `pdbg-core` reach the backend only through the existing
  `Shim` / `ShimDocument` seam.
- Keep DTO conversion behavior identical where MuPDF and FakeShim overlap.

### M1.4 Crash-Class Tests

- Malformed-PDF loop: repeatedly open damaged PDFs on the same context and
  verify clean `PDBG_ERROR_FORMAT` or equivalent mapped errors without crashes
  or poisoned MuPDF exception state.
- Locks/concurrency smoke: open and traverse multiple documents across worker
  threads with cloned MuPDF contexts and the shared lock table installed before
  any clone is created.
- Run the real shim under ASAN/UBSan where available; keep the fake-shim local
  gate as the required baseline until real MuPDF CI is stable.

### M1.5 UI Real-Data Slice

- Replace fake tree rows with real lazy object summaries.
- Render tree rows using the PDFBox-inspired anatomy documented in
  `docs/ui/pdfbox-reference-notes.md`: type badge, key/label, inline scalar,
  child count, indirect ref, and diagnostic severity marker where available.
- Add the selected `NodeId` breadcrumb/path bar.
- Drive the existing Object / Stream / Diagnostics inspector tabs from real
  object detail. Stream byte viewing remains Milestone 2; M1 may show stream
  summary metadata only.

## Exit Gate

M1 is complete when:

- `real-mupdf` can open ordinary and encrypted PDFs through the shim.
- The document summary, root/trailer/xref/page tree, and object detail panel are
  populated from real MuPDF data.
- Indirect-reference navigation works against real objects through stable
  `NodeId`s.
- The malformed-PDF loop and MuPDF locks/concurrency smoke pass.
- The default local gate still passes without linking MuPDF.
- `NOTICES` and source-offer documentation cover the enabled MuPDF build path.

## Non-Goals

- Full stream viewer and decoded byte paging.
- Page rendering.
- Text extraction and search.
- Resource-specific inspectors.
- MCP transport.
- Operator visualization and render overlays.
