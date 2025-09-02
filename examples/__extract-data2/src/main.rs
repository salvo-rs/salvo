use salvo::Extractible;
use salvo::macros::Extractible;
use salvo::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Extractible)]
pub struct PageRequest<T> {
    /// 页码
    #[serde(default)]
    pub page: u64,

    /// 每页大小
    #[serde(default)]
    pub limit: Option<u64>,

    /// 查询条件
    #[serde(flatten)]
    pub query: Option<T>,
}

// #[derive(Debug, Deserialize, Extractible)]
// pub struct PageRequest<T> {
//     /// 页码
//     #[serde(default)]
//     pub page: u64,

//     /// 每页大小
//     #[serde(default)]
//     pub limit: u64,

//     /// 查询条件
//     #[serde(flatten)]
//     pub query: T,
// }

// impl<'__macro_gen_ex, T> salvo::extract::Extractible<'__macro_gen_ex> for PageRequest<T>
// where
//     T: salvo::extract::Extractible<'__macro_gen_ex> + ::serde::de::Deserialize<'__macro_gen_ex>,
// {
//     fn metadata() -> &'static salvo::extract::Metadata {
//         static METADATA: ::std::sync::OnceLock<salvo::extract::Metadata> =
//             ::std::sync::OnceLock::new();
//             let tm: &'static salvo::extract::Metadata = <T as salvo::extract::Extractible<'__macro_gen_ex>>::metadata();
//         METADATA.get_or_init(|| {
//             let mut metadata = salvo::extract::Metadata::new("PageRequest");
//             let mut field = salvo::extract::metadata::Field::new("page");
//             metadata = metadata.add_field(field);
//             let mut field = salvo::extract::metadata::Field::new("limit");
//             metadata = metadata.add_field(field);
//             let mut field = salvo::extract::metadata::Field::new("query");
//             field = field.metadata(tm);
//             field = field.flatten(true);
//             metadata = metadata.add_field(field);
//             metadata
//         })
//     }
//     #[allow(refining_impl_trait)]
//     async fn extract(
//         req: &'__macro_gen_ex mut salvo::http::Request,
//     ) -> Result<Self, salvo::http::ParseError>
//     where
//         Self: Sized,
//     {
//         salvo::serde::from_request(req, Self::metadata()).await
//     }
// }

// #[derive(Debug, Deserialize, ToSchema)]
// pub struct PageRequest<T>  {
//     /// 页码
//     #[serde(default)]
//     pub page: u64,

//     /// 每页大小
//     #[serde(default)]
//     pub limit: u64,

//     /// 查询条件
//     #[serde(flatten)]
//     pub query: T,
// }

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::with_path("{id}");

    println!("Example url: http://0.0.0.0:5800/95");
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
