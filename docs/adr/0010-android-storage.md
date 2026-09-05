# ADR 0010: Android storage is deferred until verified

Android is a planned first-class experience, but this milestone does not claim an APK.
The storage strategy will use app-private storage where appropriate plus explicit
Storage Access Framework export/import, without broad storage permission. It will be
finalized after Tauri mobile behavior is built and tested on a real emulator/device.
