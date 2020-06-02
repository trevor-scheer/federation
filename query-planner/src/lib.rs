mod utils;
use graphql_parser::query::{parse_query};

use wasm_bindgen::prelude::*;

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

// #[wasm_bindgen]
// extern {
//     #[wasm_bindgen(js_namespace = console)]
//     fn log(s: &str);
// }

#[wasm_bindgen]
pub fn plan(_schema: &JsValue, _operation: &str) -> Result<String, JsValue> {
    let plan = include_str!("../mock_plans/one_plan.txt");
    // parse_query(_operation).unwrap();

    // let short = String::from(plan).lines();

    // log(operation);
    Ok(format!("{}", plan))
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
