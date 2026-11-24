# Executable loading API check

The `executable-loading-api` production acceptance case passed in Dolphin
2609a. It checks loading-screen entry, progress reset and bounds, completion,
repeat completion, and the matching `AppRuntime` forwarding methods.

The result and target DOL hash are in `result.json`. Reborn uses a loading
caption and activity indicator around its loader path. This state check does
not prove rendered UI equivalence, storage read timing, or physical Wii launch
behavior; those remain separate requirements.
