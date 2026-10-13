# Releases and app updates

Every new version merged to `main` is published by `.github/workflows/release.yml`
as a GitHub Release (`v0.1.41`, ...) with a signed Windows installer and
`latest.json`. The app checks that file at launch and every 6 hours
(`src/store/updater.ts`) and offers "Update and restart".

The signature is the Tauri updater's own key pair (free, made locally). It is
not a Windows code-signing certificate: that one is paid and only removes the
SmartScreen "unknown publisher" prompt.

## One-time setup

1. Make the key pair on your PC (Node.js is enough):

   ```bash
   npx @tauri-apps/cli signer generate -w ~/.tauri/ai-management.key
   ```

   Pick a password when asked. This writes `ai-management.key` (private) and
   `ai-management.key.pub` (public). Keep a backup of the private key and the
   password: without them, installed apps cannot take updates any more.
2. In the repository on GitHub: Settings → Secrets and variables → Actions →
   New repository secret:
   - `TAURI_SIGNING_PRIVATE_KEY`: the whole content of `ai-management.key`
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: the password
3. Settings → Actions → General → Workflow permissions: "Read and write
   permissions" (the workflow creates releases).
4. Put the content of `ai-management.key.pub` in `plugins.updater.pubkey` in
   `src-tauri/tauri.conf.json`. The public key is safe to commit.

The private key never goes in the repository. The release workflow stops with
an error while the public key or the secret is missing.

## Notes

- The app downloads `latest.json` and the installer without signing in, so
  the releases must be public.
- Updates start working from the first version installed with the updater in
  it; install that one by hand from the Releases page.
