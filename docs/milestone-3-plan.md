# Milestone 3 Search And Diagnostics Plan

Status: implemented on 2026-06-06. Milestone 2 is complete for the required
macOS real-mupdf developer path; M3 uses the same opt-in `real-mupdf` feature
and keeps the default workspace gate MuPDF-free. `scripts/run_m3_local_gate.sh`
runs the default M3 focused checks and then the M0 local gate; real MuPDF
coverage remains in `scripts/run_real_gate.sh`.

Milestone 3 turns the byte-and-page inspector into a searchable diagnostics
workbench: real text extraction, object search over the lazy tree, text search
over bounded extracted text, and a document-level diagnostics surface suitable
for basic report export. MCP transport, resource-specific inspectors, and
content-stream operator visualization remain later milestones.

## Committed Boundaries

- Search is an application-layer feature. The C shim exposes text extraction,
  but it does not need dedicated search entry points unless profiling later
  proves Rust-side search too slow.
- Object search must use the existing lazy node model and bounded child pages.
  It must not materialize a whole large tree just to answer a query.
- Text search is built on bounded `pdbg_page_extract_text` results and cached
  page text. Extracted text is untrusted egress and must reuse the existing
  clipboard/report escaping path.
- Diagnostics output must carry `diagnostic_schema_version` when serialized.
  UI cards can remain concise, but JSON/Markdown report data must use stable
  diagnostic codes.
- The existing real-MuPDF opt-in gate remains the integration path. The default
  M0 gate must keep passing without MuPDF.

## Implementation Phases

### M3.1 Real Text Extraction

- Initial slice: real `pdbg_page_extract_text` is wired through MuPDF structured
  text, returning owned line spans with UTF-8 text, untrusted markers,
  cooperative cancellation, and bounded character/block copying.
- Implement real `pdbg_page_extract_text` using MuPDF structured-text output.
- Return owned `pdbg_text_page` / `pdbg_text_span` data copied out before MuPDF
  handles are dropped.
- Preserve the normalized top-left page-space coordinate contract used by
  `TextSpan.bbox`.
- Enforce `pdbg_text_options.max_chars` and `max_blocks` during extraction or
  while copying spans, returning `PDBG_ERROR_LIMIT` before unbounded growth.
- Honor cooperative cancellation through the existing cancel token and map abort
  to `PDBG_ERROR_CANCELLED`.
- Add fake and real tests for span text, coordinates, limits, cancellation, and
  post-cancellation document usability.
- The real gate must include a positioned-text golden fixture that pins
  top-left page-space coordinates for a known glyph span, including CropBox or
  rotation normalization, interior-NUL/lossy-text handling, and `max_chars`
  truncation behavior.

### M3.2 Object Search

- Initial slice: core object search walks lazy `ShimDocument` child pages with
  per-node page bounds, max-depth/max-node/max-result limits, and stable hits for
  object refs, dictionary keys, name previews, labels, and scalar previews.
- GUI slice: the left document panel now runs bounded lazy object search from
  the open session, displays hit counts/truncation, and routes hits through the
  existing object navigation and Back/Forward history without materializing the
  full tree.
- Add a Rust search model for object number, dictionary key, name object, and
  scalar preview queries.
- Search bounded lazy pages from the current root/trailer/xref/page tree without
  whole-tree materialization.
- Return stable search hits containing display label, matched field, optional
  `ObjectId`, and optional `NodeId` for navigation.
- Wire GUI search results to indirect-reference navigation and Back/Forward
  history.
- Add tests proving large fake/real trees stay bounded while still finding
  visible and explicitly expanded matches.

### M3.3 Text Search

- Initial slice: bounded text search now runs over cached `TextPage` extraction
  results, with explicit page/byte cache budgets, max-page/max-result bounds,
  page-level extraction errors, bbox/untrusted propagation, and GUI search
  workers that can be cancelled while results jump the preview to the hit page.
- Add bounded text-search over `TextPage` caches.
- Page extraction should be demand-driven and cancellable; broad document
  search must report partial progress rather than freezing the UI.
- Cache entries must have an explicit memory or page-count budget, and the large
  document smoke must record a coarse search timing bound rather than allowing
  unbounded extraction.
- Search hits must include page index, matched excerpt, span bbox when available,
  and untrusted-marker propagation.
- GUI results should jump to the page preview and select the hit; page-overlay
  drawing remains post-MVP unless needed for a small highlight smoke.

### M3.4 Diagnostics And Basic Reports

- Initial slice: diagnostics are consolidated into a filterable core model, GUI
  diagnostics support severity/code filtering plus JSON and Markdown copy
  export, text-search page errors are promoted into document diagnostics, and
  the Markdown report covers summary, selected object, diagnostics, and bounded
  object/text search hits with Markdown escaping.
- Consolidate document, object, stream, render, and text diagnostics into a
  document-level diagnostics model with severity/code filtering.
- Extend the real MuPDF/Rust open-error surfaces to emit the diagnostic codes
  used by that model, including missing/broken xref entries where MuPDF exposes
  repair context, stream decode failures carried on bounded stream buffers,
  Rust-synthesized encryption password failures for failed opens,
  JavaScript-disabled safety notices, and existing repair warnings.
- Add JSON diagnostic payload export using
  `diagnostics_payload_to_json_string`.
- Add the Markdown report builder used by the GUI/export path, then add a
  bounded Markdown report for summary, selected object, diagnostics, and search
  hits. Markdown output must use the existing egress escaping rules.
- Keep repair/error reporting visible for damaged PDFs, including the M1
  repair-success fixture.

## Exit Gate

M3 is complete when:

- real MuPDF text extraction returns owned spans with top-left page-space
  coordinates, bounded memory, and clean cancellation;
- object search finds object numbers, dictionary keys, names, and scalar
  previews without whole-tree materialization, with a test that proves the
  search touches only bounded child pages unless the user explicitly expands
  more of the tree;
- text search finds bounded excerpts across extracted real pages, remains
  cancellable, and stays inside the configured text-cache budget;
- GUI search results navigate to real objects/pages and preserve Back/Forward
  history;
- the diagnostics panel supports severity/code filtering and emits JSON with
  `diagnostic_schema_version`;
- the real gate includes diagnostic-emission fixtures for at least
  `stream_decode_failure` from a malformed stream buffer and
  Rust-synthesized `encryption_password_failure` from a wrong-password open;
  missing/broken xref diagnostics remain best-effort where MuPDF exposes enough
  repair context;
- a bounded Markdown diagnostics report escapes untrusted PDF text;
- the default local gate still passes without linking MuPDF, and the opt-in
  real gate plus manual `real-mupdf` workflow covers the M3 slices.

## Non-Goals

- Full MCP transport and agent tools.
- OCR.
- Resource-specific font/image/color-space inspectors.
- Content-stream operator visualization and syntax-highlighted Nice view.
- Page overlay rendering beyond small search-hit smoke coverage.
- Full cross-platform search-performance benchmarking.
