# Catalog reconciliation API check

The `catalog-reconcile-api` production acceptance case passed in Dolphin
2609a. It checks selection identity and page position after a sorted insertion,
first-entry fallback after removing the selected app, and cleared selection
for an empty catalog.

Reborn's refresh path positions its page around the selected entry when that
entry remains. The test uses a controlled catalog and does not prove physical
FAT refresh timing or rendered page transitions. The result and target DOL
hash are in `result.json`.
