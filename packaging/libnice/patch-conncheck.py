#!/usr/bin/env python3
"""Patch libnice's agent/conncheck.c so a Muji group-call mesh doesn't abort the process.

Stock libnice (0.1.22/0.1.23) calls abort() via g_assert in
priv_conn_check_tick_stream_nominate when the controlling agent's nominate tick runs while the
best candidate pair isn't yet NICE_CHECK_SUCCEEDED — which happens with slow TCP-relay pairs in a
multi-agent mesh. This turns those two fatal assertions into a graceful `continue` (skip
nominating a not-yet-ready pair this tick; it's reconsidered once it succeeds).

Usage:  patch-conncheck.py [path/to/agent/conncheck.c]   (default: agent/conncheck.c)

Idempotent. Exits non-zero if the expected code can't be found, so a build never silently
produces an unpatched (still-crashing) libnice.
"""
import re
import sys
import pathlib

MARKER = "monocles: graceful nomination"

path = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "agent/conncheck.c")
src = path.read_text()

if MARKER in src:
    print(f"{path}: already patched")
    sys.exit(0)

# The unique succeeded_pair + double-assert block (whitespace-insensitive; matches 0.1.22/0.1.23).
pattern = re.compile(
    r"if \(p->succeeded_pair != NULL\)\s*\{\s*"
    r"g_assert \(p->state == NICE_CHECK_DISCOVERED\);\s*"
    r"p = p->succeeded_pair;\s*\}\s*"
    r"g_assert \(p->state == NICE_CHECK_SUCCEEDED\);",
    re.S,
)
replacement = (
    "if (p->succeeded_pair != NULL) {\n"
    "                if (p->state != NICE_CHECK_DISCOVERED) continue; /* " + MARKER + " */\n"
    "                p = p->succeeded_pair;\n"
    "              }\n"
    "              if (p->state != NICE_CHECK_SUCCEEDED) continue; /* " + MARKER + " (libnice mesh crash workaround) */"
)

new_src, n = pattern.subn(replacement, src)
if n != 1:
    sys.exit(
        f"ERROR: graceful-nomination patch matched {n} times in {path} (expected 1).\n"
        "libnice source may have changed — inspect priv_conn_check_tick_stream_nominate and "
        "update packaging/libnice/patch-conncheck.py. Refusing to build an unpatched libnice."
    )

path.write_text(new_src)
print(f"{path}: patched ({MARKER})")
