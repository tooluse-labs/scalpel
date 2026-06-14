# PDFBox PDFDebugger — UI Reference Notes

Apache PDFBox `PDFDebugger` is the reference tool this project benchmarks
against (see the parity discussion behind the `Full PDFBox PDFDebugger feature
parity` non-goal in `pdf-debugger-architecture.md` §3.2).

**Scope of this document:** it is a *functional / interaction* reference, **not a
visual one**. PDFBox is an old Java Swing app — its menu bar, grey split
borders, default fonts, and tree-icon styling are not worth copying; Scalpel's
themed egui shell already looks more modern. Borrow the *workflows and
information density*, render them in Scalpel's modern, dense, low-noise style.

This is reference material only. It does **not** widen the M1.0 UI shell spike
scope (see `milestone-1-ui-shell-spike.md`); each item below is tagged with the
milestone where it actually applies.

## Worth borrowing (by value)

### 1. Tree node anatomy — the highest-value borrow — *M1*

Every structure-tree row carries dense, scannable per-node information, not just
an object number. PDFBox shows, on one line:

```
Info:       (12) [3 0 R]
Creator:    WPS 演示
Type:       Catalog
Pages:      (3) [2 0 R]  /T:Pages
```

i.e. **type glyph + key + inline scalar value + child count + indirect ref
`[N 0 R]` + resolved `/T:Type`**, with COS-type icons (dict / array / string /
name / int / stream).

Scalpel's DTOs already carry all of this, so the real tree is a *rendering*
decision, not new data:

- `ObjectKind` → type glyph/badge
- `child_count` → `(12)` count
- `object: Option<ObjectId>` → `[N 0 R]`
- `ObjectValue` (scalar) → inline value for leaf nodes
- `label` → the key
- `DiagnosticSummary` on the summary → a severity badge on the row

Target the dense one-line-per-node form when real MuPDF data lands — not the
current `obj N 0 R /Name` placeholder.

### 2. Two-axis stream view — *M2*

PDFBox separates two **orthogonal** axes:

- a decode-layer **dropdown** (`Decoded (Plain Text)`) — which bytes;
- render-mode **tabs** (`Nice` / `Raw` / `Hex`) — how to display.

Do **not** flatten these into one tab strip. The correct model matches Scalpel's
own DTOs:

- `StreamMode` (decode layer): raw decrypted-compressed / decoded;
- `StreamViewMode` (presentation): Hex / Text / Nice (syntax-highlighted
  operators).

So the stream panel should be: a `Summary` tab (filters, raw/decoded size hints,
`can_decode`) **plus** a `Bytes` view with *both* a decode-layer selector and a
presentation selector, so `raw hex`, `decoded hex`, and `decoded nice` are all
expressible.

### 3. Breadcrumb / object path bar — *M1*

PDFBox shows the selected node's path (`Root/Pages/Kids/[0]/Contents`) in a top
bar. Scalpel should surface its `NodeId` / `SerializedNodeId` path the same way:
copyable, and clickable to jump back up the path (ties into the cross-reference
navigation + back/forward history in §10.2).

### 4. Tree ↔ right-panel linkage — *M1*

Selecting a tree node (e.g. `Contents [7 0 R]`) drives the right side directly to
that object's inspector / stream view. Keep tree selection as the single driver
of the inspector / stream / page-preview panels.

### 5. Content-stream operator syntax highlighting — *M5*

PDFBox's `Nice view` renders content-stream operators with syntax highlighting
(operators colored, `/Name`s in magenta, operands plain). This is the UX target
for Scalpel's post-MVP operator viewer (architecture §4.1 Rendering Diagnostics).

### 6. Page overlays + render timing — *M5*

- The `View` menu toggles page-render overlays: `Show TextStripper
  TextPositions`, `Show TextStripper Beads`, `Show Approximate Text Bounds`,
  `Show Glyph Bounds`. These are Scalpel's post-MVP text-extraction overlays
  (architecture §4.1).
- A status-bar render-timing readout (`Rendered in 81 ms`) maps to
  `RenderResult.duration_ms` / the render-profiler goal.

## Not worth borrowing

- The Swing menu bar and grey/white split borders.
- The tree's icon *look* (do borrow the *concept* of per-type glyphs/badges —
  just render them in Scalpel's modern style).
- Large blank panels with no empty-state or context (Scalpel panels should always
  show an empty/loading state).
- Hiding features in deep nested menus (`Zoom ▸` / `Rotation ▸` / `Image type ▸`)
  — Scalpel prefers right-side inspector tabs + a command palette + local toolbars.

## Mapping to Scalpel's existing design

- "Show Pages" vs "Show Internal Structure" in PDFBox = Scalpel's **Render mode**
  vs **Inspect mode** (product-shape §3 mode IA). No new focus-switch system is
  needed.
- Tree node fields ↔ `ObjectSummary` / `ObjectKind` / `child_count` / `ObjectId`
  / `ObjectValue` / `DiagnosticSummary` (architecture §5).
- Stream axes ↔ `StreamMode` × `StreamViewMode` (architecture §5.6).
