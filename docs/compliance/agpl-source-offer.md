# AGPL Source Offer Scaffold

Status: M0 scaffold

This repository is licensed as `AGPL-3.0-only`.

M0 does not distribute a network service and does not bundle MuPDF. If a future
build exposes pdbg over a network transport, the product must provide a prominent
corresponding-source offer for the exact running version.

The source-offer path for future network builds is:

```text
/source
```

That endpoint or equivalent UI action must provide:

- this repository's source for the deployed revision;
- generated files needed to rebuild the deployed artifact;
- build scripts and dependency manifests;
- MuPDF source and notices if `real-mupdf` is linked or bundled;
- any local patches applied to bundled dependencies.

The source offer must be verified before enabling a network MCP server in a
distributed build.
