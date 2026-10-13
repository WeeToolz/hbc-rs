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
