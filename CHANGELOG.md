# Changelog

## 0.1.0 (unreleased)

- First release.
- Readers refuse hostile bytes instead of panicking: an SCP whose blob offset lies past the end of the file is an
  error at the header field, and the library denies `clippy::indexing_slicing`, `clippy::arithmetic_side_effects` and
  `clippy::cast_possible_truncation` (every offset sum is checked, every read goes through a bounds-checked read).
- `reference_rig::ReferenceRig::source_matrices` returns `reference_rig::MatrixError` (was `String`).
- `reference_rig::Report` counts `rigs_with_source_payload` from the rigs; `source_space_rigs` (always equal to
  `rigs`) and `rigs_with_uninterpreted_fields` are gone.
- `Error::Texture` keeps the texture catalog's error whole (with its own offset) where `MaterialLibrary` used to wrap
  it at offset 0.
- `scene::Scene::unread` no longer lists a span covering the whole SCP.
- Every public item is documented (`missing_docs` is denied).
