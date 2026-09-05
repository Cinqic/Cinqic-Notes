# Platform status

The current development milestone has been built and smoke-tested on Windows x64
through the Tauri debug MSI path. Linux artifacts are prepared in CI but are not
claimed as locally tested from this Windows development environment.

Android is intentionally deferred. The app has no Android package, permissions, or
Storage Access Framework claim yet. A future mobile phase must implement app-private
storage plus explicit import/export and verify an APK on an emulator before documenting
support or adding Android to a release matrix.
