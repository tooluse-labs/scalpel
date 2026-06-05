# ADR 0002: MuPDF Source and Link Policy for Milestone 1

Status: Accepted for Milestone 1
Date: 2026-06-05

## Decision

Milestone 1 uses MuPDF 1.27.2 under the AGPL-3.0 license.

The selected upstream source is the open-source MuPDF 1.27.2 release, announced
in the official release history on 2026-02-20. The source archive used for
developer builds is:

```text
https://casper.mupdf.com/downloads/archive/mupdf-1.27.2-source.tar.gz
```

The `real-mupdf` build remains opt-in. The default workspace continues to build
and test only the fake shim.

For M1 developer builds, the preferred source layout is an extracted pinned
MuPDF source tree outside the default workspace, referenced by
`PDBG_MUPDF_SOURCE_DIR`. A checked-in `third_party/mupdf-1.27.2` tree may be
introduced later only together with regenerated notices and corresponding-source
publication steps.

The M1 link mode is static by default. `pdbg-shim/build.rs` accepts
`PDBG_MUPDF_LINK_MODE` and `PDBG_MUPDF_LIBS` for local experiments, but the
documented product path is a pinned source release plus MuPDF's static
`libmupdf.a` and `libmupdf-third.a` libraries.

The initial supported M1 platform is macOS for developer integration. Linux is
the next required platform before any release build. Windows remains an explicit
follow-up until the MuPDF build and sanitizer story is validated there.

The binding policy does not change in M1.1: Rust declarations remain checked-in
raw ABI declarations for `pdbg_shim.h`. If generated bindings are introduced in
a later M1 patch, generation must target `pdbg_shim.h` only, never MuPDF
`fz_*` or `pdf_*` headers, and the ABI drift guard must be updated in the same
change.

## Build Contract

The default build must remain MuPDF-free:

```sh
cargo test --workspace
```

The real MuPDF build is enabled explicitly:

```sh
PDBG_MUPDF_SOURCE_DIR=/path/to/mupdf-1.27.2-source \
cargo test -p pdbg-shim --no-default-features --features real-mupdf
```

If MuPDF was built into a non-default location, set:

```sh
PDBG_MUPDF_INCLUDE_DIR=/path/to/mupdf/include
PDBG_MUPDF_LIB_DIR=/path/to/mupdf/build/release
```

`PDBG_MUPDF_LIBS` defaults to `mupdf,mupdf-third`. Additional platform or
third-party libraries may be added locally while M1.1/M1.2 settles the final
static link line.

## Rationale

This keeps the M0 green floor intact while making the real integration path
concrete enough for M1.2. The project gets a reproducible upstream version and
license posture without forcing every contributor or CI job to build MuPDF.

Static linking matches the intended single-binary debugger shape and makes the
AGPL corresponding-source obligation explicit: any distributed real-MuPDF build
must publish the application source, shim source, build scripts, exact MuPDF
source archive, notices, and local patches.

## Upgrade Policy

The MuPDF upgrade owner is the project maintainer for the current milestone.
During active M1 work, check upstream MuPDF security and release notes at least
monthly and before every public binary distribution.

Every MuPDF upgrade must update:

- this ADR or a successor ADR;
- `NOTICES`;
- the real-shim smoke tests;
- malformed and repair fixture results;
- sanitizer and TSan notes for supported platforms.
