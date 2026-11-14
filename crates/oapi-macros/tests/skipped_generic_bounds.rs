//! Skipped PhantomData should not require a schema for its type parameter.
#![allow(dead_code)]
use std::marker::PhantomData;

use salvo::oapi::{Components, ToSchema};

#[derive(ToSchema)]
struct Marker<T> {
    value: i32,
    #[salvo(schema(skip))]
    marker: PhantomData<T>,
}

struct NoSchema;

#[test]
fn skipped_generic_parameter_does_not_need_to_schema() {
    let _ = Marker::<NoSchema>::to_schema(&mut Components::new());
}
