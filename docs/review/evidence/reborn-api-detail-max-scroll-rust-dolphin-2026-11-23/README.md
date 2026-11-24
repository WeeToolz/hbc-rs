# Detail memo maximum scroll API

The `detail-focus-api` acceptance case calls
`AppController::detail_description_max_scroll` after setting the controller's
measured memo bound to three lines. It requires the API to return three. The
case also checks the Load default and detail action focus boundaries.

The production acceptance DOL passed in Dolphin. The result JSON and raw case
log are archived in this directory. The DOL SHA-256 is
`72f1f1e6f852ac0df561c7101abb4f6ada9620e7cfa4f88fc695d591086e5eed`.

This verifies the controller state API in Dolphin. It does not verify rendered
memo geometry, physical pointer input, or Wii behavior.
