# Frozen Ronit runtime subject guard

`owner=RonitNath/ronitnath#2`.

Actual native production image manifest
`ghcr.io/ronitnath/ronitnath-app@sha256:b1cbe94cfcb43e500c61485f6ee3b6a7ba7c44962c633d229c34faa3e96b214a`,
local image SHA `619bc050cdec3cfe593b32a905c35a8612d8753aa861517ca3456e4bd5cbd912`,
native APP_VERSION `1e97e2db71f2393ebb9a8a1d19aa0b268d0e31ad`. The original full source history is
not present on canonical main; its recovery remains tracked in issue2. This bounded source-owned
guard uses the verified immutable complete application artifact without rebuilding its UI or state.

All two native OIDC callbacks require fixed `https://auth.isoastra.com`, verified email and explicit
immutable Ronit/support subject metadata. Denials use the frozen native BetterCall APIError shape
(name/status/statusCode/body) to return403 instead of an uncaught callback500. All five native currentPrincipal copies select the persisted
federated `accountId` and require the protected subject before yielding any application principal.
Unknown subjects cannot inherit operator permission by matching email or reusing an old federated
cookie. Local guest/customer permission remains in its existing native authority. Legacy Ronit native
accounts with a subject-placeholder email remain authorized by immutable subject, preserving historical
bootstrap operator identity. The patch refuses any unexpected count/layout and parses every changed
native JS module before writing; six auth-bearing files change, with static UI/public assets preserved.

Run `./build.sh` on native Linux Docker, then validate the complete candidate's health/public page on
loopback using the actual native environment. Do not print or save environment secrets. `deploy.py`
checks the live baseline, stores a mode0600 encrypted-state compose backup, replaces only its immutable
image selector and applies only the web service. SFO/NYC use the same frozen artifact. No user/account,
password, mailbox or business data is changed. UI qualification includes a fresh real unassigned IdP
callback denial, viewed screenshot and owner/support native account preservation. SCIM positive
application provisioning remains unsupported until separately implemented; no intern personal-site grant.
