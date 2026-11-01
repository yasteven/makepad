## Framework priorities

- Validate GPU-completion behavior in browser and physical-device workflows:
  changing selection, data updates, resizing and prolonged use.
- Preserve native device services, bounded live video presentation, texture
  ownership, keyboard ownership and browser clock/layout repairs together.
- Compare upstream in an isolated integration workspace; port overlapping fixes
  once and validate before publishing changes to the established fork branch.
- Keep encoded image bytes, decoded CPU images and GPU residency independently
  bounded. Evaluate lazy image delivery and encoding using real workloads.
  Encoding alone does not reduce decoded texture size at unchanged dimensions.
