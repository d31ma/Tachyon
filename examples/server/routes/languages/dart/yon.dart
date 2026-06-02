import 'dart_language_service.dart';

class Handler {
  static final DartLanguageService _service = DartLanguageService();
  static final Map<String, Map<String, Object?>> _responses = {
    '429': {'code': '429', 'detail': 'too many requests'},
    '431': {'code': '431', 'detail': 'header too large'},
    '451': {'code': '451', 'detail': 'unavailable legal'},
    '500': {'code': '500', 'detail': 'internal error'},
    '501': {'code': '501', 'detail': 'not implemented'},
  };

  static Future<Map<String, Object?>> DELETE(Map<String, dynamic> request) async {
    final response = _responses[_statusCode(request)];
    if (response != null) {
      return response;
    }

    return await _service.delete(request);
  }

  static String _statusCode(Map<String, dynamic> request) {
    final query = request['query'];
    final raw = query is Map ? query['code'] : null;
    return raw is num ? raw.floor().toString() : (raw?.toString() ?? '');
  }
}
