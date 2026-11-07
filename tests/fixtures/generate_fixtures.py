#!/usr/bin/env python3
"""Regenerate the checked-in OBJ fixtures beside this file (fixtures/). The large and
derived fixtures (large, mixed_crlf, chunked, chunked_bad_utf8) are written by main.luc
itself when the tests run."""
import math
from pathlib import Path
import random


def write_fixtures(directory):
    rng = random.Random(7)
    (directory / "numbers.obj").write_text(numbers(rng))
    (directory / "polygons.obj").write_text(polygons())
    (directory / "mixed.obj").write_text(mixed(rng))
    # Non-ASCII text in comments and names is fine; invalid UTF-8 is not.
    (directory / "accents.obj").write_text("# café ✓\no pièce\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n", encoding="utf-8")
    (directory / "bad_utf8.obj").write_bytes(b"# caf\xe9\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n")


def number_text(rng, index, bounded=False):
    """Decimal spellings a converter must round exactly as strtod does."""
    kind = index % (6 if bounded else 12)
    value = rng.uniform(-1.0, 1.0) * 10 ** rng.randint(-8, 5 if bounded else 8)
    if kind == 0:
        return repr(value)
    if kind == 1:
        return "%.17g" % value
    if kind == 2:
        return "%.6f" % value
    if kind == 3:
        return "%.9e" % value
    if kind == 4:
        return "%.19g" % value
    if kind == 5:
        return "%.25g" % value  # more than 19 significant digits
    if kind == 6:
        return str(rng.randint(-10 ** 18, 10 ** 18))
    if kind == 7:
        return "0.%017dE%+d" % (rng.randint(0, 10 ** 17 - 1), rng.randint(-90, 90))
    if kind == 8:
        return rng.choice(["+1.5", ".5", "5.", "-0", "-0.0", "0.000", "1e0", "007.25", "-.125e+3",
                           "9007199254740993", "9007199254740995", "4.9406564584124654e-320",
                           "2.2250738585072014e-308", "1.7976931348623157e308", "1e-400"])
    if kind == 9:
        # Halfway between two doubles near 1: needs the full decimal.
        return "1.00000000000000011102230246251565404236316680908203125"
    if kind == 10:
        return "%d.%de%d" % (rng.randint(0, 99999), rng.randint(0, 99999), rng.randint(-30, 30))
    return "%.16g" % (value * 1e-20)


def numbers(rng):
    """Points take values within the mesh bound; normals, which are not
    bounded, take every spelling, and each is used by a face corner."""
    lines = []
    for index in range(3000):
        lines.append("v " + " ".join(number_text(rng, index * 3 + axis, bounded=True) for axis in range(3)))
    for index in range(4000):
        lines.append("vn " + " ".join(number_text(rng, index * 3 + axis) for axis in range(3)))
    lines += ["v 0 0 0", "v 1 0 0", "v 0 1 0"]
    for index in range(1, 4001):
        lines.append("f -3//%d -2//%d -1//%d" % (index, index, index))
    return "\n".join(lines) + "\n"


def polygons():
    """Convex polygons of 3..1000 sides, each on its own points."""
    lines = []
    points = 0
    for sides in list(range(3, 40)) + [100, 255, 256, 257, 1000]:
        for step in range(sides):
            angle = 2 * math.pi * step / sides
            lines.append("v %.9f %.9f %d" % (math.cos(angle), math.sin(angle), sides))
        lines.append("f " + " ".join(str(points + at + 1) for at in range(sides)))
        points += sides
    return "\n".join(lines) + "\n"


def mixed(rng):
    """Negative and positive references, faces with and without vt/vn, and the
    declarations the decoder ignores, interleaved with the data."""
    lines = ["# exported by a test", "mtllib scene.mtl", "o grid", ""]
    n = 30
    for z in range(n):
        for x in range(n):
            w = rng.choice(["", "", " 1"])
            lines.append("v %r %r %r%s" % (x * 0.1, math.sin(x + z) * 0.05, z * 0.1, w))
            lines.append("vt %r %r" % (x / n, z / n) if x % 3 else "vt %r" % (x / n))
            lines.append("vn 0 1 %r # trailing comment" % (x * 1e-3))
    lines += ["g faces", "usemtl steel", "s 1", "s off"]
    for z in range(n - 1):
        for x in range(n - 1):
            a = z * n + x + 1
            quad = [a, a + n, a + n + 1, a + 1]
            form = (x + z) % 5
            if form == 0:
                corners = [str(p) for p in quad]
            elif form == 1:
                corners = ["%d/%d" % (p, p) for p in quad]
            elif form == 2:
                corners = ["%d//%d" % (p, p) for p in quad]
            elif form == 3:
                corners = ["%d/%d/%d" % (p, p, p) for p in quad]
            else:
                # Negative references count back from the last declared element.
                total = n * n
                corners = ["%d/%d/%d" % (p - total - 1, p - total - 1, p - total - 1) for p in quad]
            if (x + z) % 7 == 0:
                lines.append("f " + " ".join(corners[:3]))
                lines.append("f " + " ".join([corners[0], corners[2], corners[3]]) + "  # split")
            else:
                lines.append("f " + " ".join(corners))
    # Records after the faces: a later point that no face uses, and a comment.
    lines += ["v 9 9 9", "vt 0.5 0.5", "#done"]
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    write_fixtures(Path(__file__).resolve().parent / "fixtures")
