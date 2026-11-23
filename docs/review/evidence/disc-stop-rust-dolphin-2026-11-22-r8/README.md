# Rust HBC asynchronous DI stop acceptance

This run uses the production `disc::start_stop` and `disc::shutdown` APIs. The current acceptance DOL passed in Dolphin. It reported callback mask `7` (open, stop ioctl, close), successful open and stop ioctl, and a zero shutdown result.

The shutdown path waits for a deferred completion fence. The SDK runs deferred callbacks serially. The IPC callback queues the fence, returns, and then the IPC layer releases its request before the worker can run the fence. This prevents app handoff while the callback or its IPC request is still active. Failed async close results keep the descriptor for synchronous cleanup after the fence.

The run verifies emulator behavior only. IOS DI behavior on a physical Wii remains pending.
