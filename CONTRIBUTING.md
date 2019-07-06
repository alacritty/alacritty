# Contributing to Alacritty

Thank you for your interest in contributing to Alacritty!

Table of Contents:

1. [Feature Requests](#feature-requests)
2. [Bug Reports](#bug-reports)
3. [Patches / Pull Requests](#patches--pull-requests)
    1. [Testing](#testing)
    2. [Performance](#performance)
    3. [Documentation](#documentation)
    4. [Style](#style)
4. [Contact](#contact)

## Feature Requests

Feature requests should be reported in the [Alacritty issue tracker](https://github.com/jwilm/alacritty/issues). To reduce the number of duplicates, please make sure to check the existing [enhancement](https://github.com/jwilm/alacritty/issues?utf8=%E2%9C%93&q=is%3Aissue+label%3Aenhancement) and [missing feature](https://github.com/jwilm/alacritty/issues?utf8=%E2%9C%93&q=is%3Aissue+label%3A%22B+-+missing+feature%22) issues.

## Bug Reports

Bug reports should be reported in the [Alacritty issue tracker](https://github.com/jwilm/alacritty/issues).

If a bug was not present in a previous version of Alacritty, providing the exact commit which introduced the regression helps out a lot.

Since a multitude of operating systems are supported by Alacritty, not all issues might apply to every OS. So make sure to specify on which OS the bug has been found. Since Linux has a variety of window managers, compositors and display servers, please also specify those when encountering an issue on Linux.

Depending on the bug, it might also be useful to provide some of the following information:
 - Configuration file
 - `alacritty -v(vv)` output
 - `alacritty --print-events` output
 - `glxinfo` output
 - `xrandr` output

Here's a template that you can use to file a bug, though it's not necessary to use it exactly:

```
# System
|                  |                               |
|------------------|-------------------------------|
| Operating System | [Linux/BSD/macOS/Windows]     |
| Rust Version     | [stable/beta/nightly/X.Y.Z]   |
| Display Server   | [X11/Wayland] (only on Linux) |
| Window Manager   | [i3/xfwm/...] (only on Linux) |
| Compositor       | [compton/...] (only on Linux) |

# Summary
[Short summary of the Bug]

# Behavior
[Description of Alacritty's current behavior]

# Expectation
[Description of expected behavior]

# Extra
[Additional information like config or logs]
```

## Patches / Pull Requests

All patches have to be sent on Github as [pull requests](https://github.com/jwilm/alacritty/pulls).

If you are looking for a place to start contributing to Alacritty, take a look at the [help wanted](https://github.com/jwilm/alacritty/issues?q=is%3Aopen+is%3Aissue+label%3A%22help+wanted%22) and [easy](https://github.com/jwilm/alacritty/issues?q=is%3Aopen+is%3Aissue+label%3A%22D+-+easy%22) issues.

Please note that the minimum supported version of Alacritty is Rust 1.32.0. All patches are expected to work with the minimum supported version.

### Testing

To make sure no regressions were introduced, all tests should be run before sending a pull request. The following command can be run to test Alacritty:

```
cargo test
```

Additionally if there's any functionality included which would lend itself to additional testing, new tests should be added. These can either be in the form of Rust tests using the `#[test]` annotation, or Alacritty's ref tests.

To record a new ref test, a release version of the patched binary should be created and run with the `--ref-test` flag. After closing the Alacritty window, or killing it (`exit` and `^D` do not work), some new files should have been generated in the working directory. Those can then be copied to the `./tests/ref/NEW_TEST_NAME` directory and the test can be enabled by editing the `ref_tests!` macro in the `./tests/ref.rs` file. When fixing a bug, it should be checked that the ref test does not complete correctly with the unpatched version, to make sure the test case is covered properly.

### Performance

Alacritty mainly uses the [vtebench](https://github.com/jwilm/vtebench) tool for testing Alacritty's performance. Any change which could have an impact on Alacritty's performance, should be tested with it to prevent potential regressions.

### Documentation

Code should be documented where appropriate. The existing code can be used as a guidance here and the general `rustfmt` rules can be followed for formatting.

If any change has been made to the `config.rs` file, these changes should also be documented in the example configuration file `alacritty.yml`.

Changes compared to the latest Alacritty release which have a direct effect on the user (opposed to things like code refactorings or documentation/tests) additionally need to be documented in the `CHANGELOG.md`. The existing entries should be used as a style guideline. The change log should be used to document changes from a user-perspective, instead of explaining the technical background (like commit messages). More information about Alacritty's change log format can be found [here](https://keepachangelog.com).

### Style

All Alacritty changes are automatically verified by CI to conform to its rustfmt guidelines. If a CI build is failing because of formatting issues, you can install rustfmt using `rustup component add rustfmt` and then format all code using `cargo fmt`.

# Contact

If there are any outstanding questions about contributing to Alacritty, they can be asked on the [Alacritty issue tracker](https://github.com/jwilm/alacritty/issues).

As a more immediate and direct form of communication, the Alacritty IRC channel (`#alacritty` on Freenode) can be used to contact many of the Alacritty contributors.
