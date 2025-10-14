use napi_derive::napi;

#[napi]
pub mod math {
    use jsonata_rust::functions::math as math_impl;

    #[napi]
    pub fn sum(args: Option<Vec<f64>>) -> Option<f64> {
        math_impl::sum(args.as_deref())
    }

    #[napi]
    pub fn max(args: Option<Vec<f64>>) -> Option<f64> {
        math_impl::max(args.as_deref())
    }

    #[napi]
    pub fn min(args: Option<Vec<f64>>) -> Option<f64> {
        math_impl::min(args.as_deref())
    }

    #[napi]
    pub fn average(args: Option<Vec<f64>>) -> Option<f64> {
        math_impl::average(args.as_deref())
    }
}
