# Unreleased

- ***Breaking*** The entire API for headless contexts has been removed. Please instead use `Context::new()` when trying to make a context without a visible window. Also removed `headless` feature.
- ***Breaking*** Types implementing the `GlContext` trait must now be sized.
- ***Breaking*** Added new `CreationErrorPair` enum variant to enum `CreationError`.

# Version 0.18.0 (2018-08-03)

- cocoa and core-graphics updates.
- **Breaking:** Added `OsError` variant to `ContextError`.
- Improved glX error reporting.
- The iOS backend no longer fails to compile... again (added iOS testing on CI to prevent further issues).
- Update winit dependency to 0.17.0. See [winit's CHANGELOG](https://github.com/tomaka/winit/blob/v0.17.0/CHANGELOG.md#version-0170-2018-08-02) for more info.

# Version 0.17.0 (2018-06-27)

- Fix regression that prevented automatic graphics switching in MacOS ([#980](https://github.com/tomaka/glutin/issues/980))
- Add `ContextBuilder::with_double_buffer` function
- Add `ContextBuilder::with_hardware_acceleration` function
- Work around a presumed Android emulator bug
  that would cause context creation to return `CreationError::OpenGlVersionNotSupported`
  in some configurations
  ([#1036](https://github.com/tomaka/glutin/pull/1036))
- Update winit dependency to 0.16.0. See [winit's CHANGELOG](https://github.com/tomaka/winit/blob/v0.16.0/CHANGELOG.md#version-0160-2018-06-25) for more info.
- The iOS backend no longer fails to compile.

# Version 0.16.0 (2018-05-09)

- Update winit dependency to 0.14.0. See [winit's CHANGELOG](https://github.com/tomaka/winit/blob/v0.14.0/CHANGELOG.md#version-0140-2018-05-09) for more info.
- Update winit dependency to 0.15.0. See [winit's CHANGELOG](https://github.com/tomaka/winit/blob/v0.15.0/CHANGELOG.md#version-0150-2018-05-22) for more info.

# Version 0.15.0 (2018-04-25)

- Update winit dependency to 0.13.0. See [winit's CHANGELOG](https://github.com/tomaka/winit/blob/v0.13.0/CHANGELOG.md#version-0130-2018-04-25) for more info.

# Version 0.14.0 (2018-04-06)

- Update winit dependency to 0.12.0
- Update wayland backend to not use deprecated `get_inner_size_points` method.

# Version 0.13.1 (2018-03-07)

- Fix android activity life cycle
- Update winit dependency to 0.11.2

# Version 0.13.0 (2018-02-28)

- Update winit dependency to 0.11.1

# Version 0.12.2 (2018-02-12)

- Don't use yanked version of winit

# Version 0.12.1 (2018-02-05)

- Add support for winapi 0.3 ([#975](https://github.com/tomaka/glutin/pull/975))
- Fix MacOS to return compatibility profile if applicable (#[977](https://github.com/tomaka/glutin/pull/977))
- Update gl_generator and macos dependencies
