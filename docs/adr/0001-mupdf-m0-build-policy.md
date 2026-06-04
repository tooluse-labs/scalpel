# ADR 0001: MuPDF Build, Vendoring, and Upgrade Policy for M0

Status: Accepted for Milestone 0
Date: 2026-06-04

## Decision

Milestone 0 does not vendor, link, download, or bindgen against MuPDF.

The default workspace builds only the frozen `pdbg_shim.h` ABI, the fake C shim,
and the Rust contract surface. The `real-mupdf` path stays disabled until
Milestone 1, and bindgen must continue to target only `pdbg_shim.h`, never
MuPDF's `fz_*` or `pdf_*` headers.

## Rationale

M0 is a contract baseline. Pulling MuPDF into the default build would make the
green floor depend on an external source tree, C toolchain details, and licensing
decisions that M0 is not meant to settle.

Keeping MuPDF outside M0 makes the boundary explicit:

- `pdbg-shim` owns the C ABI shape.
- `pdbg-core` owns safe Rust DTO conversion and scheduler contracts.
- The fake shim proves the ABI and lifecycle contracts without libmupdf.

## Milestone 1 Entry Criteria

Before enabling `real-mupdf`, the project must decide:

- source acquisition: vendored source archive, checked submodule, or system
  package for developer-only builds;
- link mode: static or dynamic, with the corresponding license obligations;
- platform matrix: macOS, Linux, and any Windows support target;
- upgrade cadence and owner for MuPDF security releases;
- generated binding policy: still bind only `pdbg_shim.h`; no direct generated
  Rust bindings to MuPDF internals.

## Upgrade Policy

Every MuPDF upgrade after M1 must include:

- the upstream version and source URL in `NOTICES`;
- rebuilt real-shim smoke tests;
- malformed-PDF regression loop results;
- review of new or changed license notices;
- confirmation that `real-mupdf` remains off in default M0-style contract CI.
