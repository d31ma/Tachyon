// Appended to a tac.dart companion. The author writes plain Dart and declares
// which members the island may reach; everything below is the ABI of ADR 0011.
//
// Dart has no reflection in a wasm build, so the members cannot be discovered:
// they are declared once, in `tac`, as closures over the author's own
// variables and functions.

/// One readable, optionally writable member of a companion.
class TacField {
  final Object? Function() read;
  final void Function(Object? value)? write;
  TacField(this.read, [this.write]);
}

/// One callable member of a companion.
class TacMethod {
  final Object? Function(List<Object?> arguments) invoke;
  TacMethod(this.invoke);
}

String _tacRespond(Object? value) => jsonEncode({'value': value});

String _tacFail(String message) => jsonEncode({'error': message});

String _tacHandle(String raw) {
  try {
    final request = jsonDecode(raw) as Map<String, Object?>;
    final operation = request['op'];
    if (operation == 'init') {
      return _tacRespond({
        'fields': [
          for (final member in tac.entries)
            if (member.value is TacField) member.key,
        ],
        'methods': [
          for (final member in tac.entries)
            if (member.value is TacMethod) member.key,
        ],
      });
    }
    final name = request['name'] as String?;
    final member = name == null ? null : tac[name];
    if (member == null) return _tacFail('Unknown companion member: $name');
    if (operation == 'get' && member is TacField) {
      return _tacRespond(member.read());
    }
    if (operation == 'set' && member is TacField) {
      final write = member.write;
      if (write == null) return _tacFail('Companion field is read-only: $name');
      write(request['value']);
      return _tacRespond(null);
    }
    if (operation == 'call' && member is TacMethod) {
      return _tacRespond(member.invoke((request['args'] as List?) ?? const []));
    }
    return _tacFail('Companion member does not support $operation: $name');
  } catch (error) {
    return _tacFail('$error');
  }
}

// dart2wasm reserves wasm exports for its own use, so the entry point is
// installed as a JavaScript function under the name the host passes to main.
void main(List<String> arguments) {
  globalContext.setProperty(
    arguments.first.toJS,
    ((JSString request) => _tacHandle(request.toDart).toJS).toJS,
  );
}
