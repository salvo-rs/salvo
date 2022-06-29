#[derive(Clone, Debug, Extractible)]
#[salvo::extract(default_from(path_params))]
struct Article {
    #[salvo::extract(from(params))]
    name: String,
    #[salvo::extract(from(params, queries, name = "desc", format = "json"))]
    description: String,
}
