pub(super) struct DartHttpScope;

impl DartHttpScope {
    const OWNERS: &'static str = "  static final Map<int, BehaviorSignal> _owners = {};\n";
    const CREATE: &'static str = "final owner = BehaviorSignal();\n      _owners[_next] = owner;\n      try {\n        _live[_next] = BehaviorBrowserHttp.withOwner(owner, () => make_{class}(<String, Object>{if (socket != null) 'tachyon.socket': socket}, properties, publish));\n      } catch (_) {\n        _owners.remove(_next)?.abort();\n        rethrow;\n      }";
    const DEFAULT_CREATE: &'static str = "_live[_next] = make_{class}(<String, Object>{if (socket != null) 'tachyon.socket': socket}, properties, publish);";

    pub(super) fn owners(applies: bool) -> &'static str {
        ["", Self::OWNERS][usize::from(applies)]
    }

    pub(super) fn create(applies: bool, class: &str) -> String {
        [Self::DEFAULT_CREATE, Self::CREATE][usize::from(applies)].replace("{class}", class)
    }

    pub(super) fn dispose(applies: bool) -> &'static str {
        ["", "_owners.remove(handle.toDartInt)?.abort();"][usize::from(applies)]
    }

    pub(super) fn run(applies: bool) -> &'static str {
        [
            "return _run(operation, handle, member, arguments);",
            "return BehaviorBrowserHttp.withOwner(_owners[handle.toDartInt], () => _run(operation, handle, member, arguments));",
        ][usize::from(applies)]
    }

    pub(super) fn error_code(applies: bool) -> &'static str {
        [
            "error is TacSocketError ? error.code : 'DART_FAILED'",
            "error is BehaviorError ? error.code : error is TacSocketError ? error.code : 'DART_FAILED'",
        ][usize::from(applies)]
    }
}
