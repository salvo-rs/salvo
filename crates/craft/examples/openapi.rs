#![allow(missing_docs)]

use std::sync::Arc;

use salvo::oapi::OpenApi;
use salvo::oapi::extract::QueryParam;
use salvo::prelude::*;
use salvo_craft::craft;

#[derive(Clone, Debug)]
pub struct Calculator {
    base: i64,
}

#[craft]
impl Calculator {
    #[craft(handler)]
    fn add(&self, left: QueryParam<i64>, right: QueryParam<i64>) -> String {
        (self.base + *left + *right).to_string()
    }

    #[craft(handler)]
    fn add_with_arc(self: Arc<Self>, left: QueryParam<i64>, right: QueryParam<i64>) -> String {
        (self.base + *left + *right).to_string()
    }

    #[craft(endpoint)]
    fn add_plain(left: QueryParam<i64>, right: QueryParam<i64>) -> String {
        (*left + *right).to_string()
    }
}

fn main() {
    let calculator = Arc::new(Calculator { base: 1 });
    let router = Router::new()
        .push(Router::with_path("add").get(calculator.add()))
        .push(Router::with_path("add-arc").get(calculator.add_with_arc()))
        .push(Router::with_path("add-plain").get(Calculator::add_plain()));

    let _doc = OpenApi::new("Craft Example", "0.1.0").merge_router(&router);
}
