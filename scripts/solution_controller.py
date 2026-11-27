#!/usr/bin/env python3
"""Independent Solution Link controller emulator. Python 3, standard library only."""

import argparse
import base64
import os
import re
import select
import struct
import sys
import zlib
from pathlib import Path

FACES = "URFDLB"
NORMALS = ((0, 1, 0), (1, 0, 0), (0, 0, 1), (0, -1, 0), (-1, 0, 0), (0, 0, -1))
RIGHT = ((1, 0, 0), (0, 0, -1), (1, 0, 0), (1, 0, 0), (0, 0, 1), (-1, 0, 0))
DOWN = ((0, 0, 1), (0, -1, 0), (0, -1, 0), (0, 0, -1), (0, -1, 0), (0, -1, 0))
COLOURS = dict(zip(FACES, (
    (244, 244, 239), (217, 43, 58), (31, 170, 89),
    (247, 208, 56), (245, 130, 32), (47, 111, 222),
)))
SCRAMBLE = ("F", "U2", "R'")


def translate(move, half=True):
    """Map logical notation to a fixture channel and signed quarter steps."""
    match = re.fullmatch(r"([URFDLB])(['2]?)", move)
    if not match or (match[2] == "2" and not half):
        raise ValueError("unsupported notation")
    face, suffix = match.groups()
    return FACES.index(face), {"": 1, "'": -1, "2": 2}[suffix]


def dot(a, b):
    return sum(x * y for x, y in zip(a, b))


def quarter(vector, axis):
    """A clockwise 90 degree turn about an outward axis."""
    a, b, c = axis
    x, y, z = vector
    cross = (b * z - c * y, c * x - a * z, a * y - b * x)
    return tuple(dot(vector, axis) * axis[i] - cross[i] for i in range(3))


class Fixture:
    """Six face channels acting on an independent sticker-coordinate model."""

    def __init__(self):
        self.slots = [
            (tuple(normal[k] + right[k] * col + down[k] * row for k in range(3)), normal)
            for normal, right, down in zip(NORMALS, RIGHT, DOWN)
            for row in (-1, 0, 1) for col in (-1, 0, 1)
        ]
        self.home = {slot: FACES[i // 9] for i, slot in enumerate(self.slots)}
        self.stickers = self.home.copy()
        for move in SCRAMBLE:
            self.step(*translate(move))

    def step(self, channel, steps):
        axis = NORMALS[channel]
        for _ in range(steps % 4):
            self.stickers = {
                (quarter(p, axis), quarter(normal, axis)) if dot(p, axis) == 1 else (p, normal): colour
                for (p, normal), colour in self.stickers.items()
            }

    def face(self, face):
        start = FACES.index(face) * 9
        return [self.stickers[slot] for slot in self.slots[start:start + 9]]


def photo(stickers, rotate):
    """Render the fixture as RGB PNG pixels, with a camera border and sticker gaps."""
    side = 320
    background = bytes((60, 64, 70))
    plastic = bytes((11, 12, 15))
    pixels = bytearray(background * side * side)
    for y in range(35, 285):
        for x in range(35, 285):
            col, dx = divmod(x - 37, 82)
            row, dy = divmod(y - 37, 82)
            on_sticker = 0 <= row < 3 and 0 <= col < 3 and 5 <= dx < 77 and 5 <= dy < 77
            colour = bytes(COLOURS[stickers[row * 3 + col]]) if on_sticker else plastic
            # Inverse of the clockwise correction requested in FACE.
            px, py = (x, y) if rotate == 0 else (y, side - 1 - x)
            offset = (py * side + px) * 3
            pixels[offset:offset + 3] = colour
    stride = side * 3
    rows = b"".join(b"\0" + pixels[i:i + stride] for i in range(0, len(pixels), stride))

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    header = struct.pack(">IIBBBBB", side, side, 8, 2, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")


class Abort(Exception):
    pass


def serve(args, cube):
    # Read from the pipe without prefetching. select() can then detect STOP or
    # EOF even when the controller is executing its own motion sequence.
    reader = sys.stdin.fileno()

    def send(line):
        print(line, flush=True)

    def receive():
        raw = bytearray()
        while len(raw) <= 128:
            byte = os.read(reader, 1)
            if not byte:
                raise Abort()
            if byte == b"\n":
                if raw == b"STOP":
                    raise Abort()
                return raw.decode("ascii")
            raw.extend(byte)
        raise ValueError("line too long")

    def expect(command):
        if receive() != command:
            raise ValueError(f"expected {command}")

    def motion_pause():
        if select.select([reader], [], [], args.delay)[0]:
            raise Abort()

    try:
        expect("HELLO 1")
        send(f"SOLUTION 1 {args.camera} 3 1 {int(not args.no_half)} {round(args.delay * 1000)}")
        while True:
            command = receive()
            if command == "SCAN":
                send("OK")
                for i, face in enumerate("FRUBLD"):
                    rotate = 90 if i % 2 else 0
                    if args.camera == "png" or args.frames_dir:
                        frame = photo(cube.face(face), rotate)
                    if args.frames_dir:
                        (args.frames_dir / f"{i}.png").write_bytes(frame)
                    send(f"FACE {i} {face} {rotate}")
                    if args.camera == "png":
                        data = base64.b64encode(frame).decode("ascii")
                        for start in range(0, len(data), 60):
                            send(f"DATA {data[start:start + 60]}")
                        send("END")
                    expect(f"ACK {i}")
                send("DONE")
            elif command.startswith("SOLVE "):
                count = int(command.split()[1])
                if not 1 <= count <= 4096:
                    raise ValueError("batch size outside controller limit")
                send("OK")
                moves = []
                for _ in range(count):
                    record = receive()
                    if not record.startswith("MOVE "):
                        raise ValueError("expected MOVE")
                    moves.append(translate(record[5:], not args.no_half))
                    send("OK")
                expect("RUN")
                if args.reject:
                    raise ValueError("fixture rejected RUN before any motion")
                send("ACCEPT")
                for i, (channel, steps) in enumerate(moves):
                    motion_pause()
                    cube.step(channel, steps)
                    print(f"actuator channel={channel} quarter_steps={steps}", file=sys.stderr, flush=True)
                    send(f"MOVED {i}")
                send("DONE")
                print(f"Fixture solved: {cube.stickers == cube.home}", file=sys.stderr, flush=True)
            else:
                raise ValueError("unexpected command")
    except (ValueError, IndexError) as error:
        send(f"ERR {error}")
    except (Abort, ConnectionError):
        print("Link closed or stopped. Discarding any queued moves.", file=sys.stderr, flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--camera", choices=("png", "app"), default="png")
    parser.add_argument("--delay", type=float, default=0.35, help="seconds per logical move")
    parser.add_argument("--no-half", action="store_true", help="advertise quarter turns only")
    parser.add_argument("--reject", action="store_true", help="reject RUN before any motion")
    parser.add_argument("--frames-dir", type=Path, help="write synthetic camera frames for browser testing")
    args = parser.parse_args()
    if not 0 <= args.delay <= 60:
        parser.error("--delay must be between 0 and 60 seconds")
    if args.frames_dir:
        args.frames_dir.mkdir(parents=True, exist_ok=True)
    serve(args, Fixture())

if __name__ == "__main__":
    main()
