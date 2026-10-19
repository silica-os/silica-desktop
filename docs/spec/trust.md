# Gatekeeper: `dev.frascella.Silica.Trust1`

Status: implemented (silica-os#3), `crates/daemon/src/trust.rs` on the verification code of
`crates/bundle`. Formats (signatures, quarantine attribute): silica-os `docs/app-bundles.md`.

Decides whether an app may open, and shows the alerts that ask the user.

## Policy

`security.appSources` in Settings1: `flathub`, `trusted` (the default) or `anywhere`. The
Settings pane offers `flathub` ("Flathub": only Flatpak apps and the system's apps open) and
`trusted` ("Flathub and identified developers"); `anywhere` is set from the command line
(`silica-cli settings set security.appSources anywhere`) and then shows in the pop-up as
"Anywhere", as on the reference.

## Verdicts

`Assess(s path) → (s verdict, a{sv} info)`:

| Verdict | When |
| --- | --- |
| `allow` | a system app (`/usr/Applications`, desktop entries of the image), a Flatpak app, a *verified* bundle not in quarantine, an app the user already approved (same manifest) |
| `confirm` | a *verified* bundle in quarantine; an AppImage not yet approved; a *self-signed* bundle in quarantine with the policy `anywhere` |
| `block` | an unsigned or tampered bundle; a *self-signed* bundle with the policy `trusted` (whether in quarantine or not) |
| `malware` | the developer key is revoked |

With the policy `anywhere`, unsigned and self-signed bundles are `confirm` instead of `block`;
tampered and revoked stay as they are. With the policy `flathub`, every bundle and AppImage the
user has not approved is `block` (reason `policy`), the verified ones too; Open Anyway still
approves one, as on the reference.

`info`: `name`, `id`, `developer` (`s`), `developer_status` (`verified`, `self-signed`,
`unsigned`, `revoked`), `origin` (`s`, from the quarantine attribute), `agent` (`s`),
`quarantined` (`b`), `sandbox` (`b`, an AppImage the user runs in the sandbox), `reason` (`s`, for `block`: `unsigned`, `self-signed`, `tampered`,
`invalid`, `policy`), `kind` (`s`).

## Prompts

`Prompt(s path) → s choice` assesses again and, for anything but `allow`, asks the shell to
show the alert and waits for the answer (like `Authentication1`): it emits
`PromptRequested(u id, s verdict, a{sv} info)`; the shell draws the alert and calls
`Answer(u id, s choice)`. The shell calls `RegisterAgent()` when it starts; without a
registered shell that still owns its bus name, a prompt is answered `cancel` at once.
`Pending() → a(usa{sv})` lists the open prompts (a shell that restarts shows them again).
Without an answer within 10 minutes the choice is `cancel`.

| Verdict | Alert | Choices |
| --- | --- | --- |
| `confirm` | "“Name” is an app downloaded from the Internet. Are you sure you want to open it?", the origin and the developer | Open, Cancel (default) |
| `block` | "“Name” Not Opened", "Silica cannot verify that “Name” is free of malware or has not been modified." | Move to Trash, Done (default) |
| `malware` | "“Name” will damage your computer.", "Its developer’s key was revoked. Move it to the Trash." | Done, Move to Trash (default, destructive) |

- `open` on `confirm` removes the quarantine attribute and records the approval.
- `trash` moves the app to the user's Trash (the freedesktop trash of its file system).
- `block` records the app among the recently blocked: Settings › Privacy & Security shows
  "“Name” was blocked to protect Silica." with Open Anyway for one hour.

## Run in sandbox

`SetSandbox(s path, b on) → b` runs an AppImage in the sandbox from now on, or not
(`launch.md`); false for anything but an AppImage. The choice is kept in `trust.bin` by path
(version 2 of the file adds the list; version 1 is read as having none).

## Move to Applications

`SuggestMove(s path) → s` (silica-desktop#255): for an app bundle on an attached disk image
(the daemon's record of attached volumes), the shell shows the alert `volume` through the
same agent as the prompts (`PromptRequested` with the verdict `volume`): "“Name” is running from
a disk image.", Move to Applications (the default, on top) and Not Now, stacked. On `move` the
daemon copies the bundle into `/Applications` under its name, or the next free one ("Name
2.app"), with the quarantine attribute (flag `0002`, agent `dev.frascella.Silica`, origin the
image's `file://` URL), directly when the folder is writable and through the polkit helper
otherwise (as Mustache installs); the app goes on running from the image. The result is
`moved`, `cancel` or `failed: <reason>`; anything not a bundle on an attached image is
`cancel` at once. The copy keeps the approval of the first launch (same id and developer key):
it opens without asking again.

`Blocked() → a(sst)` (path, name, time) lists them. `OpenAnyway(s path) → b` checks the polkit
action `dev.silica.trust.override` (administrator password, `auth_admin`), records the approval
and launches the app; it refuses a path that is not in the blocked list or was blocked more than
an hour ago.

Layout and texts are the Amalfi masters `Alerts/Gatekeeper/*` (`docs/design/amalfi/
app-installation.md` § 1): the default button is always the trailing one (Return), Escape
answers `cancel`. The Confirm detail names the origin's host (the image's file name for a disk
image) and the developer (`(self-signed)`, or `unknown` for an AppImage); a block says the same
for every reason. The behaviour above is the reference's.

## Approvals and cache

`~/.local/state/silica/trust.bin` (binary, magic `STRU`, atomic writes) keeps:

- the cache of assessments, keyed by (path, inode, modification time of `Info.toml` and of
  `_Signature/manifest`, SHA-256 of the manifest): a bundle that changes is verified again;
- the approvals, keyed by (id, developer key id) for signed bundles, so that an update signed by
  the same developer stays approved (a modified bundle is blocked anyway: integrity is checked
  before approvals), and by (path, inode, size, modification time) for unsigned bundles and
  AppImages: replacing them asks again;
- the recently blocked list;
- the AppImages run in the sandbox (by path).

Verifying hashes every file of the bundle. A bundle whose full verification takes under
200 ms (most apps: a Flutter app of 30 MB hashes in about 30 ms) is verified in full at every
launch. A larger one is answered from the cache while its stamp (inode and modification times of
the bundle, `Contents/`, `Info.toml` and the manifest) is unchanged, and verified again in the
background: a file changed in place, which no folder time shows, moves the app to `block` for the
next launch.

## Developers, CA and revocation

- CA keys: `/usr/share/silica/trust/developer-ca/*.pub` (minisign public keys).
- Revocation list: `/usr/share/silica/trust/revoked.list` from the image, and a newer copy in
  `~/.local/state/silica/revoked.list`, used only if its signature by a CA key is valid and its
  `serial` is higher. Format: a minisign-signed text file, `silica-revoked 1`, `serial <n>`,
  then one key id (16 hex digits) per line.
- Phase 7 (later, not implemented): the Silica developer CA that issues `developer.cert`, the
  service that updates the revocation list, and notarization (a CA countersignature of the
  manifest after an automated scan).
