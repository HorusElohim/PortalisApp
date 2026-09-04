# Distributing Portalis to testers

Status of each platform's CI pipeline, and exactly what is left to do before
it can hand a build to a real tester rather than only a CI artifact.

## Android — done

`ANDROID_RELEASE_KEYSTORE_BASE64` / `ANDROID_RELEASE_KEYSTORE_PASSWORD` are
set as repo secrets. Every CI run on `android` now produces a **stably
signed** release APK — the same signing key every time.

The original upload key was lost and was rotated on 2026-09-04. Existing APKs
signed with the lost key cannot be updated in place; testers must uninstall
those builds and install the first APK signed by the replacement key. From
that point onward, all releases must use the replacement keystore below.

**The replacement keystore itself lives at
`~/secrets-portalis/portalis-release.jks` on Atlas (192.168.1.20), and nowhere
else.** Back it up now, somewhere durable
(password manager attachment, encrypted archive, etc.) — if it is lost,
every future Android release has to ship as a *new* app: existing installs
cannot update, and every tester loses their data on reinstall. There is no
recovery path Google or anyone else can offer; only what you back up.
Password: stored alongside the keystore file, also needs backing up.

Alias: `portalis`
Replacement certificate SHA-256:
`BDE4495A828AB76463F5ABEC3114593B918282596A240EE861F854D7D179E6EB`

To hand someone the APK directly (outside the Play Store): download the
`Portalis-*-Android-*-release` artifact from the `android` job of a CI run,
or build locally:

```bash
cd portalis
flutter build apk --release
# build/app/outputs/flutter-apk/app-release.apk
```

Distribute the `.apk` by any file-transfer method (Portalis itself, if you
want to be thorough about it). No Play Store listing is required to sideload.

## iOS — needs three things from your Apple ID, then CI does the rest

The `ios` job (`.github/actions/build-ios/action.yml`) will build a *signed*
`.ipa` and upload it to TestFlight automatically once these three secrets
exist. Until then it silently falls back to the old unsigned
`--no-codesign` build (still uploaded as a CI artifact, but it cannot run on
a real device).

**Prerequisite:** an active [Apple Developer Program](https://developer.apple.com/programs/)
enrollment ($99/year) for the team already referenced in the Xcode project
(`DEVELOPMENT_TEAM = BLJKHVH3A8`). If that team ID is not yours or has
lapsed, this has to be re-pointed at a team you control first — check
Xcode → Runner target → Signing & Capabilities.

1. **Create an App Store Connect app record** for bundle ID `com.portalis`
   (App Store Connect → Apps → +). One-time; nothing to automate.

2. **Generate an App Store Connect API key**
   (App Store Connect → Users and Access → Integrations → App Store Connect
   API → Team Keys → Generate API Key). Give it the **App Manager** role.
   Downloading the `.p8` file is only possible once — save it immediately.
   Note the **Key ID** and **Issuer ID** shown next to it.

3. **Set three repo secrets** from that key:

   ```bash
   base64 -w0 AuthKey_XXXXXXXXXX.p8 | gh secret set ASC_API_KEY_BASE64
   gh secret set ASC_API_KEY_ID -b 'XXXXXXXXXX'
   gh secret set ASC_API_ISSUER_ID -b '<issuer-uuid-from-that-same-page>'
   ```

   These three secrets are shared with the macOS notarization step below —
   set them once, both platforms pick them up.

That's it — no manual certificate export, no `.mobileprovision` file to
manage. `xcodebuild -allowProvisioningUpdates` with this API key creates and
downloads the distribution certificate and provisioning profile itself on
the runner, every run.

Once uploaded, a build needs **"Manage Compliance"** answered once per
version in App Store Connect (export-compliance question — Portalis'
crypto is public/self-classified, answer per Apple's questionnaire) before
it clears TestFlight processing and becomes installable by testers. Then:

- **Internal testers** (your own App Store Connect team, up to 100 people,
  no review): added instantly, get a TestFlight invite email.
- **External testers** (up to 10,000 people): first build needs a quick
  Apple Beta App Review (usually under 24h), then invite by email or a
  public link.

## macOS — needs a Developer ID cert, same API key as iOS

The `macos` job (`.github/actions/build-macos/action.yml`) signs and
notarizes `portalis.app` when a Developer ID certificate is configured;
otherwise it ships an unsigned build that Gatekeeper will block on anyone
else's Mac ("app is damaged, move to Trash").

1. **Create a Developer ID Application certificate**
   (developer.apple.com → Certificates, Identifiers & Profiles →
   Certificates → + → Developer ID Application). This requires the Apple
   Developer Program enrollment from the iOS section above — same account,
   different certificate type from the "Apple Distribution" one used for
   TestFlight.

2. **Export it as a `.p12`** from Keychain Access (right-click the cert →
   Export) with a password you choose.

3. **Set two more repo secrets:**

   ```bash
   base64 -w0 DeveloperID.p12 | gh secret set MACOS_DEVELOPER_ID_CERT_BASE64
   gh secret set MACOS_DEVELOPER_ID_CERT_PASSWORD -b '<the export password>'
   ```

4. The `ASC_API_KEY_*` secrets from the iOS section are reused automatically
   for `notarytool` — nothing extra to configure there.

With both in place, every `macos` CI run signs with the Developer ID cert,
submits to Apple's notary service, and staples the ticket to `portalis.app`
— a tester can then just double-click it, no Gatekeeper warning.

Without the `ASC_API_KEY_*` secrets but *with* the Developer ID cert, the
app is signed but not notarized — still shows a one-time Gatekeeper warning
(right-click → Open bypasses it) but is otherwise usable. Signing alone is
worth having even before notarization is wired up.

### Extracting the macOS artifact — important

The CI artifact is a `.zip` file **containing another zip
(`portalis-macos.zip`)** — GitHub always wraps whatever you upload in its
own outer zip, so an `.app` bundle downloaded from the Actions UI needs two
extraction steps, not one.

**Do not** drag `portalis-macos.zip` to the Desktop and just double-click
it if Finder auto-extracted the outer GitHub zip into a bare `Contents/`
folder already — that flattening is exactly the earlier bug (framework
symlinks turned into duplicate real files, causing the `objc[...]: Class X
is implemented in both A and B` crash). Instead:

1. Download the artifact zip from the Actions run page.
2. Double-click it once in Finder (or `unzip`) — this produces
   `portalis-macos.zip`, not `portalis.app` directly.
3. Double-click `portalis-macos.zip` **once more** — *this* extraction step
   is the one `ditto` created, and Finder's Archive Utility preserves the
   framework symlinks correctly. This one produces the real `portalis.app`.
4. Run `portalis.app` from wherever it landed — do not run it from inside
   `Contents/MacOS/portalis` directly by path (as in
   `/Users/you/Downloads/Contents/MacOS/portalis`); that path shape is the
   tell that step 3 above did not happen and you are looking at a
   flattened, broken extraction.

## Linux / Windows

Both already build unsigned artifacts in CI (`linux`/`windows` jobs) — no
store or notarization process exists on either platform for a hobby-scale
distribution, so these are ready to hand out as-is via the CI artifact.
Windows Defender SmartScreen may warn on first run without a paid code-
signing certificate (a separate ~$300+/yr purchase from a CA, not an Apple
account) — optional, not required to distribute.

## Summary: what's actually blocking you right now

| Platform | Blocking | Owner |
|---|---|---|
| Android | Nothing — done | — |
| iOS | Apple Dev Program active? App Store Connect app record + API key | You |
| macOS | Developer ID cert (needs same Apple Dev Program membership) | You |
| Linux/Windows | Nothing — usable as-is | — |

Everything CI-side (signing logic, TestFlight upload, notarization,
artifact naming) is built and wired to fall back gracefully when secrets
are absent. What remains is entirely Apple-account administrative work only
you can do — generating keys and certs under your own Apple ID.
