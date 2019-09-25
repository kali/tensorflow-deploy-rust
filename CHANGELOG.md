## Unreleased

### Notable

* Make onnx loader operator set aware, and Slice-10 support.

## 0.5.1 - 2019-09-24

### Bugfix

* remove the no_panic checks, as too fragile (breaking non-lto builds)

## 0.5.0 - 2019-09-20

### Breaking

* Change tensor facts names for consistency: TensorFact is now InferenceFact.

### Notable

* Introduce Windows support, including CI coverage for linalg
* Switch from Travis to GitHub Actions
* Internal refactoring around tract-core canonic opset
* Tract CLI can now compute a FLOP number for networks ("cost" subcommand). 
    Furthermore the CI asserts its value for a few networks to prevent optimisation regressions.
* Fix: handling of -1 in ONNX Reshape op

## 0.4.2 - 2019-09-10

* Fix release script after 0.4.1 release disaster.

## 0.4.1 - 2019-09-09 [YANKED]

* Fix for OS where CARGO_CFG_TARGET_FAMILY is undefined
* Linear Algebra package refactor
* tract-core canonic operator set introduction
* significant performance boost (up to 20% on some real-life networks)

## 0.4.0 - 2019-07-30

* Start Kaldi networks support (LSTM, Renorm, Affine, downsample)

## Before...

This Changelog started way too late. But better late than never.

