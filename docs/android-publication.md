# Preparing an Android fork for publication

The Android work derives from [ludovic111/horizon](https://github.com/ludovic111/horizon).
Keep the original MIT license/copyright, third-party notices, vendor licenses and
contribution attribution. The Android README identifies this fork and does not
claim upstream parity results certify Android. The remote fork is [Vasysik/horizon-android](https://github.com/Vasysik/horizon-android).
The local `android` branch preserves upstream history; `origin` is the fork
and `upstream` is the original repository.

## Reviewable source snapshot

```sh
python -m unittest discover -s tests -p test_android_source_export.py
python tools/export_android_source.py
```

The ZIP and adjacent SHA-256 report are written under `.local/android/public/`.
`SOURCE-MANIFEST.json` records every exported file and its hash. The exporter
selects source directories, excludes caches, saves, build products, game inputs,
APKs and signing material, and blocks recognized credential patterns. This is
a bounded credential scan, not a guarantee against every possible secret format.
It rejects symbolic links and unexpected large files instead of following them.
Generated tables and Mojang downloads remain private in `.local/`.

Upstream GitHub Actions are retained under `.github/upstream-workflows/` with
`.disabled` extensions in the export. The only active exported workflow is a
manual Android source check: read-only token, no self-hosted runner, no release,
deployment or telemetry upload. Original files remain intact in the working
directory; this transformation applies to the publication archive.

## Restore a real upstream relationship

The original source directory lacked `.git`. Its files were compared by Git
blob hashes against 309 first-parent upstream revisions. The selected base,
`d65282fb0f0fd7ba57876aef87dc4c6c2b5fa04b`, matched 3106 of 3147 shared source
files; the 41 differing existing files correspond to port work and fixes.
The actual upstream commit graph was restored, not recreated with invented
original authors. Evidence is recorded in [android-upstream.json](android-upstream.json).

Develop on `android` and push to `origin`. Fetch upstream separately and review
merges; do not replace a working Android engine with newer upstream files by
copying a snapshot. The fork's original `main` is preserved. Before publishing,
run `python tools/check_public_tree.py --history`; `.githooks` supplies the
commit guard. Agent commits carry their own Co-Authored-By trailer.

The inactive upstream workflows and the sole manual Android check now also
apply to the Android Git branch, rather than only to ZIP exports. Original
Bedrock textures are supplied through `tools/android_bedrock_controls.py` to
private `.local/android/bedrock-controls.zip`, with pinned Git blob checks and
the original Mojang LICENSE.md. The PNGs and generated ZIP never enter Git.

## Binary releases need a separate packaging step

Current APKs are local debuggable development builds with a private development
signing key and generated `engine-data.zip`. The repository rule forbids
publishing prepared Minecraft resources, and the source export never includes
this ZIP or an APK. Before a public APK release, design a resource preparation
path that respects that boundary, use a publisher-owned release key, disable
debugging/development control, choose the fork's application ID and update
endpoints, and establish the account/sign-in configuration. Keep signing keys
and account tokens outside the source tree. Do not upload the local development
APK as an automatic public release just because source export checks pass.

The Android fork still shares upstream service URLs in parts of the desktop
code. They are not replaced with invented fork services. Public branding,
repository ownership, publisher identity and production endpoints must be real
values chosen by the publisher.
