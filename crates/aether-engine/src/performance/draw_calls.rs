use std::cell::Cell;

thread_local! {
    static DRAW_CALL_COUNT: Cell<Option<u64>> = const { Cell::new(None) };
}

/// Count one renderer draw call inside the current profiling scope.
pub fn record_draw_call() {
    DRAW_CALL_COUNT.with(|count| {
        if let Some(current) = count.get() {
            count.set(Some(current.saturating_add(1)));
        }
    });
}

/// Run one render schedule and return its issued draw-call count.
pub fn capture_draw_calls<T>(render: impl FnOnce() -> T) -> (T, u64) {
    let previous = DRAW_CALL_COUNT.with(|count| count.replace(Some(0)));
    let restore = DrawCallCountRestore(previous);
    let result = render();
    let calls = DRAW_CALL_COUNT.with(|count| count.get().unwrap_or_default());
    drop(restore);
    (result, calls)
}

struct DrawCallCountRestore(Option<u64>);

impl Drop for DrawCallCountRestore {
    fn drop(&mut self) {
        DRAW_CALL_COUNT.with(|count| count.set(self.0));
    }
}

/// Issue one wgpu draw operation and include it in an active T9 sample.
#[macro_export]
macro_rules! counted_draw {
    ($pass:ident, draw, $($args:tt)*) => {{
        $crate::performance::record_draw_call();
        $pass.draw($($args)*);
    }};
    ($pass:ident, draw_indexed, $($args:tt)*) => {{
        $crate::performance::record_draw_call();
        $pass.draw_indexed($($args)*);
    }};
    ($pass:ident, draw_indirect, $($args:tt)*) => {{
        $crate::performance::record_draw_call();
        $pass.draw_indirect($($args)*);
    }};
    ($pass:ident, draw_indexed_indirect, $($args:tt)*) => {{
        $crate::performance::record_draw_call();
        $pass.draw_indexed_indirect($($args)*);
    }};
}
