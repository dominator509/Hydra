#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc = "Hydra-local Postgres-only SQLx facade."]

pub use sqlx_core::acquire::Acquire;
pub use sqlx_core::arguments::{Arguments, IntoArguments};
pub use sqlx_core::column::{Column, ColumnIndex};
pub use sqlx_core::connection::{ConnectOptions, Connection};
pub use sqlx_core::database::{self, Database};
pub use sqlx_core::describe::Describe;
pub use sqlx_core::executor::{Execute, Executor};
pub use sqlx_core::from_row::FromRow;
pub use sqlx_core::pool::{self, Pool};
#[doc(hidden)]
pub use sqlx_core::query::query_with_result as __query_with_result;
pub use sqlx_core::query::{query, query_with};
pub use sqlx_core::query_as::{query_as, query_as_with};
pub use sqlx_core::query_builder::{self, QueryBuilder};
#[doc(hidden)]
pub use sqlx_core::query_scalar::query_scalar_with_result as __query_scalar_with_result;
pub use sqlx_core::query_scalar::{query_scalar, query_scalar_with};
pub use sqlx_core::raw_sql::{raw_sql, RawSql};
pub use sqlx_core::row::Row;
pub use sqlx_core::statement::Statement;
pub use sqlx_core::transaction::{Transaction, TransactionManager};
pub use sqlx_core::type_info::TypeInfo;
pub use sqlx_core::types::Type;
pub use sqlx_core::value::{Value, ValueRef};
pub use sqlx_core::Either;

#[doc(inline)]
pub use sqlx_core::error::{self, Error, Result};

#[cfg(feature = "migrate")]
pub use sqlx_core::migrate;

#[cfg(feature = "postgres")]
#[cfg_attr(docsrs, doc(cfg(feature = "postgres")))]
#[doc(inline)]
pub use sqlx_postgres::{
    self as postgres, PgConnection, PgExecutor, PgPool, PgTransaction, Postgres,
};

#[cfg(feature = "macros")]
#[doc(hidden)]
pub extern crate sqlx_macros;

#[cfg(feature = "migrate")]
#[doc(hidden)]
pub use sqlx_core::testing;

#[doc(hidden)]
pub use sqlx_core::rt::test_block_on;

#[cfg(feature = "macros")]
mod macros;

#[cfg(feature = "macros")]
#[doc(hidden)]
pub mod spec_error;

#[cfg(feature = "macros")]
#[doc(hidden)]
pub mod ty_match;

#[doc(hidden)]
pub use sqlx_core::rt as __rt;

pub mod types {
    pub use sqlx_core::types::*;
}

pub mod encode {
    pub use sqlx_core::encode::{Encode, IsNull};
}

pub use self::encode::Encode;

pub mod decode {
    pub use sqlx_core::decode::Decode;
}

pub use self::decode::Decode;

pub mod query {
    pub use sqlx_core::query::{Map, Query};
    pub use sqlx_core::query_as::QueryAs;
    pub use sqlx_core::query_scalar::QueryScalar;
}

pub mod prelude {
    pub use super::Acquire;
    pub use super::ConnectOptions;
    pub use super::Connection;
    pub use super::Decode;
    pub use super::Encode;
    pub use super::Executor;
    pub use super::FromRow;
    pub use super::IntoArguments;
    pub use super::Row;
    pub use super::Statement;
    pub use super::Type;
}
