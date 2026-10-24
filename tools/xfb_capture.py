"""Validate YUYV framebuffer data before converting a Dolphin capture."""


def summarize_yuyv_luma(data: bytes, width: int, height: int) -> dict[str, int]:
    if width <= 0 or height <= 0 or (width * height) % 2:
        raise ValueError("YUYV dimensions must contain a positive even pixel count")
    expected = width * height * 2
    if len(data) != expected:
        raise ValueError(f"XFB has {len(data)} bytes; expected {expected}")

    luma = data[0::2]
    active_pixels = sum(value > 32 for value in luma)
    levels = len(set(luma))
    summary = {
        "active_luma_pixels": active_pixels,
        "luma_levels": levels,
        "minimum_luma": min(luma),
        "maximum_luma": max(luma),
    }
    minimum_active = max(1, width * height // 1000)
    if active_pixels < minimum_active or levels < 8:
        raise ValueError(
            "XFB is blank or nearly blank: "
            f"{active_pixels} active pixels (minimum {minimum_active}), "
            f"{levels} luma levels (minimum 8)"
        )
    return summary


def write_yuyv_png(path, data: bytes, width: int, height: int):
    """Write an RGB PNG from a validated YUYV software framebuffer."""
    import struct
    import zlib

    if width <= 0 or height <= 0 or width % 2:
        raise ValueError("YUYV dimensions must have a positive even width")
    expected = width * height * 2
    if len(data) != expected:
        raise ValueError(f"XFB has {len(data)} bytes; expected {expected}")
    rgb = bytearray()
    for offset in range(0, len(data), 4):
        y0, u, y1, v = data[offset:offset + 4]
        for y in (y0, y1):
            c, d, e = y - 16, u - 128, v - 128
            channels = ((298*c + 409*e + 128) >> 8,
                        (298*c - 100*d - 208*e + 128) >> 8,
                        (298*c + 516*d + 128) >> 8)
            rgb.extend(max(0, min(255, value)) for value in channels)
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    rows = b''.join(b'\x00' + rgb[y*width*3:(y+1)*width*3] for y in range(height))
    png = (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0))
           + chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b''))
    path.write_bytes(png)
