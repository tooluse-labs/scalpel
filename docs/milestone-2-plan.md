# Milestone 2 Streams And Pages Plan

Status: in progress on 2026-06-05. The stream byte slice is implemented for the
macOS developer path behind the existing opt-in `real-mupdf` feature; page
rendering and cancellation are not implemented yet.

Milestone 2 turns the M1 inspect-only debugger into a byte-and-page inspector:
raw/decoded stream chunks, stream presentation modes, page list/render preview,
and cancellation boundaries. The default workspace gate must remain MuPDF-free.

## Completed Stream Slice

- Real `pdbg_stream_load` is wired to MuPDF streaming APIs:
  `pdf_open_raw_stream_number` for raw decrypted-compressed bytes and
  `pdf_open_stream_number` for decoded bytes.
- Real open options now store the configured output and decoded-stream limits
  on the document handle, so decoded-stream limits are enforced during `fz_read`
  rather than after full materialization.
- Real stream loading gates unauthenticated encrypted documents, cancellation,
  invalid object ids, non-stream objects, output limits, and decoded-size
  overflows with mapped `pdbg_error` values.
- Core real-mupdf tests cover raw vs decoded semantics, offset/limit paging,
  truncation, decoded-limit failure during read, and password-gated stream
  access.
- The GUI real Stream tab now consumes real `stream_load` output as bounded
  chunks, displays a read-only stream view, and copies visible output through
  the existing Markdown egress escaping path.
- Stream presentation is modeled as two axes:
  - decode layer: `StreamMode::Raw` / `StreamMode::Decoded`;
  - display mode: `StreamViewMode::Hex` / `Text` / `Bytes`.

## Remaining M2 Work

- Page list populated from real MuPDF data rather than only object-tree page
  roots.
- Page render preview using the existing `pdbg_page_render` ABI and bounded
  render options.
- Render-result ownership and image-buffer accessor tests on the real shim.
- Cancellation plumbing for long stream/render operations, including a clean
  `PDBG_ERROR_CANCELLED` path and no poisoned MuPDF state after cancellation.
- Optional stream-view polish after real PDFs are exercised:
  syntax-highlighted "nice" content-stream view, richer binary/text affordances,
  and selected-byte copy beyond visible-chunk copy.

## Validation Record

Latest local validation after the stream slice:

```sh
PDBG_MUPDF_SOURCE_DIR=/private/tmp/xreflab-mupdf/mupdf-1.27.2-source \
sh scripts/run_m1_real_gate.sh
```

That gate currently covers the implemented M2 stream slice in addition to the
M1 open/inspect baseline:

- real shim/core stream tests for raw and decoded chunks;
- decoded stream limit enforcement during read;
- real GUI stream chunk loading and Hex/Text/Bytes presentation tests;
- the full default M0 local gate, proving the MuPDF-free floor still holds.

## M2 Exit Gate

M2 is complete when:

- raw and decoded stream chunks are loaded through real MuPDF with bounded
  memory and correct raw/decoded semantics;
- the GUI exposes stream summary plus bounded Hex/Text byte views over real
  documents;
- page list and first-page render preview are populated from real MuPDF data;
- render output is copied out through owned Rust buffers before C handles are
  dropped;
- cancellation returns cleanly for at least one long stream or render operation;
- the default local gate still passes without linking MuPDF.

## Non-Goals

- Full MCP transport and agent-facing stream tools. The MCP contract core can
  keep stream limits/tool visibility, but server transport remains Milestone 4.
- Content-stream operator visualization and page overlays; those remain
  post-MVP rendering diagnostics.
- Full cross-platform render benchmarking.
