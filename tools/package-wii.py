"""Package a built Wii DOL for launch through the Homebrew Channel."""

import argparse
import hashlib
from pathlib import Path
import shutil
import xml.etree.ElementTree as ET
import zipfile


def package(root: Path, diagnostics: bool) -> Path:
    name = "hbc-rs-diagnostic" if diagnostics else "hbc-rs"
    dol = root / "target" / ("hbc-rs-startup.dol" if diagnostics else "hbc-rs.dol")
    data = dol.read_bytes()
    stage = root / "target" / ("wii-diagnostic-sd" if diagnostics else "wii-sd")
    app_dir = stage / "apps" / name
    app_dir.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(dol, app_dir / "boot.dol")
    app = ET.Element("app", version="1")
    fields = {
        "name": "HBC Rust Diagnostic" if diagnostics else "HBC Rust",
        "version": "0.1.0",
        "short_description": "Wii startup fault diagnosis" if diagnostics else "Rust port of the Homebrew Channel",
        "long_description": "Local Wii test build. This application is not a channel installer.",
    }
    for key, value in fields.items():
        ET.SubElement(app, key).text = value
    # Match the launch policy of the working C reference app on CARROT.
    ET.SubElement(app, "no_ios_reload")
    ET.SubElement(app, "ahb_access")
    ET.indent(app)
    ET.ElementTree(app).write(app_dir / "meta.xml", encoding="UTF-8", xml_declaration=True)
    archive = root / "target" / ("hbc-rs-wii-diagnostic.zip" if diagnostics else "hbc-rs-wii.zip")
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as output:
        for filename in ("boot.dol", "meta.xml"):
            output.write(app_dir / filename, f"apps/{name}/{filename}")
    with zipfile.ZipFile(archive) as output:
        bad_entry = output.testzip()
        if bad_entry is not None or output.read(f"apps/{name}/boot.dol") != data:
            raise RuntimeError("Package verification failed")
        metadata = ET.fromstring(output.read(f"apps/{name}/meta.xml"))
        if metadata.find("no_ios_reload") is None or metadata.find("ahb_access") is None:
            raise RuntimeError("Package launch policy is missing")
    print(f"Verified {archive}")
    print(f"DOL SHA-256: {hashlib.sha256(data).hexdigest()}")
    return archive


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--diagnostics", action="store_true")
    args = parser.parse_args()
    package(Path(__file__).resolve().parents[1], args.diagnostics)
