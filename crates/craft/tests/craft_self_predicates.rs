//! Service predicates keep their original meaning in crafted handler wrappers.
use salvo::prelude::*;
use salvo::test::{ResponseExt, TestClient};
use salvo_craft::craft;

trait Values {
    type Value;
}

#[derive(Clone, Debug)]
struct Service<T> {
    value: T,
}

impl<T> Values for Service<T> {
    type Value = T;
}

#[craft]
impl<T: Clone + Send + Sync + std::fmt::Display + 'static> Service<T>
where
    Self: Clone + Values,
    <Self as Values>::Value: std::fmt::Display,
{
    #[craft(handler)]
    fn show(&self) -> String {
        self.value.to_string()
    }
}

#[tokio::test]
async fn impl_self_and_associated_type_predicates_still_name_the_service() {
    let router = Router::new().get(Service { value: 7_u32 }.show());
    let body = TestClient::get("http://localhost/")
        .send(router)
        .await
        .take_string()
        .await
        .unwrap();
    assert_eq!(body, "7");
}

trait ConstantBounds {
    type Wrapped<T>;
    const WIDTH: usize;
}

#[derive(Clone)]
struct ConstantService;
impl ConstantBounds for ConstantService {
    type Wrapped<T> = Option<T>;
    const WIDTH: usize = 3;
}
impl ConstantService {
    const LEN: usize = 3;
    const fn size<T>() -> usize {
        std::mem::size_of::<T>()
    }
}

#[craft]
impl ConstantService
where
    [u8; Self::LEN]: Sized,
    [u8; Self::size::<Self>()]: Sized,
    [u8; <Self as ConstantBounds>::WIDTH]: Sized,
    <Self as ConstantBounds>::Wrapped<Self>: Default,
    [u8; {
        #[allow(clippy::identity_op)]
        let n = Self::LEN + 0;
        n
    }]: Sized,
{
    #[craft(handler)]
    fn constants(&self) -> String {
        Self::LEN.to_string()
    }
}

#[tokio::test]
async fn constant_predicates_keep_service_self_and_nested_arguments() {
    let body = TestClient::get("http://localhost/")
        .send(Router::new().get(ConstantService.constants()))
        .await
        .take_string()
        .await
        .unwrap();
    assert_eq!(body, "3");
}

trait ScopedWidth {}
impl ScopedWidth for [u8; 5] {}

#[derive(Clone)]
struct NestedScopeService;
impl NestedScopeService {
    const LEN: usize = 3;
    const BASE: usize = 3;
}

#[craft]
impl NestedScopeService
where
    [u8; {
        struct Local;
        impl Local {
            const LEN: usize = 5;
            const fn size() -> usize {
                Self::LEN
            }
        }
        Local::size()
    }]: ScopedWidth,
    [u8; {
        trait LocalWidth {
            const BASE: usize;
            const WIDTH: usize = Self::BASE;
        }
        struct Local;
        impl LocalWidth for Local {
            const BASE: usize = 5;
        }
        <Local as LocalWidth>::WIDTH
    }]: ScopedWidth,
    [u8; {
        trait LocalType {
            type Associated;
        }
        struct Local;
        impl LocalType for Local {
            type Associated = Self;
        }
        let _: Option<<Local as LocalType>::Associated> = None::<Local>;
        5
    }]: ScopedWidth,
{
    #[craft(handler)]
    fn nested(&self) -> &'static str {
        debug_assert_eq!((Self::LEN, Self::BASE), (3, 3));
        "nested"
    }
}

#[tokio::test]
async fn nested_impl_and_trait_items_keep_their_own_self_bindings() {
    let body = TestClient::get("http://localhost/")
        .send(Router::new().get(NestedScopeService.nested()))
        .await
        .take_string()
        .await
        .unwrap();
    assert_eq!(body, "nested");
}
