#!/usr/bin/env python3
"""A terminal movie backed by the actual Rust bundle demo. Standard library only."""
import argparse
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import time

REPO = Path(__file__).resolve().parents[2]
WIDTH = 70


class Movie:
    def __init__(self, fast=False, record=None):
        self.fast = fast
        self.started = time.monotonic()
        self.record = record
        self.animated = sys.stdout.isatty() or record is not None
        if record:
            record.write(json.dumps({"version": 2, "width": 76, "height": 25,
                                     "title": "Kai: bytes survive the journey"}) + "\n")

    def emit(self, text):
        sys.stdout.write(text)
        sys.stdout.flush()
        if self.record:
            self.record.write(json.dumps([round(time.monotonic() - self.started, 4), "o", text]) + "\n")
            self.record.flush()

    def pause(self, seconds=1.3):
        if not self.fast:
            time.sleep(seconds)

    def scene(self, number, title, lines):
        if self.animated:
            self.emit("\033[2J\033[H")
        self.emit("+" + "-" * WIDTH + "+\r\n")
        self.emit("|" + " K A I   /   BYTES SURVIVE THE JOURNEY".ljust(WIDTH) + "|\r\n")
        self.emit("+" + "-" * WIDTH + "+\r\n")
        self.emit(f"\r\n  {number}/6  {title}\r\n\r\n")
        for line in lines:
            self.emit("  " + line + "\r\n")
            self.pause(0.14)
        self.emit("\r\n")

    def travel(self, left, right):
        for i in range(18):
            self.emit("\r  " + left + "  " + "." * i + "[###]" + "." * (17 - i) + "  " + right)
            self.pause(0.055)
        self.emit("\r\n\r\n")


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def command(args, work):
    result = subprocess.run(args, cwd=REPO, capture_output=True, text=True)
    with (work / "commands.log").open("a") as log:
        log.write(f"{args!r}\n{result.stdout}{result.stderr}\nexit={result.returncode}\n")
    return result


def checked(args, work):
    result = command(args, work)
    require(result.returncode == 0, result.stderr.strip() or "command failed")
    return result.stdout.strip()


def live(movie):
    require(os.name == "posix", "This restoration demo requires POSIX modes and symlinks.")
    require(shutil.which("cargo") and shutil.which("tar"), "Install Cargo and a capable tar tool first.")
    work = Path(tempfile.mkdtemp(prefix="kai-ascii-demo-"))
    print(f"Demo files and command log: {work}", flush=True)
    cargo = ["cargo", "run", "--quiet", "--locked", "--offline"]
    reader = cargo + ["-p", "kai-object-reference", "--bin", "kai-bundle-reader", "--"]
    source = work / "source.kai-bundle"
    archive = work / "source.tar"
    restored = work / "restored"

    movie.scene(1, "A tiny world worth preserving", [
        "source/", "|-- hello.txt           text, including its final newline",
        "|-- run.sh              executable script (never executed)",
        "|-- data/bytes.bin      binary: 00 ff 0d 0a",
        "`-- hello-link          symlink -> hello.txt", "",
        "Synthetic fixture. Real Rust encoders and reader.",
        "Preparing the bundle; first run may compile Rust..."])
    root = checked(cargo + ["--example", "source_bundle", "--", str(source)], work)
    movie.pause()

    movie.scene(2, "Give the snapshot an exact identity", [
        "[ files + kinds + tree ] ---> [ immutable objects ]", "",
        "The sample bundle was created successfully.",
        f"Bundle size: {source.stat().st_size:,} bytes",
        "Tree fingerprint:", root[:38], root[38:], "",
        "The ID covers the saved tree, not a filename or timestamp."])
    movie.travel("SOURCE", "BUNDLE")
    movie.pause()

    movie.scene(3, "An independent reader checks the package", [
        "BUNDLE --> inventory --> hashes --> typed source closure", "",
        "Checking with kai-bundle-reader...",
        "This reader does not link the production Kai library."])
    found = checked(reader + [str(source)], work)
    require(found == root, "Independent reader returned an unexpected root.")
    movie.emit("  [PASS] Complete sample source bundle verified.\r\n")
    movie.pause(2)

    movie.scene(4, "Bring the files back", [
        "[ verified bundle ] ---> [ tar archive ] ---> [ fresh folder ]", "",
        "Exporting and restoring with the local tar tool..."])
    checked(reader + [str(source), "--archive", root, str(archive)], work)
    restored.mkdir()
    checked(["tar", "-xpf", str(archive), "-C", str(restored)], work)
    checks = [
        ((restored / "hello.txt").read_bytes() == b"Hello from a verified Kai bundle.\n", "Text bytes match"),
        ((restored / "data/bytes.bin").read_bytes() == bytes([0, 255, 13, 10]), "Binary bytes match: 00 ff 0d 0a"),
        ((restored / "run.sh").read_bytes() == b"#!/bin/sh\nprintf 'saved executable bytes\\n'\n", "Script bytes match"),
        (stat.S_IMODE((restored / "run.sh").stat().st_mode) == 0o755, "Executable mode preserved: 0755"),
        (stat.S_IMODE((restored / "hello.txt").stat().st_mode) == 0o644, "Regular mode preserved: 0644"),
        ((restored / "hello-link").is_symlink() and os.readlink(restored / "hello-link") == "hello.txt", "Symlink target preserved: hello.txt"),
    ]
    for passed, label in checks:
        require(passed, label + " FAILED")
        movie.emit(f"  [PASS] {label}\r\n")
        movie.pause(0.45)
    movie.pause(2)

    movie.scene(5, "Now flip one byte", [
        "GOOD BUNDLE --> copy --> change one byte --> CORRUPT BUNDLE", "",
        "Will the reader export damaged source?"])
    corrupt = work / "corrupt.kai-bundle"
    data = bytearray(source.read_bytes())
    data[-1] ^= 1
    corrupt.write_bytes(data)
    rejected = work / "should-not-exist.tar"
    result = command(reader + [str(corrupt), "--archive", root, str(rejected)], work)
    require(result.returncode != 0 and not rejected.exists(), "Corrupt bundle was not safely refused.")
    require("checksum" in result.stderr, "Expected checksum rejection; see command log.")
    movie.emit("  [REJECTED] " + result.stderr.strip() + "\r\n")
    movie.emit("  [PASS] No output archive was created.\r\n")
    movie.pause(2.5)

    movie.scene(6, "The bytes survived. The damaged copy did not pass.", [
        "CREATE       VERIFY       RESTORE       TAMPER", "  [OK] ------ [OK] ------- [OK] -------- [BLOCKED]", "",
        "These were real commands, with checked results.",
        "Today: verified source bundles and snapshot archive export.",
        "Also available: repository recovery. Durable storage is next.",
        "No live collaboration or working VCS is claimed here."])
    movie.emit(f"  Explore the restored files:\r\n  {restored}\r\n\r\n")
    movie.pause(2)


def replay(path, fast):
    with path.open() as stream:
        first = next(stream, None)
        require(first is not None, "Empty recording; expected an asciicast v2 header.")
        header = json.loads(first)
        require(isinstance(header, dict) and header.get("version") == 2,
                "Expected an asciicast v2 recording.")
        previous = 0.0
        for line in stream:
            event = json.loads(line)
            require(isinstance(event, list) and len(event) == 3
                    and isinstance(event[0], (int, float)) and not isinstance(event[0], bool)
                    and isinstance(event[1], str) and isinstance(event[2], str),
                    "Malformed recording event; expected [time, kind, text].")
            timestamp, kind, text = event
            if kind != "o":
                continue
            if not fast:
                time.sleep(max(0, min(timestamp - previous, 10)))
            sys.stdout.write(text)
            sys.stdout.flush()
            previous = timestamp


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fast", action="store_true", help="skip animation delays")
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--record", type=Path, help="save an asciicast v2 recording; refuses overwrite")
    group.add_argument("--replay", type=Path, help="play a prior recording without running Rust")
    args = parser.parse_args()
    try:
        if args.replay:
            replay(args.replay, args.fast)
        elif args.record:
            with args.record.open("x") as output:
                live(Movie(args.fast, output))
        else:
            live(Movie(args.fast))
    except (OSError, RuntimeError, ValueError) as error:
        print(f"\nDemo stopped: {error}", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        print("\nDemo stopped. Temporary demo files are retained.", file=sys.stderr)
        return 130
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
