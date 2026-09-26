//! # fermium-units
//!
//! Fermium's units: dimensions with rational exponents, the unit database, the CODATA constants,
//! natural/nuclear/astro unit systems and number printing. A port of Fermium 1.5 (fermium/units.py,
//! fermium/constants.py, fermium/natural.py and the printing in fermium/runtime/core.py), which is the
//! oracle: `cargo test -p fermium-units` checks this crate against fixtures generated from the Python
//! implementation by rust/tools/units_fixtures.py (see PARITY.md).
//!
//! ## API
//!
//! Dimensions ([`dim`]):
//! - [`Dim`]`(pub [Rational64; 7])`: exponents of (m, kg, s, A, K, mol, cd), Python's `Dim.e` order.
//!   `Mul`, `Div`, [`Dim::pow`], [`Dim::powi`], [`Dim::base`], [`Dim::is_dimensionless`], [`DIMLESS`]
//!   and [`LENGTH`], [`MASS`], [`TIME`], [`CURRENT`], [`TEMPERATURE`], [`AMOUNT`], [`LUMINOSITY`].
//! - [`format_dim`] ("kg m/s²"), [`fmt_exp`], [`join_units`], [`superscript`].
//!
//! Units ([`db`], [`parse`]):
//! - [`Unit`] `{ name, dim, factor, offset }`: SI value = x · factor + offset; [`Unit::affine`] (°C/°F),
//!   [`Unit::mul`], [`Unit::div`], [`Unit::pow`], [`Unit::to_si`], [`Unit::from_si`].
//! - [`lookup_unit`] (one name, possibly prefixed), [`is_unit_name`], [`parse_unit_string`]
//!   ("J/(mol K)"; errors are [`UnitSyntaxError`] with Python's messages).
//! - Tables: [`PREFIXES`], [`AFFINE`], [`UNIT_PRETTY`] / [`unit_pretty`] / [`unit_ascii`],
//!   [`SPELLED_UNITS`] / [`spelled_unit`], [`UNIT_NAMES_LONG`] / [`unit_name_long`], [`unit_names`].
//! - The non-SI factors come from fermium/selfhost/units_db.fm, evaluated by build.rs at build time
//!   ([`self_hosted_factors`]).
//!
//! Constants ([`constants`]): [`constants()`] (every name and alias, like Python's `all_constants()`),
//! [`constant`] (by name), [`constant_defs`] (the table with aliases).

pub mod constants;
pub mod db;
pub mod dim;
pub mod numfmt;
pub mod parse;

pub use constants::{constant, constant_defs, constants, Constant, ConstantDef};
pub use db::{
    is_unit_name, lookup_unit, self_hosted_factors, spelled_unit, unit_ascii, unit_name_long, unit_names,
    unit_pretty, Unit, AFFINE, MICRO_SIGN, MU, PREFIXES, SPELLED_UNITS, UNIT_NAMES_LONG, UNIT_PRETTY,
};
pub use dim::{
    format_dim, format_dim_style, fmt_exp, join_units, superscript, Dim, AMOUNT, BASE_NAMES, BASE_SYMBOLS,
    CURRENT, DIMLESS, LENGTH, LUMINOSITY, MASS, TEMPERATURE, TIME,
};
pub use num_rational::Rational64;
pub use parse::{parse_unit_string, tokenize_unit, UnitSyntaxError};
