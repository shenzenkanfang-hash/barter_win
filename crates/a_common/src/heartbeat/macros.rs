#[macro_export]
macro_rules! heartbeat {
    ($token:expr, $point_id:expr) => {{
        $crate::heartbeat_reporter::global()
            .report($token, $point_id, module_path!(), function_name!(), file!())
            .await;
    }};
}

#[macro_export]
macro_rules! heartbeat_with_info {
    ($token:expr, $point_id:expr, $module:expr, $function:expr, $file:expr) => {{
        $crate::heartbeat_reporter::global()
            .report($token, $point_id, $module, $function, $file)
            .await;
    }};
}

/// 获取当前函数名 (兼容 stable)
#[macro_export]
macro_rules! function_name {
    () => {{
        fn f() {}
        fn type_name<T>(_: &T) -> &'static str { std::any::type_name::<T>() }
        let name = type_name(&f);
        &name[5..name.len() - 1]
    }};
}
