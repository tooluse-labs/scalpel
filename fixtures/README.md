# Fixture Policy

Milestone 0 fixtures are synthetic and license-clean.

Do not add customer PDFs, downloaded PDFs, copyrighted samples, or files copied
from bug reports unless their license and redistribution terms are explicit and
compatible with this repository.

Fixture rules:

- prefer tiny synthetic PDFs that exercise one contract at a time;
- document the purpose of each fixture next to the file;
- keep malicious or damaged samples minimal and deterministic;
- never include passwords, embedded credentials, or private document content;
- add regression fixtures only with a short note describing the bug and expected
  behavior.

Real-world damaged PDF corpora are out of scope for M0. They need a separate
license review and storage policy before inclusion.
