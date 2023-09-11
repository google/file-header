// Copyright 2023 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Constructs headers for SPDX licenses from the `license` crate.
//!
//! Some licenses are effectively templates: certain tokens like `<yyyy>` or `year` are intended
//! to be replaced with some user-defined value, like the copyright year in this case. These are
//! represented by the [LicenseTokens] trait, with the replacement values needed to construct the
//! final header text defined by [LicenseTokens::TokenReplacementValues]. If no tokens need to be
//! replaced, [NoTokens] is available for that purpose.
//!
//! Several common licenses have structs defined, with `LicenseTokens` already appropriately
//! implemented (e.g. `APACHE_2_0`).
//!
//! Most other licenses don't need anything other than the copyright year and the copyright holder,
//! or have nothing at all, and can be easily turned into headers with [SpdxLicense], a type to
//! define the tokens to replace (or [NoTokens]), and [YearCopyrightOwnerValue].
//!
//! If you find yourself needing a license that's not already available easily with a struct in
//! this module, see the examples below, and consider making a PR to add support.
//!
//! # Examples
//!
//! ## Getting a header for the Apache 2.0 license:
//!
//! ```
//! // Copyright 2023 Google LLC.
//! // SPDX-License-Identifier: Apache-2.0
//! use std::path;
//! use file_header::license::spdx::*;
//!
//! // Apache 2 has all relevant types already defined, and just needs year and name
//! let header = APACHE_2_0.build_header(YearCopyrightOwnerValue::new(2023, "Some copyright holder".to_string()));
//!
//! // use normal header API to check or add
//!```
//!
//! ## Getting a header for an SPDX license with no tokens to replace
//!
//! ```
//! // Copyright 2023 Google LLC.
//! // SPDX-License-Identifier: Apache-2.0
//! use file_header::license::spdx::*;
//!
//! let license = SpdxLicense::<NoTokens>::new(
//!    Box::new(license::licenses::Rpsl1_0),
//!    "Copyright (c) 1995-2002 RealNetworks, Inc. and/or its licensors".to_string(),
//!    10
//! );
//!
//! let header = license.build_header(());
//! ```
//!
//!
//! ## Getting a header for an SPDX license type that uses the typical year & name
//!
//!```
//! // Copyright 2023 Google LLC.
//! // SPDX-License-Identifier: Apache-2.0
//! use file_header::license::spdx::*;
//!
//! // Replacement tokens used in LGPL2
//! struct Lgpl2_0Tokens;
//!
//! impl LicenseTokens for Lgpl2_0Tokens {
//!    type TokenReplacementValues = YearCopyrightOwnerValue;
//!
//!    fn replacement_pairs(replacements: Self::TokenReplacementValues) -> Vec<(&'static str, String)> {
//!        vec![
//!             ("year", replacements.year.to_string()),
//!             ("name of author", replacements.copyright_owner),
//!         ]
//!    }
//! }
//!
//! let license = SpdxLicense::<Lgpl2_0Tokens>::new(
//!    Box::new(license::licenses::Lgpl2_0),
//!    "GNU Library General Public License as published by the Free Software Foundation; version 2.".to_string(),
//!    10
//! );
//!
//! let header = license.build_header(YearCopyrightOwnerValue::new(2023, "Foo Inc.".to_string()));
//! ```
//!
//! ## Getting a header for an unusual SPDX license
//!
//!```
//! // Copyright 2023 Google LLC.
//! // SPDX-License-Identifier: Apache-2.0
//! use file_header::license::spdx::*;
//!
//! /// Replacement values for `W3c20150513` license
//! struct W3c20150513Values {
//!    name_of_software: String,
//!    distribution_uri: String,
//!    date_of_software: String,
//! }
//!
//! impl W3c20150513Values   {
//!    fn new(name_of_software: String, distribution_uri: String, date_of_software: String,) -> Self {
//!        Self {
//!            name_of_software,
//!            distribution_uri,
//!            date_of_software
//!        }
//!    }
//! }
//!
//! /// Tokens in the W3c20150513 license
//! struct W3c20150513Tokens;
//!
//! impl LicenseTokens for W3c20150513Tokens {
//!    type TokenReplacementValues = W3c20150513Values;
//!
//!    fn replacement_pairs(
//!        replacements: Self::TokenReplacementValues,
//!    ) -> Vec<(&'static str, String)> {
//!        vec![
//!            ("$name_of_software", replacements.name_of_software),
//!            ("$distribution_URI", replacements.distribution_uri),
//!            // yes, it really does use dashes just for this one
//!            ("$date-of-software", replacements.date_of_software),
//!        ]
//!    }
//!}
//!
//! let license = SpdxLicense::<W3c20150513Tokens>::new(
//!    Box::new(license::licenses::W3c20150513),
//!    "This work is distributed under the W3C® Software License".to_string(),
//!    10
//! );
//!
//! let header = license.build_header(W3c20150513Values::new(
//!    "2023".to_string(),
//!    "https://example.com".to_string(),
//!    "Foo Inc.".to_string()));
//!
//! ```

use crate::{Header, SingleLineChecker};
use lazy_static::lazy_static;
use std::marker;

/// Re-export of the `license` crate for user convenience
pub use license;

#[cfg(test)]
mod tests;

/// A boxed `license::License`.
// Including the `Send` trait for compatibility with crates that use `lazy_static` with the `spin_no_std` feature.
type BoxedLicense = Box<dyn license::License + Sync + Send>;

/// Metadata around an SPDX license to enable constructing a [Header].
///
/// `<L>` is the [LicenseTokens] that defines what, if any, replacement tokens are needed.
pub struct SpdxLicense<L: LicenseTokens> {
    license_text: BoxedLicense,
    search_pattern: String,
    lines_to_search: usize,
    marker: marker::PhantomData<L>,
}

impl<L: LicenseTokens> SpdxLicense<L> {
    /// `spdx_license`: the SPDX license
    /// `search_pattern`: the text to search for when checking for the presence of the license
    /// `lines_to_search`: how many lines to search for `search_pattern` before giving up
    pub fn new(license_text: BoxedLicense, search_pattern: String, lines_to_search: usize) -> Self {
        Self {
            license_text,
            search_pattern,
            lines_to_search,
            marker: marker::PhantomData,
        }
    }

    /// Build a header for this license using the provided `year` and `copyright_holder` to
    /// interpolate into the license.
    /// The license's header is used, if the license offers one, otherwise the main license text
    /// is used instead.
    pub fn build_header(
        &self,
        replacement_values: L::TokenReplacementValues,
    ) -> Header<SingleLineChecker> {
        let checker = SingleLineChecker::new(self.search_pattern.clone(), self.lines_to_search);
        // use header, if the license has a specific header else the license text
        let text = self
            .license_text
            .header()
            .unwrap_or(self.license_text.text());

        let header = L::replacement_pairs(replacement_values).iter().fold(
            text.to_string(),
            |current_text, (replace_token, replace_value)| {
                // replacing only the first occurrence seems wise
                current_text.replacen(replace_token, replace_value, 1)
            },
        );

        Header::new(checker, header)
    }
}

/// Tokens in license text to be replaced, e.g. `yyyy` which will be replaced with the copyright
/// year.
pub trait LicenseTokens {
    /// Struct holding the replacement values needed for the tokens used by the license
    type TokenReplacementValues;
    /// List of `(token to search for, replacement value)`.
    fn replacement_pairs(replacements: Self::TokenReplacementValues)
        -> Vec<(&'static str, String)>;
}

/// For licenses with no tokens to replace
pub struct NoTokens;

impl LicenseTokens for NoTokens {
    type TokenReplacementValues = ();

    fn replacement_pairs(
        _replacements: Self::TokenReplacementValues,
    ) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}

/// Tokens for the Apache 2 license
#[doc(hidden)]
pub struct Apache2Tokens;

impl LicenseTokens for Apache2Tokens {
    type TokenReplacementValues = YearCopyrightOwnerValue;

    fn replacement_pairs(
        replacements: Self::TokenReplacementValues,
    ) -> Vec<(&'static str, String)> {
        vec![
            ("[yyyy]", replacements.year.to_string()),
            ("[name of copyright owner]", replacements.copyright_owner),
        ]
    }
}

/// Tokens for the MIT license
#[doc(hidden)]
pub struct MitTokens;

impl LicenseTokens for MitTokens {
    type TokenReplacementValues = YearCopyrightOwnerValue;

    fn replacement_pairs(
        replacements: Self::TokenReplacementValues,
    ) -> Vec<(&'static str, String)> {
        vec![
            ("<year>", replacements.year.to_string()),
            ("<copyright holders>", replacements.copyright_owner),
        ]
    }
}

/// Tokens for the BSD 3-clause license
#[doc(hidden)]
pub struct Bsd3ClauseTokens {}

impl LicenseTokens for Bsd3ClauseTokens {
    type TokenReplacementValues = YearCopyrightOwnerValue;

    fn replacement_pairs(
        replacements: Self::TokenReplacementValues,
    ) -> Vec<(&'static str, String)> {
        vec![
            ("<year>", replacements.year.to_string()),
            ("<owner>", replacements.copyright_owner),
        ]
    }
}

/// Tokens for the GPL-3.0 license
#[doc(hidden)]
pub struct Gpl3Tokens {}

impl LicenseTokens for Gpl3Tokens {
    type TokenReplacementValues = YearCopyrightOwnerValue;

    fn replacement_pairs(
        replacements: Self::TokenReplacementValues,
    ) -> Vec<(&'static str, String)> {
        vec![
            ("<year>", replacements.year.to_string()),
            ("<name of author>", replacements.copyright_owner),
        ]
    }
}

/// Replacement values for licenses that use a _year_ and _copyright owner name_.
pub struct YearCopyrightOwnerValue {
    /// The year of the copyright
    pub year: u32,
    /// The holder of the copyright
    pub copyright_owner: String,
}

impl YearCopyrightOwnerValue {
    /// Construct a new instance with the provided year and copyright owner
    pub fn new(year: u32, copyright_owner: String) -> Self {
        Self {
            year,
            copyright_owner,
        }
    }
}

lazy_static! {
    /// Apache 2.0 license
    pub static ref APACHE_2_0: SpdxLicense<Apache2Tokens> = SpdxLicense ::new(
        Box::new(license::licenses::Apache2_0),
         "Apache License, Version 2.0".to_string(),
        10
    );
}
lazy_static! {
    /// MIT license
    pub static ref MIT: SpdxLicense<MitTokens> = SpdxLicense ::new(
        Box::new(license::licenses::Mit),
         "MIT License".to_string(),
         10
    );
}
lazy_static! {
    /// BSD 3-clause license
    pub static ref BSD_3: SpdxLicense<Bsd3ClauseTokens> = SpdxLicense ::new(
        Box::new(license::licenses::Bsd3Clause),
         "Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:".to_string(),
         10
    );
}
lazy_static! {
    /// GPL 3.0 license
    pub static ref GPL_3_0_ONLY: SpdxLicense<Gpl3Tokens> = SpdxLicense ::new(
        Box::new(license::licenses::Gpl3_0Only),
         "GNU General Public License".to_string(),
         10
    );
}
lazy_static! {
    /// EPL 2.0 license
    pub static ref EPL_2_0: SpdxLicense<NoTokens> = SpdxLicense ::new(
        Box::new(license::licenses::Epl2_0),
         "Eclipse Public License - v 2.0".to_string(),
         10
    );
}
lazy_static! {
    /// MPL 2.0 license
    pub static ref MPL_2_0: SpdxLicense<NoTokens> = SpdxLicense ::new(
        Box::new(license::licenses::Mpl2_0),
         "Mozilla Public License, v. 2.0".to_string(),
         10
    );
}
