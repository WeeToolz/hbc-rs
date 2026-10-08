"""Set Dolphin options required for CPU reads of GX framebuffer data."""

import configparser
from pathlib import Path


def configure_capture_profile(user_dir: Path) -> None:
    config_dir = user_dir / "Config"
    config_dir.mkdir(parents=True, exist_ok=True)
    path = config_dir / "GFX.ini"
    config = configparser.ConfigParser()
    config.optionxform = str
    config.read(path)
    if not config.has_section("Hacks"):
        config.add_section("Hacks")
    # Dolphin otherwise writes 01 FE placeholders or defers the RAM readback.
    config["Hacks"]["XFBToTextureEnable"] = "False"
    config["Hacks"]["DeferEFBCopies"] = "False"
    # The startup probe also reads EFB pixels through the CPU aperture.
    config["Hacks"]["EFBAccessEnable"] = "True"
    with path.open("w") as output:
        config.write(output)
