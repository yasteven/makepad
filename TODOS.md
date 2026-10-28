## Operator rules — read before any work

- GitHub owner: **yasteven**. Use SSH: `git@github.com:yasteven/makepad.git`;
  upstream is `git@github.com:makepad/makepad.git`. Do not switch to HTTPS.
- Fix failures from the preceding patch first. Development and patch application
  happen on **seejn only**, under `~/1tb/see/1-c0d3/rust/`. The shared Makepad
  checkout is the `makepad` sibling of `trade` and `sols`.
- seeab uses the same suffix under `~/see`. Only pasted/cat'd deployment logs
  come from seeab; do not request saved source or report files from that host.
- Use the existing Rust tools and explicit `cd` commands. Checkpoint each patch
  before validation, including patches whose tests fail. Preserve unrelated
  staging/working changes. Delete installers only after successful validation.
- Run only affected tests on seejn; no extra application/WASM builds there.
  Native and mocked-GPU tests do not establish a browser soak result.
- Trade updates deploy on seeab. SOLS updates deploy on both machines, preserving
  the Space/non-Space split and refreshing seejn-certs. Do not auto-push, reset,
  deploy, or merge upstream from a development patch.
- Framework fixes belong here. Projects may relocate/package source for a
  build, but must not maintain separate Rust/JS behavior patches for Makepad.
  Both consumers must record and verify the same reviewed fork commit.

## Shared Makepad priorities

- Finish switching SOLS and Trade source selection, packaging and deployment to
  this sibling checkout at the same immutable commit. Reject mismatched pins.
- Confirm the GPU-completion repair in the actual headless Chrome/phone workflow:
  option selection, ticker updates, resizing and prolonged chart/chain use.
- Compare upstream on a separate integration branch after preserving this
  baseline; port overlapping fixes once and validate before changing the pin.
- High priority afterward: audit GSTA embedded/staged image payloads and add
  bounded, lazy WSTA → GSTA image delivery. Evaluate QOI against PNG/JPEG;
  independently bound encoded bytes, decoded CPU images and GPU residency.
  Encoding alone does not reduce decoded texture size at unchanged dimensions.

