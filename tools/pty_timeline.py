#!/usr/bin/env python3
"""Protocol-level timeline of the interactive TUI, captured over a pty.

Kitty graphics commands are invasive to inspect by eye (the payloads are
megabytes of base64), so this harness prints only the lifecycle calls —
`a=t` transmits, `a=p` placements and `a=d` deletes — in arrival order. It is
the end-to-end reproduction for the sorter -> slide flicker: before the
spare-id swap the captured order was

    delete id=17..22                  (grid torn down)
    transmit id=4 1280x720            (card raster shown full-screen)
    place    id=4 p=1
    transmit id=4 2400x1350           (kitty deletes the displayed image here)
    place    id=4 p=1                 (slide pops back after the whole transmit)

and after it

    transmit id=4 1280x720
    place    id=4 p=1
    delete   id=17..22                (grid released once the slide is up)
    transmit id=5 2400x1350           (spare id: id=4 stays on screen)
    place    id=5 p=1
    delete   id=4                     (outgoing bitmap released after the swap)

Usage:
    python3 tools/pty_timeline.py DECK.pptx [keys]
    INITIAL_WAIT=9 KEY_WAIT=3 python3 tools/pty_timeline.py deck.pptx tab,right,enter

`keys` is a comma-separated list of tab/enter/left/right/esc/q. The waits are
seconds of output draining after the start and after each key: raise them for
large decks, whose canvas transmits are several megabytes over a pty.

The terminal is a fresh pty sized 120x40 and the child runs with
`KITTY_PPTX_PROTOCOL=kitty`, so no real Kitty terminal is required.
"""
import fcntl
import os
import pty
import re
import struct
import sys
import termios
import time

INITIAL_WAIT = float(os.environ.get("INITIAL_WAIT", "2.0"))
KEY_WAIT = float(os.environ.get("KEY_WAIT", "1.5"))
FINAL_WAIT = float(os.environ.get("FINAL_WAIT", "3.0"))
BIN = os.path.abspath(
    "target/release/kitty-pptx"
    if os.path.exists("target/release/kitty-pptx")
    else "target/debug/kitty-pptx"
)

# A complete kitty APC sequence ends with ST (ESC backslash): matching the
# terminator keeps rescanning a growing buffer from re-emitting partials.
COMMAND = re.compile(rb"\x1b_Ga=(t|p|d)([^\x1b]*)\x1b\\")
TRANSMIT = re.compile(rb"a=t(?:,f=100)?(?:,i=(\d+))(?:,s=(\d+))?(?:,v=(\d+))?")
PRESENT = re.compile(rb"a=p,i=(\d+),p=(\d+)")
DELETE = re.compile(rb"a=d,d=(\w+)(?:,i=(\d+))?")

KEYS = {
    "tab": b"\t",
    "enter": b"\r",
    "right": b"\x1b[C",
    "left": b"\x1b[D",
    "up": b"\x1b[A",
    "down": b"\x1b[B",
    "esc": b"\x1b",
    "w": b"w",
    "j": b"j",
    "k": b"k",
    "d": b"d",
    "u": b"u",
    "q": b"q",
}


def spawn(deck, rows=40, cols=120):
    pid, fd = pty.fork()
    if pid == 0:
        os.environ["KITTY_PPTX_PROTOCOL"] = "kitty"
        os.environ["TERM"] = "xterm-kitty"
        os.execv(BIN, [BIN, deck])
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
    return pid, fd


class Reader:
    """Accumulates the child's output and emits each complete graphics
    command exactly once, stamped at arrival."""

    def __init__(self, out):
        self.buf = b""
        self.scanned = 0
        self.out = out
        self.total_bytes = 0

    def feed(self, chunk):
        self.total_bytes += len(chunk)
        self.buf += chunk
        while True:
            m = COMMAND.search(self.buf, self.scanned)
            if not m:
                break
            self.scanned = m.end()
            self.emit(m.group(0))
        # Compact on every feed: image payloads arrive as `\x1b_Gm=1;...`
        # continuation chunks that match no command, so an uncompacted buffer
        # grows by megabytes and every search re-scans it (O(n^2)). Only an
        # unscanned tail can still start a command, and a command header is
        # tiny — keep at most 4 KiB of it.
        tail = self.buf[self.scanned:]
        self.buf = tail[-4096:]
        self.scanned = 0

    def emit(self, seq):
        stamp = time.time()
        head = seq[5:6]
        if head == b"t":
            t = TRANSMIT.search(seq)
            if t and t.group(1):
                w = t.group(2).decode() if t.group(2) else "?"
                h = t.group(3).decode() if t.group(3) else "?"
                self.out.append((stamp, f"transmit  id={t.group(1).decode():>3} {w}x{h}"))
        elif head == b"p":
            p = PRESENT.search(seq)
            if p:
                self.out.append((stamp, f"place     id={p.group(1).decode():>3} p={p.group(2).decode()}"))
        else:
            d = DELETE.search(seq)
            if d:
                ident = d.group(2).decode() if d.group(2) else "-"
                self.out.append((stamp, f"delete    d={d.group(1).decode()} id={ident}"))


def read_for(fd, seconds, reader):
    """Drain the pty for `seconds`. The child blocks in write(2) while the
    pty buffer is full, so drain as fast as possible: non-blocking reads until
    EAGAIN, then a short sleep."""
    deadline = time.time() + seconds
    os.set_blocking(fd, False)
    while time.time() < deadline:
        try:
            chunk = os.read(fd, 1 << 16)
        except BlockingIOError:
            time.sleep(0.0005)
            continue
        except OSError:
            return
        if not chunk:
            return
        reader.feed(chunk)


def main():
    args = sys.argv[1:]
    deck = args[0]
    keys = args[1] if len(args) > 1 else "tab,right,right,right,right,enter"
    pid, fd = spawn(deck)
    out = []
    reader = Reader(out)
    t0 = time.time()

    read_for(fd, INITIAL_WAIT, reader)
    out.append((time.time(), "=== first key ==="))
    print(f"[{INITIAL_WAIT:.1f}s] initial drained: {reader.total_bytes/1e6:.1f} MB, {len(out)} events", flush=True)
    # Each entry is `key` or `key@ms` (send, then drain for `ms` instead of
    # KEY_WAIT — use a small ms to simulate continuous scrolling).
    for entry in keys.split(","):
        key, _, wait = entry.partition("@")
        os.write(fd, KEYS[key])
        read_for(fd, float(wait) / 1000.0 if wait else KEY_WAIT, reader)
        out.append((time.time(), f"--- sent {key} ---"))
        print(f"[+] sent {key} (total {reader.total_bytes/1e6:.1f} MB, {len(out)} events)", flush=True)

    read_for(fd, FINAL_WAIT, reader)
    try:
        os.kill(pid, 9)
    except ProcessLookupError:
        pass

    for stamp, line in out:
        print(f"{stamp - t0:7.3f}s  {line}")
    print(f"total bytes read: {reader.total_bytes} in {time.time() - t0:.1f}s")


if __name__ == "__main__":
    main()
