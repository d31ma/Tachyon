use super::csharp_browser_bindings::CsharpBrowserBindings;

#[test]
fn csharp_http_bindings_capture_only_qualified_runtime_namespaces_and_typed_errors() {
    let authored = "namespace TachyonService0 { internal static class BehaviorBrowserHttp {} }\nnamespace TachyonService1 { class PlainService {} }\nnamespace TachyonService2 { internal static class BehaviorBrowserHttp {} }";
    let bindings = CsharpBrowserBindings::read(authored);
    assert!(
        bindings
            .contains("TachyonService0.BehaviorBrowserHttp.Capture ??= Tac.CaptureHttpTransport")
    );
    assert!(
        bindings
            .contains("TachyonService2.BehaviorBrowserHttp.Capture ??= Tac.CaptureHttpTransport")
    );
    assert!(!bindings.contains("TachyonService1.BehaviorBrowserHttp"));
    assert!(bindings.contains("TachyonService0.BehaviorError failure0 => failure0.Code"));
    assert!(!bindings.contains("GetProperty"));
    assert!(!bindings.contains("GetType"));
}

#[test]
fn fsharp_detached_http_bindings_keep_captured_namespace_indices_and_identity() {
    let authored = "namespace TachyonService0 { class PlainService {} }\nnamespace TachyonService1 { class AsyncService {} }\nnamespace TachyonService2 { internal static class BehaviorBrowserHttp {} }";
    let runtime = "namespace TachyonService1 { internal static class BehaviorBrowserHttp {} }\nnamespace TachyonService2 { internal static class BehaviorBrowserHttp {} }\nnamespace TachyonService7Injected { internal static class BehaviorBrowserHttp {} }";
    let bindings = CsharpBrowserBindings::read_with_runtime(authored, runtime);
    assert!(
        bindings
            .contains("TachyonService1.BehaviorBrowserHttp.Capture ??= Tac.CaptureHttpTransport")
    );
    assert!(bindings.contains("TachyonService1.BehaviorError failure1 => failure1.Code"));
    assert_eq!(
        bindings
            .matches("TachyonService2.BehaviorBrowserHttp.Capture")
            .count(),
        1
    );
    assert!(!bindings.contains("TachyonService0.BehaviorBrowserHttp"));
    assert!(!bindings.contains("TachyonService7.BehaviorBrowserHttp"));
    assert!(!bindings.contains("GetType"));
}

#[test]
fn csharp_socket_bindings_reach_only_namespaces_that_declare_the_socket_sender() {
    let authored = "namespace TachyonService0 { internal static class BehaviorBrowserHttp {} internal static class BehaviorBrowserSocket {} }\nnamespace TachyonService1 { internal static class BehaviorBrowserHttp {} }";
    let bindings = CsharpBrowserBindings::read(authored);
    assert!(bindings.contains("TachyonService0.BehaviorBrowserSocket.Host ??= Tac.SendSocket;"));
    assert!(!bindings.contains("TachyonService1.BehaviorBrowserSocket"));
    assert!(
        bindings
            .contains("TachyonService1.BehaviorBrowserHttp.Capture ??= Tac.CaptureHttpTransport")
    );
}
