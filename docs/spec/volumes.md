# Disk images: `dev.frascella.Silica.Volumes1`

Status: implemented (silica-os#3), `crates/daemon/src/volumes.rs` on `crates/bundle`. The
`.sxv` format is specified in silica-os `docs/app-bundles.md` § 5.

| Member | Signature | |
| --- | --- | --- |
| `Inspect(s image)` | `→ a{sv}` | reads and verifies the image without mounting it |
| `Attach(s image, a{sv} options)` | `→ s mountpoint` | verifies, mounts, returns the folder |
| `Detach(s mountpoint)` | | unmounts (eject) |
| `List()` | `→ a(ssa{sv})` | mount point, image, info of each attached volume |
| `Attached(s mountpoint)`, `Detached(s mountpoint)` | signals | |

`Inspect` info: `name` (`s`), `signature` (`valid`, `invalid`, `unsigned`), `developer`
(`s`), `developer_status` (`verified`, `self-signed`, `unsigned`, `revoked`), `license`
(`s`, the text of `LICENSE.txt`, read through a short-lived mount; empty when none),
`filesystem` (`erofs`, `squashfs`).

`Attach`:

1. The header must parse and the payload must lie inside the file; else
   `…Volumes1.Error.Invalid`.
2. The signature must be valid (`…Error.BadSignature` otherwise; a damaged image never mounts).
   An unsigned image mounts only with the policy `anywhere` (`…Error.Unsigned`). A revoked key
   is refused (`…Error.Revoked`).
3. With a license, `options["accept_license"] = true` is required (`…Error.LicenseRequired`);
   Mustache shows the license (Agree / Disagree) and calls again.
4. Mount point `/run/user/<uid>/silica-volumes/<volume name>` (a free name: `Name 2`… when
   taken); `erofsfuse --offset=4096 <image> <dir>` (or `squashfuse -o offset=4096`). The mount
   is checked in `/proc/self/mountinfo`: the FUSE tools may exit 0 without mounting.
5. Attaching an image already attached returns its mount point.

`Detach` runs `fusermount3 -u`, removes the empty folder and emits `Detached`; a busy volume
fails with `…Error.Busy` (Mustache says the volume is in use). When the daemon exits it leaves
the mounts alone; at start it adopts the FUSE mounts it finds under `silica-volumes/`.

Mustache shows each attached volume in its sidebar under Locations, with an eject button, and
opens the volume window with the layout of `.silica-window.toml`.
