# Reborn HOME overlay control API Dolphin evidence

The `overlay-control-api` production acceptance case passed in Dolphin 2609a.
It calls the production `OpenSignal` and `OverlayControl` APIs. The result,
DOL build manifest, and raw case log are archived beside this file.

The case checks a new signal, repeated request coalescing, one-shot consume,
closed-state errors, duplicate open, one pending action, mismatched completion,
failed action retention, retry, explicit close, failed system-menu exit, and
successful power-off close.

Reborn's `hbc_agent_home_pending` owns its request flag inside the C agent, and
`hbc_agent_home_fb` owns the open overlay loop. Rust exposes an application
owned state machine. It returns typed errors and keeps the overlay open until
the host confirms a destructive or exit action succeeded. This is a safety
difference: an error cannot leave the caller believing that the Wii has exited
or powered off. This probe covers the control state only; it does not prove
framebuffer capture, controller behavior, GX drawing, or physical Wii exit
actions.
