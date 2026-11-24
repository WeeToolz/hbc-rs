# FAT storage APIs in Dolphin

The production `fat-storage-api` acceptance case passed in Dolphin 2609a. The runner used a private folder-synced SD fixture and called the target-only `FatStorage` implementation.

The case checks SD mount and unmount, `sd:` prefix, file size, path and directory queries, bounded reads, an opened file, directory enumeration, missing files, invalid traversal, idempotent directory creation, file/directory collision handling, ZIP extraction with and without progress, resulting file sizes, the safe no-op result for rename recovery on distinct directories, directory rename, PNG screenshot save and readback, and recursive removal of app and directory trees containing files.

The archive input is `tests/fixtures/wiiload-app.zip` (SHA-256 `a29961ae3c8059b50de4e1fe08aff403ca13b2b680e3a5dfe3f1c191f23a611b`). The DOL identity is in `build.json`; `result.json` records the isolated profile and case result.

This proves Dolphin folder-synced SD behavior only. Physical Wii SD/FAT behavior, hot-plug, IOS DMA/cache behavior, and write failures remain pending. The interrupted-rename test covers only the safe no-op case for separate directories; true alias recovery remains unverified.
