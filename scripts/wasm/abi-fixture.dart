// A Dart browser companion, written as plain Dart. There is no tac_invoke
// here and no subset of the language: this is compiled by `dart compile wasm`
// and the prelude the build appends carries the ABI of ADR 0011.
//
// Members are declared because a wasm build of Dart has no reflection to
// discover them with, and the host must know a field from a method.

int count = 6;
String label = 'from dart';

int doubled() => count * 2;

final tac = {
  'count': TacField(() => count, (value) => count = value as int),
  'label': TacField(() => label),
  'doubled': TacMethod((arguments) => doubled()),
};
