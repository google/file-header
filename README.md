file-header
==========
A Rust library to check for and add headers to files.

A _header_ can be any arbitrary text, but we provide an `spdx` feature, which when enabled
allows usage of any [SPDX](https://spdx.dev/) license known to the [`license` crate](https://crates.io/crates/license) as headers. For easy use of 
this feature, we have already defined structs to support some commonly used licenses:
* [Apache-2.0](https://spdx.org/licenses/Apache-2.0.html) 
* [MIT](https://spdx.org/licenses/MIT.html)
* [BSD-3-Clause](https://spdx.org/licenses/BSD-3-Clause.html)
* [GPL-3.0-Only](https://spdx.org/licenses/GPL-3.0-only.html)
* [EPL-2.0](https://spdx.org/licenses/EPL-2.0.html)
* [MPL-2.0](https://spdx.org/licenses/MPL-2.0.html)
