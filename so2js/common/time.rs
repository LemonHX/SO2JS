use crate::runtime::Context;

pub fn get_current_unix_time(ctx: &Context) -> f64 {
    ctx.sys
        .as_ref()
        .map(|sys| sys.current_time_millis())
        .unwrap_or(0.0)
}
