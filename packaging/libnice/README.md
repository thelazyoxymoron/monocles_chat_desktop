# Patched libnice for desktop group calls

## The problem

Group calls (XEP-0272 Muji) are a full mesh of per-pair Jingle/ICE sessions, so each participant
runs several `NiceAgent`s at once. Stock **libnice 0.1.22** (Debian trixie) — and 0.1.23 — abort
the whole process with:

```
libnice:ERROR ../agent/conncheck.c: priv_conn_check_tick_stream_nominate:
assertion failed: (p->state == NICE_CHECK_SUCCEEDED)
```

This fires when a slow **TCP-relay** candidate pair (TURN over TCP/TLS — used when a participant's
network blocks UDP) is still completing its TCP handshake at the moment the controlling agent's
nomination tick runs. A native `abort()` can't be caught from the Rust app.

So the desktop has two unsatisfiable-at-once needs on stock libnice:
- a participant on a UDP-blocking network can only connect via TCP/TLS relay, but
- TCP relay pairs in a mesh crash libnice.

## How the app behaves by default (no patched libnice)

`configure_ice` (in `crates/mxc-media/src/lib.rs`) gathers **UDP relays only for Muji legs** —
1:1 calls keep TCP/TLS relays. This means stock installs **never crash**, but a UDP-blocked peer
can't join a *group* call (that leg just fails; 1:1 still works).

## The fix: a patched libnice + an opt-in flag

`build-patched-libnice.sh` builds libnice with the fatal nomination assertion turned into a
graceful skip (don't nominate a not-yet-succeeded pair this tick; reconsider it once it
succeeds). With that libnice, TCP relay pairs in a mesh no longer abort, so group calls work even
for UDP-blocked participants.

Because that requires the patched lib to actually be loaded, the TCP-relay-for-Muji behaviour is
**opt-in** via an env var, so it can never re-crash a stock install:

| `MONOCLES_MUJI_TCP_RELAY` | Muji TURN relays | needs patched libnice? |
|---|---|---|
| unset / `0` (default) | UDP only | no |
| `1` / `true` | UDP + TCP/TLS | **yes** |

## Build + run (local / Debian)

```bash
# 1. build deps (Debian/Ubuntu)
sudo apt install build-essential meson ninja-build pkg-config git \
     libglib2.0-dev libgnutls28-dev

# 2. build the patched libnice (installs to ~/.local/monocles-libnice by default)
packaging/libnice/build-patched-libnice.sh

# 3. run the app against it (sets LD_LIBRARY_PATH + MONOCLES_MUJI_TCP_RELAY=1)
scripts/run-patched.sh            # or --debug
```

The system GStreamer `nice` plugin (`libgstnice.so`) links `libnice.so.10`; putting the patched
build first on `LD_LIBRARY_PATH` makes `webrtcbin` load it — no plugin rebuild needed (same SONAME
/ ABI within `libnice.so.10`).

Verify which libnice is loaded:
```bash
LD_LIBRARY_PATH="$HOME/.local/monocles-libnice/lib" \
  ldd "$(gst-inspect-1.0 nice | awk '/Filename/{print $2}')" | grep libnice
```

## Files here

- `patch-conncheck.py` — the actual source transform (graceful nomination). Idempotent; fails if
  it can't match, so a build never silently produces an unpatched libnice. Reused by every path
  below.
- `build-patched-libnice.sh` — clone + patch + build + install to a prefix (Linux/local).

## Distribution

- **Flatpak** (`packaging/flatpak/`): **done** — a `libnice` build module applies
  `patch-conncheck.py`, builds 0.1.23 into `/app`, and the manifest sets
  `--env=MONOCLES_MUJI_TCP_RELAY=1`. `/app/lib` precedes the runtime libs so `webrtcbin` loads it.
- **deb / rpm** (`scripts/build-deb.sh`, `packaging/rpm/`): **done** — both bundle the patched
  `libnice.so.10` into the private libdir `/usr/lib/monocles-chat-qt` (rpm: `%{_libdir}/...`) and
  install `/usr/bin/monocles-chat-qt` as a launcher wrapper that exports `LD_LIBRARY_PATH` +
  `MONOCLES_MUJI_TCP_RELAY=1`. The system `gstnice` plugin loads the bundled libnice (same SONAME).
  Build with `BUNDLE_LIBNICE=0` for a plain package on the distro's libnice (UDP-only group calls).
- **macOS / Windows** (`packaging/macos/`, `packaging/windows/`): use brew/installer libnice
  (UDP-only by default); build a patched `libnice.dylib`/`nice.dll` with `patch-conncheck.py` and
  launch with `DYLD_LIBRARY_PATH`/`PATH` + the env var — documented in each README.

## When to remove this

Drop the patch + the env-var gate (and re-enable TCP relays for Muji unconditionally) once a
libnice release fixes the `priv_conn_check_tick_stream_nominate` assertion — track upstream
(https://gitlab.freedesktop.org/libnice/libnice). The workaround is intentionally isolated to
`configure_ice` + this directory so it's easy to revert.
