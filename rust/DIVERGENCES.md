# Divergences of the Rust implementation from Fermium 1.5

Fermium 1.5 (Python) is the oracle (spec §B2). This file lists each place where the Rust implementation
deliberately behaves differently, with the conformance case ids it affects. Temporary divergences are
marked as such and disappear when the feature is ported.

## `use python` stops with one clear error (temporary)

Spec §B5.14: Python interop loads libpython at run time, and only when a program imports it. It is the last
milestone. Until then the checker handles the placement rules that don't need Python (top level only; not
inside a Fermium module). Any other `use python …` line stops with:

    use python needs Python interop, which this build doesn't have yet
      hint: run it with the Python implementation (legacy/) for now

Affected cases (area python-interop; 22 of 24 fail on purpose): 06c39fca6004 092b2cb0af63 3ff6452a9896
41c01f70ba40 43e3bfbe2b59 51e5813de4ee 5e2299e27fdd 5f1addc03ebf 72ac3f6e78b3 7437a9e2cca4 76e31071c42f
82370562a3c8 8b383857434d 9fc3b3db6503 afeacad54612 c11247eac4c7 d7c13acaf581 d8a5135d4e94 dcbf3421f1b3
e54c838bd4d0 ee9e8f8e830b f50e686a0691.

## The standard library is embedded in the binary

In v1, `import mechanics` finds `fermium/stdlib/mechanics.fm` next to the Python package. The Rust binary
embeds the same Fermium sources when it is built (`fermium-check/build.rs`), so a single binary works with no
files installed. Only the search path differs: the stdlib is searched last as before, but it is shown as
"the standard library" and its files as `stdlib/<name>.fm`, as v1 shows them. Output and messages are the
same as v1's.

## fermium.toml is read by a small TOML reader

v1 reads `fermium.toml` with Python's `tomllib`. The Rust reader accepts what v1's fallback reader
(`modules.py` `_mini_toml`) accepts: `[tables]` and `key = "text"` or `key = ["list", "of", "text"]`, with
`#` comments. For a malformed file, the message is the fallback reader's
(`… isn't a valid fermium.toml (line N: …)`) instead of `tomllib`'s. No conformance case has a malformed
`fermium.toml`.
