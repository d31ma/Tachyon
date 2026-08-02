int count = 6;
String label = 'from Dart';

int doubled() => count * 2;

final tac = {
  'count': TacField(() => count, (value) => count = value as int),
  'label': TacField(() => label),
  'doubled': TacMethod((arguments) => doubled()),
};
