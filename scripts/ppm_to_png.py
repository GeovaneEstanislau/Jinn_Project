import struct
import zlib
import sys
import os

def ppm_to_png(ppm_path, png_path):
    with open(ppm_path, "rb") as f:
        data = f.read()

    # Parse header
    idx = 0
    tokens = []
    while len(tokens) < 4:
        # Skip whitespace and comments
        while idx < len(data) and data[idx] in b" \t\r\n":
            idx += 1
        if idx < len(data) and data[idx] == ord('#'):
            while idx < len(data) and data[idx] != ord('\n'):
                idx += 1
            continue
        start = idx
        while idx < len(data) and data[idx] not in b" \t\r\n":
            idx += 1
        tokens.append(data[start:idx].decode('ascii'))

    magic = tokens[0]
    width = int(tokens[1])
    height = int(tokens[2])
    maxval = int(tokens[3])
    # Single whitespace after header
    idx += 1

    print(f"PPM: {magic}, {width}x{height}, maxval={maxval}, data offset={idx}")
    raw_pixels = data[idx:idx + width * height * 3]

    # Convert RGB to PNG raw scanlines (filter byte 0 + RGB)
    raw_scanlines = bytearray()
    row_bytes = width * 3
    for y in range(height):
        raw_scanlines.append(0) # None filter
        raw_scanlines.extend(raw_pixels[y * row_bytes : (y + 1) * row_bytes])

    compressed = zlib.compress(bytes(raw_scanlines), 9)

    def make_chunk(chunk_type, chunk_data):
        return struct.pack(">I", len(chunk_data)) + chunk_type + chunk_data + struct.pack(">I", zlib.crc32(chunk_type + chunk_data) & 0xFFFFFFFF)

    png_data = bytearray(b"\x89PNG\r\n\x1a\n")
    # IHDR: width(4), height(4), bit_depth(1=8), color_type(1=2 for RGB), comp(0), filter(0), interlace(0)
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    png_data.extend(make_chunk(b"IHDR", ihdr))
    png_data.extend(make_chunk(b"IDAT", compressed))
    png_data.extend(make_chunk(b"IEND", b""))

    with open(png_path, "wb") as f:
        f.write(png_data)

    print(f"Wrote PNG {png_path} ({len(png_data)} bytes)")

if __name__ == "__main__":
    ppm = sys.argv[1] if len(sys.argv) > 1 else "d:/Projetos/Jinn/qemu_desktop.ppm"
    png = sys.argv[2] if len(sys.argv) > 2 else "C:/Users/Geovane/.gemini/antigravity/brain/de11cced-2c5c-4522-b829-87ac08911e34/qemu_desktop.png"
    ppm_to_png(ppm, png)
