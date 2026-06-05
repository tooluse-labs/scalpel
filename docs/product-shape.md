# MuPDF Rust PDF Debugger Product Shape

## 1. Product Thesis

This product is a native PDF diagnostics workbench for engineers who need to
understand why a PDF behaves the way it does.

It should not be positioned as:

- a PDF editor;
- a PDF viewer;
- a PDFBox-app clone;
- a batch conversion tool;
- an Acrobat replacement.

It should be positioned as:

- a PDF object inspector;
- a rendering debugger;
- a PDF integrity and compatibility analyzer;
- a resource and stream workbench;
- a safe PDF context provider for LLM/MCP workflows.

The product promise:

> Open any difficult PDF, understand its structure, inspect its rendering
> behavior, diagnose broken resources, and expose bounded context to AI tools.

The MVP backend is MuPDF-only. Alternate PDF engines and multi-engine product
support are not part of the first product shape and require a separate ADR.

## 2. Target Users

### 2.1 Primary Users

#### PDF Engine Engineers

They debug parser, xref, stream, font, image, annotation, form, and rendering
issues.

Needs:

- object-level inspection;
- raw/decoded stream comparison;
- page resource graph;
- render operator tracing;
- damaged PDF repair visibility;
- reproducible diagnostics.

#### SDK Support Engineers

They need to answer customer reports quickly.

Needs:

- fast open and page preview;
- quick issue classification;
- exportable diagnostic bundles;
- object/page/resource search;
- screenshots and metadata summaries.

#### QA / Compatibility Engineers

They compare behavior across PDF engines and versions.

Needs:

- render snapshots;
- page timing;
- object summaries;
- regression-friendly output;
- stable diagnostic codes.

### 2.2 Secondary Users

#### Security Researchers

They inspect suspicious or malformed PDFs.

Needs:

- JavaScript discovery;
- embedded files;
- object streams;
- incremental update history;
- suspicious structure warnings.

#### LLM / RAG Engineers

They need reliable PDF extraction and bounded PDF context.

Needs:

- page text with coordinates;
- object-aware context;
- image/page rendering;
- MCP tools;
- prompt-injection-aware extraction.

## 3. Product Modes

The application should have clear modes. Modes prevent the UI from becoming a
large collection of unrelated panels.

### 3.1 Inspect Mode

Default mode.

Purpose:

- browse trailer, catalog, xref, pages, objects, dictionaries, arrays, streams;
- jump between indirect references;
- inspect raw and decoded object content.

Main layout:

```text
Object Tree | Object Inspector | Stream / Hex / Text View
```

### 3.2 Render Mode

Purpose:

- preview pages;
- inspect visual output;
- compare render settings;
- diagnose slow or broken pages.

Main layout:

```text
Page List | Page Canvas | Render Diagnostics
```

### 3.3 Resources Mode

Purpose:

- understand how a page depends on fonts, images, color spaces, XObjects,
  patterns, shadings, annotations, widgets, and optional content groups.

Main layout:

```text
Page/Resource Tree | Resource Detail | Preview / Dependencies
```

### 3.4 Content Mode

Post-MVP mode.

Purpose:

- inspect content stream operators;
- correlate operators with visual output;
- highlight bounding boxes;
- explain resource usage.

Main layout:

```text
Operator List | Page Canvas | Graphics State / Resources
```

### 3.5 Diagnose Mode

Purpose:

- summarize problems and likely causes;
- produce a support-friendly report;
- expose stable issue codes.

Main layout:

```text
Issue List | Evidence | Suggested Next Actions
```

### 3.6 AI/MCP Mode

Purpose:

- show MCP server status;
- configure file access roots;
- inspect recent tool calls;
- preview what context is sent to agents.

Main layout:

```text
MCP Tools | Access Policy | Tool Call Log / Output Preview
```

## 4. First-Run Experience

The first screen should be a functional workbench, not a marketing page.

Initial empty state:

- large open-file drop zone;
- recent files;
- allowed roots status for MCP;
- sample PDF optional, but not required.

No tutorial cards. The interface should show available actions through menus,
toolbars, shortcuts, and disabled panels.

## 5. Information Architecture

### 5.1 Top-Level Navigation

Recommended top navigation:

```text
Inspect | Render | Resources | Content | Diagnose | AI/MCP
```

Top-level modes should be exposed as a persistent top-bar segmented control,
not hidden under a `View` menu and not mixed into inspector tabs. This keeps the
mode switch visible while preserving the hierarchy: modes change the workbench
context; right-panel tabs change the selected mode's local inspector view.

MVP can ship with:

```text
Inspect | Render | Diagnose
```

Resources and Content can initially appear as disabled or hidden experimental
views.

### 5.2 Persistent Surfaces

Always visible:

- current file name;
- page selector;
- selected object id/path;
- task progress;
- warning/error count;
- memory/cache indicator.

### 5.3 Panels

Reusable panels:

- Document Tree;
- Xref Table;
- Object Inspector;
- Stream Viewer;
- Hex Viewer;
- Page Preview;
- Search Results;
- Diagnostics;
- Task Log;
- MCP Tool Calls.

The layout should support docking later, but the MVP can use fixed split panels.

## 6. Core Workflows

### 6.1 Find Why A Page Renders Incorrectly

Flow:

1. Open PDF.
2. Go to Render mode.
3. Select page.
4. Compare render settings.
5. Open page resources.
6. Inspect fonts/images/XObjects.
7. Open content stream.
8. Highlight suspicious operators after Content mode ships.
9. Export diagnostic report.

MVP support:

- open;
- render;
- inspect page object/resources manually;
- inspect streams;
- basic report.

### 6.2 Inspect A Broken Stream

Flow:

1. Search object number or click stream object.
2. View dictionary.
3. Compare raw stream vs decoded stream.
4. Switch to hex/text view.
5. See filter chain and decode errors.
6. Copy bounded excerpt.

MVP support:

- full.

### 6.3 Understand A Font Problem

Flow:

1. Select page.
2. Open resources.
3. Select font.
4. Inspect subtype, BaseFont, embedded file, Encoding, ToUnicode.
5. See warnings for missing ToUnicode or suspicious subset.
6. Correlate text extraction output with font mapping.

MVP support:

- manual object navigation.

Post-MVP:

- dedicated font inspector.

### 6.4 Analyze A Suspicious PDF

Flow:

1. Open in safe mode.
2. JavaScript disabled.
3. Diagnose mode scans object graph.
4. Show embedded files, JavaScript, actions, launch actions, incremental updates.
5. Export report.

MVP support:

- object inspection;
- basic JavaScript/form discovery if exposed by MuPDF data.

Post-MVP:

- security-focused diagnostics.

### 6.5 Give An LLM Safe PDF Context

Flow:

1. Open PDF in GUI.
2. Enable MCP for this document or allowlist root.
3. Agent calls read-only tools.
4. User sees tool call log.
5. User can inspect exactly what content was returned.

Initial MCP support, targeted for Preview 4:

- read-only tools only;
- bounded outputs;
- no write operations.

## 7. Product Editions

This does not require immediate commercial packaging, but edition boundaries
help keep scope clear.

Any closed-source or paid edition is conditional on the MuPDF licensing path.
Either the product ships under AGPL-compatible terms, or the project obtains a
commercial MuPDF license before distributing proprietary Pro, Team, or AI
features.

### 7.1 Community / Core

Features:

- object tree;
- stream viewer;
- page preview;
- basic diagnostics;
- local-only use.

### 7.2 Pro / Engineering

Features:

- render profiler;
- resource graph;
- font/image inspectors;
- diagnostic bundles;
- comparison tools;
- advanced search.

### 7.3 Team / Support

Features:

- shareable diagnostic reports;
- issue templates;
- batch triage;
- internal corpus notes;
- plugin policies.

### 7.4 AI Extension

Features:

- MCP server;
- tool call logs;
- context preview;
- local model integration hooks;
- external LLM provider adapters.

Edition names are placeholders. The important point is to avoid mixing advanced
AI/server features into the first debugger MVP.

## 8. Differentiation

### 8.1 Compared With PDFBox PDFDebugger

The first version should cover the PDFDebugger inspector backbone. PDFBox-specific
text overlays, color-space panes, flag decoders, and advanced form diagnostics
belong in post-MVP work unless they directly support core diagnostics.

Advantages to pursue:

- native performance;
- stronger rendering preview;
- render diagnostics;
- large-file lazy loading;
- modern UI;
- Rust safety boundaries;
- MCP/AI integration;
- diagnostic reports.

Avoid competing on:

- Java ecosystem integration;
- exact COS model parity;
- existing PDFBox workflow muscle memory.

### 8.2 Compared With General PDF Viewers

Advantages:

- object visibility;
- stream inspection;
- resource diagnostics;
- debug-oriented rendering controls.

Avoid competing on:

- reading experience;
- annotation editing;
- form filling;
- consumer UI polish.

### 8.3 Compared With Hex Editors

Advantages:

- PDF-aware object graph;
- decoded streams;
- page and resource correlation;
- readable diagnostics.

Avoid competing on:

- arbitrary binary editing.

## 9. UI Personality

This should feel like a professional engineering tool:

- dense but organized;
- fast keyboard navigation;
- low visual noise;
- clear split panes;
- stable object paths;
- predictable tabs and inspectors;
- no marketing-style hero sections inside the app;
- no decorative UI.

Visual priorities:

- legibility;
- line alignment;
- monospace views for PDF syntax;
- distinct severity colors;
- high-contrast selection;
- nonblocking progress states.

## 10. Command Palette

A command palette should exist early because debugger users navigate by action.

Examples:

- Open File;
- Jump To Object;
- Jump To Page;
- Search Text;
- Search Object Keys;
- Toggle Raw Stream;
- Toggle Decoded Stream;
- Copy Object Path;
- Render Current Page;
- Diagnose Document;
- Start MCP Server;
- Stop MCP Server.

The command palette reduces toolbar clutter and supports power users. MCP
commands should be hidden or disabled until the AI/MCP preview ships.

## 11. Reports And Artifacts

Diagnostic reports should be treated as a product feature, not a log dump.

MVP report:

- file hash;
- page count;
- object count;
- PDF version;
- encryption state;
- selected object details;
- diagnostics;
- render thumbnail for selected page.

Post-MVP report:

- resource graph;
- render timing;
- font/image warnings;
- incremental update summary;
- signature diagnostics;
- MCP context excerpts.

MVP reports should be exportable as:

- JSON for automation;
- Markdown for issue trackers.

Post-MVP reports should also support:

- HTML for support sharing.

## 12. Plugin And Extension Shape

Do not build a plugin system in the MVP. Keep public DTOs, reports, and MCP
schemas stable enough that future extensions can be evaluated, but do not design
or promise alternate PDF engine support in the MVP.

Future extension points:

- diagnostics providers;
- custom object inspectors;
- custom stream decoders;
- render comparison workflows;
- MCP tool providers;
- report exporters.

Extension API should consume stable Rust DTOs, not MuPDF pointers.

## 13. MCP Product Shape

MCP should be visible and auditable, not hidden.

The GUI should show:

- server status;
- transport;
- allowed roots;
- open documents available to MCP;
- recent tool calls;
- bytes returned;
- duration;
- errors;
- whether output contained untrusted PDF text.

MCP is a bridge from the debugger to agents, not the main product surface.

## 14. Safety UX

PDFs are untrusted inputs. The UI should make safety visible without becoming
annoying.

Required safety states:

- safe mode enabled by default;
- JavaScript disabled by default;
- password required;
- encrypted;
- repaired/damaged;
- external file references detected;
- embedded files detected;
- MCP output truncated;
- OCR disabled/enabled.

MVP safe mode means:

- JavaScript is not executed in GUI or MCP paths;
- OpenAction and additional-action JavaScript are not triggered automatically;
- external file references are reported but not followed;
- embedded files are reported but not extracted automatically;
- OCR is disabled unless the user explicitly enables it.

Dangerous future operations must use explicit confirmation:

- save;
- overwrite;
- redact;
- delete pages;
- run JavaScript;
- expose arbitrary file path to MCP.

## 15. MVP Product Definition

The first public MVP should be called something like:

> MuPDF Debugger Preview

MVP is the combination of Preview 1 and Preview 2 in the release path below. It
should include:

- Inspect mode;
- Render mode;
- basic Diagnose mode;
- local file open;
- lazy object tree;
- object inspector;
- raw/decoded stream viewer;
- page preview;
- object/text search;
- export basic JSON/Markdown report.

It should not include:

- editing;
- plugin system;
- alternate PDF engines or multi-engine switching;
- full MCP server;
- content operator visualization;
- team features;
- full signature validation UI.

## 16. Release Path

### Preview 1: Object Workbench

Internal alpha / first usable slice.

- Open PDF.
- Object tree.
- Object inspector.
- Raw/decoded streams.
- Basic page preview.

### Preview 2: Diagnostics Workbench

First public MVP preview.

- Diagnostics panel.
- Search.
- Report export.
- Better damaged PDF handling.

### Preview 3: Rendering Workbench

- Render settings.
- Resource summaries.
- Timing.
- Thumbnail cache.

### Preview 4: AI/MCP Workbench

- Read-only MCP server.
- Tool call log.
- Context preview.
- Allowlist policy.

### Preview 5: Advanced PDF Internals

- Content stream operators.
- Resource graph.
- Font/image inspectors.
- Incremental update viewer.

## 17. Product Success Criteria

MVP success:

- opens a 500 MB or 1 million-object stress PDF without freezing the UI;
- tree expansion is lazy and responsive;
- raw/decoded stream inspection is reliable;
- first interaction is available within 3 seconds for ordinary PDFs;
- no UI frame should block longer than 16 ms during background work in the
  normal case;
- object jumps have p95 latency below 100 ms after the document summary is
  loaded;
- page preview is fast enough for navigation on common office PDFs;
- diagnostics explain real broken PDFs better than a generic error message.

Long-term success:

- support engineers use it to answer customer PDFs;
- engine developers use it to debug rendering regressions;
- MCP tools provide useful context without leaking unsafe or unbounded data;
- the product remains a debugger, not a drifting PDF editor.

## 18. Open Product Questions

- Should the app expose multiple open documents as tabs in Preview 1, or keep
  the backend multi-document capability hidden until Preview 2?
- Should MCP be a built-in feature or a separate companion process?
- Should the object tree default to trailer/catalog/pages or xref list?
- Should the page preview always be visible in Inspect mode?
- Should corpus comparison be a future team feature or core engineering feature?
