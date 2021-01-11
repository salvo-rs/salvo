pub mod filter;
mod router;
pub use filter::*;
pub use router::{DetectMatched, Router};

use std::collections::HashMap;
pub type Params = HashMap<String, String>;

pub struct PathState {
    pub segements: Vec<String>,
    pub match_cursor: usize,
    pub params: Params,
}
impl PathState {
    pub fn new(segements: Vec<String>) -> Self {
        PathState {
            segements,
            match_cursor: 0,
            params: Params::new(),
        }
    }
}

// #[cfg(test)]
// mod test {

//     async fn list(_req: &mut Request, _depot: &mut Depot, res: &mut Response) {}

//     async fn read(_req: &mut Request, _depot: &mut Depot, res: &mut Response) {}

//     async fn delete(_req: &mut Request, _depot: &mut Depot, res: &mut Response) {}

//     async fn update(_req: &mut Request, _depot: &mut Depot, res: &mut Response) {}
//     #[test]
//     fn test_rest_match() {}

//     #[test]
//     fn test_param_match() {}

//     #[test]
//     fn test_method_filter() {}
//     #[test]
//     fn test_path_filter() {}
//     #[test]
//     fn test_logical_opts() {}
//     #[test]
//     fn test_complex() {}
// }
