class DartLanguageService {
  Map<String, Object?> delete(Map<String, dynamic> request) {
    final context = request['context'];
    return {
      'message': 'Hello from Dart!',
      'requestId': context is Map ? context['requestId'] : 'unknown',
    };
  }
}
