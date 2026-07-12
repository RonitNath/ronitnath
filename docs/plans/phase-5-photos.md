# Phase 5 — Photos

Contract: 000-architecture.md §e.

Scope:
- Migrations 0031_photos, 0032_photo_variants.
- Ingest: magic-byte sniff (jpeg/png/webp), read EXIF taken_at BEFORE
  stripping, re-encode ALL variants EXIF-free (incl. GPS), thumb 320 /
  medium 1280 webp, content-hash storage
  `data/photos/{account}/{event}/{sha256}[.thumb|.medium].webp`, inline.
- Serving: dedicated authz-checked routes only (never ServeDir); attendee =
  attendance.status IN ('going','attended') or Owner.
- Photo routes: own 15 MiB RequestBodyLimitLayer nested inside global 1 MiB.
- Upload UI on /e/{token} + /my paths; per-event gallery; uploader
  soft-delete own, admin any; `photos-gc --older-than` CLI.

Acceptance: non-attendee 404s on photo routes; GPS-tagged fixture
verifiably stripped in every stored variant; 15 MiB enforced (413 over);
dedup on identical upload; gallery passes brief review + completeness audit
(empty/loading/error/upload-progress states). Sonnet leg for gallery UI.
