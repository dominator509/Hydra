#[macro_export]
macro_rules! query {
    ($query:expr) => {{
        $crate::sqlx_macros::expand_query!(source = $query)
    }};
    ($query:expr, $($args:tt)*) => {{
        $crate::sqlx_macros::expand_query!(source = $query, args = [$($args)*])
    }};
}

#[macro_export]
macro_rules! query_as {
    ($out_struct:path, $query:expr) => {{
        $crate::sqlx_macros::expand_query!(record = $out_struct, source = $query)
    }};
    ($out_struct:path, $query:expr, $($args:tt)*) => {{
        $crate::sqlx_macros::expand_query!(record = $out_struct, source = $query, args = [$($args)*])
    }};
}

#[cfg(feature = "migrate")]
#[macro_export]
macro_rules! migrate {
    ($dir:literal) => {{
        $crate::sqlx_macros::migrate!($dir)
    }};
    () => {{
        $crate::sqlx_macros::migrate!("./migrations")
    }};
}
