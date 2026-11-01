## Framework contribution rules

- Keep framework source, filenames, examples and commit messages generic. Keep
  consumer names, machine topology and private operational instructions outside
  this public repository.
- Develop in the existing checkout, preserve unrelated work and staging, and
  checkpoint intended changes before validation. Publish only reviewed changes
  on the established fork branch; never publish private integration refs or tags.
- Fix failures from the preceding change first. Run affected existing tests;
  distinguish helper/native checks from actual browser and device acceptance.
- Framework behavior belongs here. Consumers may relocate Cargo manifests and
  package source, but must not maintain separate runtime behavior patches.
- Consumers fetch the published fork branch and record the resolved revision in
  build evidence. They must not select stale prepared copies or unpublished edits.
- Do not merge upstream, rewrite published history, install software, or deploy
  applications without the corresponding operator instruction.
- Follow AGENTS.md for framework implementation, lifecycle and validation rules.
