# AGPL Corresponding Source Offer

Status: distribution gate

This repository is licensed as `AGPL-3.0-only`. The default development build
does not distribute a network service and does not bundle MuPDF. Any distributed
binary, hosted network service, or package that links or bundles the
`real-mupdf` build must provide corresponding source for the exact artifact that
users receive or interact with.

## Required Offer Surface

Network builds must expose a prominent source link at:

```text
/source
```

Desktop, CLI, or other non-network binary builds must ship the same offer in
release notes and in the binary/package metadata. If the binary has an About or
diagnostics surface, it must include the source offer there too.

## Required Contents

The source offer must provide, or link directly to, an archive or repository
snapshot containing:

- the exact Scalpel source revision used for the deployed artifact;
- generated files needed to rebuild the deployed artifact;
- build scripts, lockfiles, dependency manifests, and CI configuration;
- the exact MuPDF source archive/version used by `real-mupdf`;
- any local patches applied to MuPDF or other bundled dependencies;
- bundled dependency notices, including MuPDF notices and app asset licenses;
- instructions sufficient to rebuild the distributed artifact from source.

The offer must identify the deployed source revision and MuPDF version in
machine-readable text. A minimal `/source` response is:

```json
{
  "license": "AGPL-3.0-only",
  "source_revision": "<git-sha>",
  "source_archive": "https://example.invalid/releases/<version>/source.tar.gz",
  "mupdf_version": "1.27.2",
  "mupdf_source": "https://casper.mupdf.com/downloads/archive/mupdf-1.27.2-source.tar.gz",
  "notices": "https://example.invalid/releases/<version>/NOTICES"
}
```

## Release Checklist

Before enabling a distributed network MCP server or shipping a `real-mupdf`
binary:

- replace placeholders in the `/source` response with release-specific URLs;
- verify the source archive includes this repository, generated files, and
  bundled notices;
- verify the MuPDF source archive URL and checksum match the linked binary;
- verify the binary/package includes `LICENSE` and `NOTICES`;
- verify a clean machine can rebuild the artifact from the source archive;
- record the source-offer URL in the release notes.

For source-only development milestones, this document is the compliance gate.
For shipped artifacts, the release is blocked until the checklist is completed.
