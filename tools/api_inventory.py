#!/usr/bin/env python3
"""Generate and check the HBC target public-function evidence inventory."""

from __future__ import annotations

import argparse
import csv
import io
import json
import os
from pathlib import Path
import subprocess
import sys

import parity


ROOT = Path(__file__).resolve().parents[1]
RUSTDOC_JSON = ROOT / "target" / "powerpc-nintendo-none" / "doc" / "hbc_rs.json"
CSV_REPORT = ROOT / "docs" / "generated" / "api-inventory.csv"
STATUS_REPORT = ROOT / "docs" / "generated" / "api-status.md"
RUSTDOC_FORMAT = 61
FIELDS = ("api", "source", "line", "status", "evidence", "verification", "limitations")
PROVEN_VERIFICATION = {"host_tested", "target_checked", "dolphin_tested"}


def public_functions(document: dict) -> list[dict[str, str]]:
    """Return public functions, including public trait methods, from rustdoc JSON."""
    if document.get("format_version") != RUSTDOC_FORMAT:
        raise ValueError(f"unsupported rustdoc JSON format: {document.get('format_version')!r}")
    index = document.get("index")
    paths = document.get("paths")
    if not isinstance(index, dict) or not isinstance(paths, dict):
        raise ValueError("rustdoc JSON must contain index and paths objects")

    owners: dict[str, list[tuple[list[str], bool]]] = {}
    for item in index.values():
        if item.get("crate_id") != 0:
            continue
        inner = item.get("inner", {})
        if "trait" in inner and item.get("visibility") == "public":
            path_item = paths.get(str(item.get("id")))
            if path_item is not None:
                for member_id in inner["trait"].get("items", []):
                    owners.setdefault(str(member_id), []).append((path_item["path"], True))
        if "impl" in inner:
            target = inner["impl"].get("for", {}).get("resolved_path", {})
            path_item = paths.get(str(target.get("id")))
            if path_item is not None:
                for member_id in inner["impl"].get("items", []):
                    owners.setdefault(str(member_id), []).append((path_item["path"], False))

    result: list[dict[str, str]] = []
    seen: set[str] = set()
    for item in index.values():
        if item.get("crate_id") != 0 or item.get("inner", {}).get("function") is None:
            continue
        item_id = str(item.get("id"))
        name = item.get("name")
        if not name:
            continue
        item_paths: list[list[str]] = []
        path_item = paths.get(item_id)
        if path_item is not None and item.get("visibility") == "public":
            item_paths.append(path_item["path"])
        if not item_paths:
            for owner, is_trait in owners.get(item_id, []):
                # Methods use default visibility in rustdoc JSON. Their parent
                # public type, trait, or impl makes the method available.
                if (item.get("visibility") == "public" or is_trait) and owner not in item_paths:
                    item_paths.append(owner)
        if not item_paths:
            continue
        span = item.get("span")
        if not isinstance(span, dict) or not str(span.get("filename", "")).startswith("src/"):
            raise ValueError(f"public API {name} has no crate-relative source span")
        line = span.get("begin", [None])[0]
        if type(line) is not int or line < 1:
            raise ValueError(f"public API {name} has an invalid source line")
        for path in item_paths:
            api = "::".join(path if path[-1] == name else path + [name])
            if api in seen:
                raise ValueError(f"duplicate public API identity: {api}")
            seen.add(api)
            result.append({"api": api, "source": span["filename"], "line": str(line), "name": name})
    return sorted(result, key=lambda row: row["api"])


def collect(document: dict, ledger: list[dict[str, str]]) -> list[dict[str, str]]:
    apis = public_functions(document)
    by_api: dict[str, list[dict[str, str]]] = {}
    api_by_source_name: dict[tuple[str, str], list[dict[str, str]]] = {}
    extension_by_api: dict[str, tuple[dict[str, str], str]] = {}
    for api in apis:
        api_by_source_name.setdefault((api["source"], api["name"]), []).append(api)
    for row in ledger:
        is_extension_scope = row["id"].startswith(("HBC-EXT-", "HBC-API-EXT-")) or (
            row["id"].startswith("HBC-REMOTE-INPUT-") and "api_prefix=" in row.get("c_ref", "")
        )
        if is_extension_scope:
            scope_parts = row.get("c_ref", "").split(";")
            prefixes = [part.strip().removeprefix("api_prefix=").strip()
                        for part in scope_parts if part.strip().startswith("api_prefix=")]
            if not prefixes:
                continue
            reason = scope_parts[0].removeprefix("scope:").strip()
            for prefix in prefixes:
                matches = [api for api in apis if api["api"].startswith(prefix)]
                if not matches:
                    raise ValueError(f"{row['id']}: API extension prefix {prefix!r} matches no public APIs")
                for api in matches:
                    previous = extension_by_api.get(api["api"])
                    if previous is not None and previous[0]["id"] != row["id"]:
                        raise ValueError(
                            f"{api['api']}: extension scopes {previous[0]['id']} and {row['id']} overlap"
                        )
                    extension_by_api[api["api"]] = (row, reason)
            continue
        if not row["id"].startswith("HBC-API-"):
            continue
        api = resolve_api_reference(row["rust_ref"], api_by_source_name, row["id"])
        by_api.setdefault(api["api"], []).append(row)

    result = []
    for api in apis:
        evidence_rows = by_api.get(api["api"], [])
        proven = [row for row in evidence_rows if row["verification"] in PROVEN_VERIFICATION]
        extension = extension_by_api.get(api["api"])
        if proven and extension is not None:
            raise ValueError(f"{api['api']}: API has both parity evidence and an extension exemption")
        if proven:
            status = "partial"
            evidence = ";".join(row["id"] for row in proven)
            verification = ";".join(sorted({row["verification"] for row in proven}))
            limitations = "Evidence covers the named ledger cases only; full API contract and physical Wii behavior are not established."
        elif extension is not None:
            row, reason = extension
            status = "extension"
            evidence = row["id"]
            verification = "not_applicable"
            limitations = reason
        else:
            status = "unverified"
            evidence = ";".join(row["id"] for row in evidence_rows)
            verification = ";".join(sorted({row["verification"] for row in evidence_rows})) or "not_run"
            limitations = "No exact API-specific passing evidence is linked; broad area evidence may exist."
        result.append({key: api[key] for key in ("api", "source", "line")} |
                      {"status": status, "evidence": evidence, "verification": verification,
                       "limitations": limitations})
    return result


def resolve_api_reference(
    reference: str,
    api_by_source_name: dict[tuple[str, str], list[dict[str, str]]],
    row_id: str,
) -> dict[str, str]:
    source, separator, symbol = reference.partition("#")
    if not separator:
        raise ValueError(f"{row_id}: API reference must use path#function")
    anchor = symbol.removeprefix("pub ").removeprefix("unsafe ").removeprefix("const ")
    owner = None
    if anchor.startswith("fn "):
        name = anchor[3:].split("(", 1)[0].strip()
    elif "::" in anchor:
        owner, name = anchor.rsplit("::", 1)
        owner = owner.removeprefix("pub ").strip()
        name = name.removeprefix("fn ").strip()
    else:
        raise ValueError(f"{row_id}: API reference must use path#pub fn name or path#Type::name")
    matches = api_by_source_name.get((source, name), [])
    if owner is not None:
        suffix = f"::{owner}::{name}"
        matches = [api for api in matches if api["api"].endswith(suffix)]
        if len(matches) != 1:
            raise ValueError(f"{row_id}: API reference {reference!r} matches {len(matches)} public APIs")
    elif len(matches) != 1:
        raise ValueError(f"{row_id}: API reference {reference!r} matches {len(matches)} public APIs")
    return matches[0]


def render_csv(rows: list[dict[str, str]]) -> str:
    stream = io.StringIO(newline="")
    writer = csv.DictWriter(stream, fieldnames=FIELDS, lineterminator="\n")
    writer.writeheader()
    writer.writerows(rows)
    return stream.getvalue()


def render_status(rows: list[dict[str, str]]) -> str:
    counts = {state: sum(row["status"] == state for row in rows)
              for state in ("verified", "partial", "blocked", "extension", "unverified")}
    lines = [
        "# HBC public API evidence status", "",
        "Generated by `python3 tools/api_inventory.py refresh`. Do not edit this file.", "",
        "Scope: default-feature library APIs for `powerpc-nintendo-none`, extracted from rustdoc JSON. "
        "Binary-only entry points and feature-specific APIs are not included yet.", "",
        f"Public functions and trait methods: **{len(rows)}** — "
        f"verified: **{counts['verified']}** | partial: **{counts['partial']}** | "
        f"blocked: **{counts['blocked']}** | extension: **{counts['extension']}** | "
        f"unverified: **{counts['unverified']}**.", "",
        "`partial` means at least one exact API-specific check passes; it does not mean the API contract is complete. "
        "`extension` means the parity ledger explicitly records that the API has no matching C HBC contract. "
        "`unverified` means the API has no linked exact passing check. A broad feature test may still cover it. "
        "No APIs are promoted to `verified` or `blocked` by inference. Physical Wii evidence is separate and remains pending where the parity ledger says so.", "",
        "The machine-readable API list is [api-inventory.csv](api-inventory.csv). See [API verification](../api-verification.md) for scope and commands.", "",
    ]
    return "\n".join(lines)


def generate_rustdoc() -> None:
    env = os.environ.copy()
    env["RUSTC"] = subprocess.check_output(["rustup", "which", "--toolchain", "nightly", "rustc"], text=True).strip()
    env["RUSTDOC"] = subprocess.check_output(["rustup", "which", "--toolchain", "nightly", "rustdoc"], text=True).strip()
    env["RUSTFLAGS"] = "--check-cfg=cfg(gamecube)"
    command = ["rustup", "run", "nightly", "cargo", "rustdoc", "--lib", "--target",
               "../libogc-rs/targets/powerpc-nintendo-none.json", "-Z", "build-std=core,alloc",
               "-Z", "build-std-features=compiler-builtins-mem", "-Z", "json-target-spec", "--",
               "-Z", "unstable-options", "--output-format", "json"]
    subprocess.run(command, cwd=ROOT, env=env, check=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("check", "refresh"))
    args = parser.parse_args()
    try:
        generate_rustdoc()
        document = json.loads(RUSTDOC_JSON.read_text(encoding="utf-8"))
        rows = collect(document, parity.load_rows())
        artifacts = {CSV_REPORT: render_csv(rows), STATUS_REPORT: render_status(rows)}
        if args.command == "refresh":
            for path, contents in artifacts.items():
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(contents, encoding="utf-8")
                print(f"updated {path.relative_to(ROOT)}")
        else:
            stale = [path.relative_to(ROOT) for path, contents in artifacts.items()
                     if not path.is_file() or path.read_text(encoding="utf-8") != contents]
            if stale:
                raise ValueError(f"stale API inventory reports: {', '.join(map(str, stale))}; run python3 tools/api_inventory.py refresh")
            counts = {state: sum(row["status"] == state for row in rows)
                      for state in ("verified", "partial", "blocked", "extension", "unverified")}
            print("API inventory is current: " + " | ".join(f"{key}: {value}" for key, value in counts.items()))
    except (OSError, ValueError, subprocess.CalledProcessError, json.JSONDecodeError) as error:
        print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
