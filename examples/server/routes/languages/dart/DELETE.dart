import 'dart_language_service.dart';

Map<String, Object?> handler(Map<String, dynamic> request) {
  return DartLanguageService().delete(request);
}
