"""Make a large IFC file by repeating the DATA section of a real one N times, with the
instance ids of each copy shifted so they don't collide.

    duplicate.py <in.ifc> <out.ifc> <copies>

The copies overlap in space and repeat GlobalIds: the result is only for speed and
memory measurements, not a valid model.
"""
import re, sys

src, dst, n = sys.argv[1], sys.argv[2], int(sys.argv[3])
text = open(src, "rb").read()
start = text.index(b"DATA;") + 5
end = text.rindex(b"ENDSEC;")
head, data, tail = text[:start], text[start:end], text[end:]
ids = [int(m) for m in re.findall(rb"#(\d+)\s*=", data)]
step = max(ids) + 1
ref = re.compile(rb"#(\d+)")
# don't touch '#' inside strings: split on quotes (Part 21 escapes quotes by doubling, which keeps parity)
parts = data.split(b"'")
with open(dst, "wb") as out:
    out.write(head)
    for k in range(n):
        off = k * step
        for i, p in enumerate(parts):
            if i:
                out.write(b"'")
            out.write(p if i % 2 else ref.sub(lambda m: b"#%d" % (int(m.group(1)) + off), p))
    out.write(tail)
