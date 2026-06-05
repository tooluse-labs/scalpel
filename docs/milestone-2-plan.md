# Milestone 2 Streams And Pages Plan

Status: required exit gate complete on 2026-06-05 for the macOS developer path
behind the existing opt-in `real-mupdf` feature. Optional stream-view polish can
continue without blocking Milestone 2.

Milestone 2 turns the M1 inspect-only debugger into a byte-and-page inspector:
raw/decoded stream chunks, stream presentation modes, page list/render preview,
and cancellation boundaries. The default workspace gate must remain MuPDF-free.

## Completed Stream And Render Slices

- Real `pdbg_stream_load` is wired to MuPDF streaming APIs:
  `pdf_open_raw_stream_number` for raw decrypted-compressed bytes and
  `pdf_open_stream_number` for decoded bytes.
- Real open options now store the configured output and decoded-stream limits
  on the document handle, so decoded-stream limits are enforced during `fz_read`
  rather than after full materialization.
- Real open options also store filter-expansion ratio and object-depth limits.
  Decoded stream reads enforce the expansion ratio against the stream
  dictionary's raw `/Length` hint during `fz_read`, and object previews stop
  before recursively printing containers beyond the configured depth.
- Real stream loading gates unauthenticated encrypted documents, cancellation,
  invalid object ids, non-stream objects, output limits, and decoded-size
  overflows with mapped `pdbg_error` values.
- Core real-mupdf tests cover raw vs decoded semantics, offset/limit paging,
  truncation, decoded-limit failure during read, and password-gated stream
  access.
- The GUI real Stream tab now consumes real `stream_load` output as bounded
  chunks from a cancellable background session task, displays a read-only stream
  view, and copies visible output through the existing Markdown egress escaping
  path.
- Stream presentation is modeled as two axes:
  - decode layer: `StreamMode::Raw` / `StreamMode::Decoded`;
  - display mode: `StreamViewMode::Hex` / `Text` / `Bytes`.
- Real `pdbg_page_render` is wired to MuPDF page rendering with bounded render
  options, max pixel/output-byte guards, owned `pdbg_image` pixels, and
  post-render grayscale/inverted RGBA transforms.
- Core real-mupdf tests cover successful first-page RGBA output, image-buffer
  ownership via accessors, and render pixel-limit failure.
- The GUI Page Preview panel now loads a bounded first-page render for real
  documents, uploads it as an egui texture, logs success/failure, and keeps the
  existing mock preview for the fake shell path.
- The GUI Page Preview panel also loads the first bounded page-list page from
  the real MuPDF page-root node through the existing child traversal API and
  displays it as a compact page strip above the preview.
- Page Preview controls cover page navigation, zoom, rotation, and texture
  invalidation keyed by page/render-option changes.
- GUI render refreshes now run as cancellable background session tasks. Render
  results carry the page/zoom/rotation key and stale results are discarded
  instead of overwriting the current preview.
- Cooperative cancellation has a C/Rust token surface. The token is atomic for
  cross-thread cancellation, stream/render calls can pass it through the shim,
  and fake plus real MuPDF tests cover `PDBG_ERROR_CANCELLED` with the document
  remaining usable afterward.
- Task-level mid-operation cancellation is covered by a real MuPDF smoke that
  cancels a large decoded stream from another thread and then reuses the same
  document successfully.

## Optional M2 Follow-Up

- Optional stream-view polish after real PDFs are exercised:
  syntax-highlighted "nice" content-stream view, richer binary/text affordances,
  and selected-byte copy beyond visible-chunk copy.

## Validation Record

Latest local validation after completing the required M2 exit gate:

```sh
PDBG_MUPDF_SOURCE_DIR=/private/tmp/xreflab-mupdf/mupdf-1.27.2-source \
sh scripts/run_m1_real_gate.sh
```

That gate covers the implemented M2 stream, page-list, render, preview-control,
and cancellation slices in addition to the M1 open/inspect baseline:

- real shim/core stream tests for raw and decoded chunks;
- decoded stream limit and filter-expansion-ratio enforcement during read;
- real GUI stream chunk loading and Hex/Text/Bytes presentation tests;
- real shim/core render tests for owned first-page RGBA pixels and pixel-limit
  enforcement;
- real GUI first-page render loading and stride-aware texture upload tests;
- real GUI page-list loading from the MuPDF page-root node;
- real GUI page navigation, zoom refresh, page-index clamping, and render-option
  texture invalidation through the background render job;
- fake and real cancellation-token clean-error paths for stream/render plus
  post-cancellation document usability checks;
- real mid-operation stream cancellation from a controller thread;
- the full default M0 local gate, proving the MuPDF-free floor still holds.

## M2 Exit Gate

M2 is complete when:

- raw and decoded stream chunks are loaded through real MuPDF with bounded
  memory and correct raw/decoded semantics;
- the GUI exposes stream summary plus cancellable bounded Hex/Text byte views
  over real documents;
- page list is populated from real MuPDF data;
- first-page render preview is populated from real MuPDF data and render output
  is copied out through owned Rust buffers before C handles are dropped;
- page preview controls can change page, zoom, and rotation without showing
  stale textures;
- cancellation returns cleanly for at least one long stream or render operation;
- the default local gate still passes without linking MuPDF.

## Non-Goals

- Full MCP transport and agent-facing stream tools. The MCP contract core can
  keep stream limits/tool visibility, but server transport remains Milestone 4.
- Content-stream operator visualization and page overlays; those remain
  post-MVP rendering diagnostics.
- Full cross-platform render benchmarking.
