# Reborn app metadata parser evidence

This evidence combines two production checks for `hbc_rs::metadata::parse_app_xml`:

- Seventeen release-date inputs match an independent devkitPPC C runtime probe.
  See [the Rust Dolphin record](../reborn-metadata-rust-dolphin-2026-11-23-r1/README.md)
  and [the C probe record](../reborn-metadata-c-dolphin-2026-11-23-r2/README.md).
- The `metadata-app` acceptance case checks raw text values, ASCII `strtol`
  prefix parsing for `sort_id`, and both metadata flags in Dolphin. Its result
  is in this directory's `result.json`.

Reborn's `meta_parse` copies opaque XML content for text fields. It uses
`strtol` for `sort_id`, and it sets flags when their elements exist. Rust now
keeps that text content and skips only ASCII whitespace before its checked
sort-id conversion.

The argument model is an intentional Rust extension. Reborn joins the
`<arg>` values into one space-separated field. Rust keeps them as separate
strings and passes them as separate launch arguments, which avoids ambiguity
when an argument contains spaces. This check validates their raw text values;
it does not claim byte-for-byte parity for Reborn's combined argument field.

The API evidence remains partial. It does not cover every metadata field,
filesystem failure behavior, all date/timezone cases, or physical Wii behavior.
