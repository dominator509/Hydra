use std::marker::PhantomData;

#[allow(clippy::just_underscores_and_digits)]
pub fn same_type<T>(_1: &T, _2: &T) {}

pub struct WrapSame<T, U>(PhantomData<T>, PhantomData<U>);

impl<T, U> WrapSame<T, U> {
    pub fn new(_arg: &U) -> Self {
        WrapSame(PhantomData, PhantomData)
    }
}

pub trait WrapSameExt: Sized {
    type Wrapped;

    fn wrap_same(self) -> Self::Wrapped {
        panic!("only for type resolution")
    }
}

impl<T, U> WrapSameExt for WrapSame<T, Option<U>> {
    type Wrapped = Option<T>;
}

impl<T, U> WrapSameExt for &'_ WrapSame<T, U> {
    type Wrapped = T;
}

pub struct MatchBorrow<T, U>(PhantomData<T>, PhantomData<U>);

impl<T, U> MatchBorrow<T, U> {
    pub fn new(t: T, _u: &U) -> (T, Self) {
        (t, MatchBorrow(PhantomData, PhantomData))
    }
}

pub trait MatchBorrowExt: Sized {
    type Matched;

    fn match_borrow(self) -> Self::Matched {
        panic!("only for type resolution")
    }
}

impl<'a> MatchBorrowExt for MatchBorrow<Option<&'a str>, Option<String>> {
    type Matched = Option<&'a str>;
}

impl<'a> MatchBorrowExt for MatchBorrow<Option<&'a [u8]>, Option<Vec<u8>>> {
    type Matched = Option<&'a [u8]>;
}

impl<'a> MatchBorrowExt for MatchBorrow<Option<&'a str>, Option<&'a String>> {
    type Matched = Option<&'a str>;
}

impl<'a> MatchBorrowExt for MatchBorrow<Option<&'a [u8]>, Option<&'a Vec<u8>>> {
    type Matched = Option<&'a [u8]>;
}

impl<'a> MatchBorrowExt for MatchBorrow<&'a str, String> {
    type Matched = &'a str;
}

impl<'a> MatchBorrowExt for MatchBorrow<&'a [u8], Vec<u8>> {
    type Matched = &'a [u8];
}

impl<T> MatchBorrowExt for MatchBorrow<&'_ T, T> {
    type Matched = T;
}

impl<T> MatchBorrowExt for MatchBorrow<&'_ &'_ T, T> {
    type Matched = T;
}

impl<T> MatchBorrowExt for MatchBorrow<T, &'_ T> {
    type Matched = T;
}

impl<T> MatchBorrowExt for MatchBorrow<T, &'_ &'_ T> {
    type Matched = T;
}

impl<T> MatchBorrowExt for MatchBorrow<Option<&'_ T>, Option<T>> {
    type Matched = Option<T>;
}

impl<T> MatchBorrowExt for MatchBorrow<Option<&'_ &'_ T>, Option<T>> {
    type Matched = Option<T>;
}

impl<T> MatchBorrowExt for MatchBorrow<Option<T>, Option<&'_ T>> {
    type Matched = Option<T>;
}

impl<T> MatchBorrowExt for MatchBorrow<Option<T>, Option<&'_ &'_ T>> {
    type Matched = Option<T>;
}

impl<T, U> MatchBorrowExt for &'_ MatchBorrow<T, U> {
    type Matched = U;
}

pub fn conjure_value<T>() -> T {
    panic!()
}

pub fn dupe_value<T>(_t: &T) -> T {
    panic!()
}
