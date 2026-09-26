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
//! Display ([`display`]): [`preferred_unit`] (the SI unit a dimension prints in: N, J/K, V/m², kg/(m s²)),
//! [`display_unit`] (the hint if its dimension fits, else the preferred unit), [`dim_name`] ("energy [J]"),
//! [`suggest_units`] ("energy × length", ["J m", "eV nm"]).
//!
//! Number printing ([`numfmt`], [`quantity`]): [`format_number`]`(x, sig, trim)`, [`format_default`] (D11:
//! 3 figures, whole numbers exact), [`format_default_seq`], [`format_written`], [`is_whole`], [`format_pm`];
//! [`format_quantity`]`(v, dim, hint, sf, direct, echo, whole_ok)` and, with a [`PrintFmt`], [`format_value`],
//! [`format_list`], [`format_vec`], [`format_mvec`], [`format_mat`], [`format_complex`], [`format_clist`],
//! [`format_uncertain`], [`format_uncertain_list`].
//!
//! Unit systems ([`natural`]): [`make_system`]`("natural" | "nuclear" | "astro" | "SI", consts)` gives a
//! [`UnitSystem`] with `canon_dim`, `factor`, `canon_unit`, `const_value`, `display_unit`, `describe`,
//! `label`; [`astro_display`].
//!
//! Constants ([`constants`]): [`constants()`] (every name and alias, like Python's `all_constants()`),
//! [`constant`] (by name), [`constant_defs`] (the table with aliases).

pub mod constants;
pub mod db;
pub mod dim;
pub mod display;
pub mod natural;
pub mod numfmt;
pub mod parse;
pub mod quantity;

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
pub use display::{dim_name, display_unit, preferred_unit, suggest_units};
pub use natural::{astro_display, make_system, UnitSystem};
pub use numfmt::{format_default, format_default_seq, format_number, format_pm, format_written, is_whole};
pub use quantity::{
    format_clist, format_complex, format_list, format_mat, format_mvec, format_quantity, format_uncertain,
    format_uncertain_list, format_value, format_vec, PrintFmt,
};
